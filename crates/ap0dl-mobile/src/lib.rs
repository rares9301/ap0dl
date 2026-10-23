//! Android JNI bridge for the ap0dl engine.
//!
//! The Kotlin layer is intentionally thin: every provider, extraction, HLS,
//! Cloudflare and playback decision lives in `ap0dl-core`. Values cross the
//! JNI boundary as JSON so the UI can evolve without a fragile native ABI.
#![allow(non_snake_case)]

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ap0dl_core::config::Config;
use ap0dl_core::download::{DownloadOptions, DownloadProgress, ProgressCallback, download_episode};
use ap0dl_core::models::{Anime, Audio, Chapter, Episode, SearchResult, StreamKind, Subtitle};
use ap0dl_core::provider::{Provider, ProviderInfo};
use ap0dl_core::proxy::{FlixProxy, RangeProxy};
use ap0dl_core::quality;
use ap0dl_core::{Engine, Error};
use jni::JNIEnv;
use jni::objects::{JObject, JString};
use jni::sys::{jint, jstring};
use serde::Deserialize;
use serde::Serialize;

/// The playback proxy is only useful while one player is open. Keeping the
/// newest one here lets the proxy task die when the user picks another quality
/// or closes the app.
enum PlaybackProxy {
    Flix(FlixProxy),
    Range(RangeProxy),
}

static PLAYBACK_PROXY: Mutex<Option<PlaybackProxy>> = Mutex::new(None);

static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
static DOWNLOADS: OnceLock<Mutex<HashMap<String, DownloadStatus>>> = OnceLock::new();
static DOWNLOAD_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    match RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("ap0dl-net")
            .build()
            .map_err(|error| format!("could not start network runtime: {error}"))
    }) {
        Ok(runtime) => Ok(runtime),
        Err(error) => Err(error.clone()),
    }
}

fn downloads() -> &'static Mutex<HashMap<String, DownloadStatus>> {
    DOWNLOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn engine() -> Result<&'static Engine, String> {
    match ENGINE.get_or_init(|| {
        let config = Config {
            timeout_secs: 45,
            request_delay_ms: 80,
            quality: "best".to_string(),
            ..Config::default()
        };
        Engine::new(config).map_err(|error| format!("could not start engine: {error}"))
    }) {
        Ok(engine) => Ok(engine),
        Err(error) => Err(error.clone()),
    }
}

fn json_ok<T: Serialize>(data: T) -> String {
    serde_json::json!({ "ok": true, "data": data }).to_string()
}

fn json_error(error: impl std::fmt::Display) -> String {
    serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
}

fn required_string(env: &mut JNIEnv, value: &JString) -> Result<String, String> {
    env.get_string(value)
        .map(Into::into)
        .map_err(|error| format!("invalid JNI string: {error}"))
}

fn optional_string(env: &mut JNIEnv, value: &JString) -> Option<String> {
    if value.as_raw().is_null() {
        return None;
    }
    env.get_string(value)
        .ok()
        .map(Into::into)
        .filter(|value: &String| !value.trim().is_empty())
}

fn to_jstring(env: &mut JNIEnv, value: String) -> jstring {
    env.new_string(value)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[derive(Serialize)]
struct SearchHit {
    #[serde(flatten)]
    result: SearchResult,
    provider_name: String,
}

#[derive(Serialize)]
struct DetailsResponse {
    anime: Anime,
    episodes: Vec<Episode>,
    provider: ProviderInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadStatus {
    id: String,
    anime_title: String,
    episode_number: f32,
    episode_title: Option<String>,
    provider: String,
    cover: Option<String>,
    status: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    segments_done: Option<u64>,
    segments_total: Option<u64>,
    path: Option<String>,
    subtitles: Vec<String>,
    error: Option<String>,
    audio: Option<String>,
    audio_fallback: bool,
}

#[derive(Deserialize)]
struct DownloadRequest {
    anime: Anime,
    episode: Episode,
    directory: String,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default)]
    audio: Option<String>,
}

#[derive(Serialize)]
struct TaskId {
    task_id: String,
}

#[derive(Serialize)]
struct Playback {
    url: String,
    kind: StreamKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<u32>,
    qualities: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    audio: Audio,
    audios: Vec<String>,
    requested_audio: String,
    audio_fallback: bool,
    headers: BTreeMap<String, String>,
    subtitles: Vec<Subtitle>,
    chapters: Vec<Chapter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    container: Option<String>,
    /// True when `url` points at the on-device flixcloud proxy rather than
    /// the upstream CDN.
    proxied: bool,
    /// Best-effort MIME hint for URLs that do not carry an extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
}

fn mime_for(stream: &ap0dl_core::Stream) -> Option<String> {
    match stream.kind {
        StreamKind::Hls => Some("application/x-mpegURL".to_string()),
        StreamKind::Dash => Some("application/dash+xml".to_string()),
        StreamKind::Direct => {
            let value = stream
                .container
                .as_deref()
                .or_else(|| stream.url.split(['?', '#']).next())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.ends_with("mkv") {
                Some("video/x-matroska".to_string())
            } else if value.ends_with("webm") {
                Some("video/webm".to_string())
            } else if value.ends_with("mov") {
                Some("video/quicktime".to_string())
            } else if value.ends_with("m4v") || value.ends_with("mp4") {
                Some("video/mp4".to_string())
            } else {
                None
            }
        }
    }
}

fn audio_matches(audio: Audio, preference: &str) -> bool {
    match preference.to_ascii_lowercase().as_str() {
        "sub" => matches!(audio, Audio::Sub | Audio::Both),
        "dub" => matches!(audio, Audio::Dub | Audio::Both),
        "raw" => matches!(audio, Audio::Raw),
        _ => true,
    }
}

fn audios_of(streams: &[ap0dl_core::Stream]) -> Vec<String> {
    let mut audios: Vec<String> = streams
        .iter()
        .map(|stream| stream.audio.label().to_string())
        .collect();
    audios.sort();
    audios.dedup();
    audios
}

/// Filter by the requested audio, then pick the best quality inside that
/// audio family. If the source has no matching audio, fall back to every
/// stream and report the fallback so the UI can explain it.
fn choose_stream(
    streams: &[ap0dl_core::Stream],
    quality_spec: &str,
    audio_spec: &str,
) -> Result<(ap0dl_core::Stream, Vec<u32>, bool), String> {
    if streams.is_empty() {
        return Err("no playable streams were returned".to_string());
    }
    let filtered: Vec<ap0dl_core::Stream> = streams
        .iter()
        .filter(|stream| audio_matches(stream.audio, audio_spec))
        .cloned()
        .collect();
    let fallback = filtered.is_empty();
    let pool = if fallback { streams.to_vec() } else { filtered };
    let qualities = qualities_of(&pool);
    let selected = quality::select(&pool, quality_spec)
        .into_iter()
        .next()
        .cloned()
        .ok_or_else(|| "no playable streams were returned".to_string())?;
    Ok((selected, qualities, fallback))
}

fn qualities_of(streams: &[ap0dl_core::Stream]) -> Vec<u32> {
    let mut qualities: Vec<u32> = streams.iter().filter_map(|stream| stream.quality).collect();
    qualities.sort_unstable_by(|left, right| right.cmp(left));
    qualities.dedup();
    qualities
}

fn search_hits(hits: Vec<(std::sync::Arc<dyn Provider>, SearchResult)>) -> Vec<SearchHit> {
    hits.into_iter()
        .map(|(provider, result)| SearchHit {
            provider_name: provider.name().to_string(),
            result,
        })
        .collect()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeProviders(
    mut env: JNIEnv,
    _this: JObject,
) -> jstring {
    let response = (|| {
        let engine = engine()?;
        Ok::<_, String>(json_ok(engine.provider_infos()))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeLatest(
    mut env: JNIEnv,
    _this: JObject,
    provider: JString,
) -> jstring {
    let response = (|| {
        let requested = optional_string(&mut env, &provider).unwrap_or_else(|| "reanime".into());
        let engine = engine()?;
        let runtime = runtime()?;
        let result = runtime.block_on(async {
            let mut ids = Vec::new();
            if requested.eq_ignore_ascii_case("all") {
                ids.extend(
                    ["reanime", "anidb", "kisskh", "animegg"]
                        .into_iter()
                        .map(str::to_string),
                );
            } else {
                ids.push(requested);
            }
            // A quiet fallback keeps Home useful when one upstream is having
            // a bad day, without hiding the provider name from the UI.
            if !ids.iter().any(|id| id == "reanime") {
                ids.push("reanime".to_string());
            }
            if !ids.iter().any(|id| id == "anidb") {
                ids.push("anidb".to_string());
            }

            let mut last_error = None;
            for id in ids {
                let Some(provider) = engine.get(&id) else {
                    continue;
                };
                match tokio::time::timeout(
                    Duration::from_secs(12),
                    provider.latest(&engine.http, 1),
                )
                .await
                {
                    Ok(Ok(results)) if !results.is_empty() => {
                        return Ok::<_, String>(search_hits(
                            results
                                .into_iter()
                                .map(|result| (provider.clone(), result))
                                .collect(),
                        ));
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => last_error = Some(error.to_string()),
                    Err(_) => last_error = Some(format!("{id} timed out")),
                }
            }
            Err(last_error.unwrap_or_else(|| "no provider returned a latest feed".to_string()))
        })?;
        Ok::<_, String>(json_ok(result))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeSearch(
    mut env: JNIEnv,
    _this: JObject,
    query: JString,
    provider: JString,
    limit: jint,
) -> jstring {
    let response = (|| {
        let query = required_string(&mut env, &query)?;
        let provider = optional_string(&mut env, &provider);
        let limit = limit.clamp(1, 60) as usize;
        let engine = engine()?;
        let runtime = runtime()?;
        let hits = runtime
            .block_on(engine.search(provider.as_deref(), &query, 1, limit))
            .map_err(|error| match error {
                Error::NoResults(_) => format!("No results for \"{query}\""),
                other => other.to_string(),
            })?;
        Ok::<_, String>(json_ok(search_hits(hits)))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeDetails(
    mut env: JNIEnv,
    _this: JObject,
    url: JString,
    provider: JString,
) -> jstring {
    let response = (|| {
        let url = required_string(&mut env, &url)?;
        let provider = optional_string(&mut env, &provider);
        let engine = engine()?;
        let runtime = runtime()?;
        let details = runtime.block_on(async {
            let provider = match provider.as_deref() {
                Some(id) => engine
                    .get(id)
                    .ok_or_else(|| format!("unknown provider {id}")),
                None => engine.by_url(&url).map_err(|error| error.to_string()),
            }?;
            let anime = provider
                .details(&engine.http, &url)
                .await
                .map_err(|error| error.to_string())?;
            let (episodes, episode_error) = match provider.episodes(&engine.http, &anime).await {
                Ok(episodes) => (episodes, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };
            Ok::<_, String>(DetailsResponse {
                provider: provider.info(),
                anime,
                episodes,
                episode_error,
            })
        })?;
        Ok::<_, String>(json_ok(details))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeStreams(
    mut env: JNIEnv,
    _this: JObject,
    anime_json: JString,
    episode_json: JString,
    quality: JString,
    audio: JString,
) -> jstring {
    let response = (|| {
        let anime_json = required_string(&mut env, &anime_json)?;
        let episode_json = required_string(&mut env, &episode_json)?;
        let quality = optional_string(&mut env, &quality).unwrap_or_else(|| "best".to_string());
        let audio = optional_string(&mut env, &audio).unwrap_or_else(|| "sub".to_string());
        let anime: Anime = serde_json::from_str(&anime_json)
            .map_err(|error| format!("invalid anime payload: {error}"))?;
        let episode: Episode = serde_json::from_str(&episode_json)
            .map_err(|error| format!("invalid episode payload: {error}"))?;
        let engine = engine()?;
        let runtime = runtime()?;
        let streams = runtime.block_on(async {
            let provider = engine
                .get(&anime.provider)
                .or_else(|| engine.by_url(&anime.url).ok())
                .ok_or_else(|| format!("unknown provider {}", anime.provider))?;
            provider
                .streams(&engine.http, &anime, &episode)
                .await
                .map_err(|error| error.to_string())
        })?;
        let audios = audios_of(&streams);
        let (selected, qualities, audio_fallback) = choose_stream(&streams, &quality, &audio)?;

        let proxy = if let Some(info) = &selected.flix {
            Some(PlaybackProxy::Flix(
                runtime
                    .block_on(FlixProxy::start(info.clone()))
                    .map_err(|error| format!("could not start playback proxy: {error}"))?,
            ))
        } else if selected.url.contains("cloud.animetoki.com") {
            Some(PlaybackProxy::Range(
                runtime
                    .block_on(RangeProxy::start(
                        selected.url.clone(),
                        selected.headers.clone(),
                    ))
                    .map_err(|error| error.to_string())?,
            ))
        } else {
            None
        };
        let url = match &proxy {
            Some(PlaybackProxy::Flix(proxy)) => proxy.url(),
            Some(PlaybackProxy::Range(proxy)) => proxy.url(),
            None => selected.url.clone(),
        };
        if let Some(proxy) = proxy {
            *PLAYBACK_PROXY
                .lock()
                .map_err(|_| "playback proxy lock poisoned".to_string())? = Some(proxy);
        }

        Ok::<_, String>(json_ok(Playback {
            url,
            kind: selected.kind,
            quality: selected.quality,
            qualities,
            title: selected.title.clone(),
            audio: selected.audio,
            audios,
            requested_audio: audio.clone(),
            audio_fallback,
            headers: selected.headers.clone(),
            subtitles: selected.subtitles.clone(),
            chapters: selected.chapters.clone(),
            container: selected.container.clone(),
            proxied: selected.flix.is_some() || selected.url.contains("cloud.animetoki.com"),
            mime_type: mime_for(&selected),
        }))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeDownload(
    mut env: JNIEnv,
    _this: JObject,
    anime_json: JString,
    episode_json: JString,
    directory: JString,
    quality: JString,
    audio: JString,
) -> jstring {
    let response = (|| {
        let anime_json = required_string(&mut env, &anime_json)?;
        let episode_json = required_string(&mut env, &episode_json)?;
        let directory = required_string(&mut env, &directory)?;
        let quality = optional_string(&mut env, &quality);
        let audio = optional_string(&mut env, &audio).unwrap_or_else(|| "sub".to_string());
        let anime: Anime = serde_json::from_str(&anime_json)
            .map_err(|error| format!("invalid anime payload: {error}"))?;
        let episode: Episode = serde_json::from_str(&episode_json)
            .map_err(|error| format!("invalid episode payload: {error}"))?;
        let engine = engine()?;
        let runtime = runtime()?;
        let request = DownloadRequest {
            anime,
            episode,
            directory,
            quality,
            audio: Some(audio),
        };

        let task_id = format!(
            "dl-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default(),
            DOWNLOAD_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        downloads()
            .lock()
            .map_err(|_| "download state lock poisoned".to_string())?
            .insert(
                task_id.clone(),
                DownloadStatus {
                    id: task_id.clone(),
                    anime_title: request.anime.title.clone(),
                    episode_number: request.episode.number,
                    episode_title: request.episode.title.clone(),
                    provider: request.anime.provider.clone(),
                    cover: request.anime.cover.clone(),
                    status: "queued".to_string(),
                    downloaded_bytes: 0,
                    total_bytes: None,
                    segments_done: None,
                    segments_total: None,
                    path: None,
                    subtitles: Vec::new(),
                    error: None,
                    audio: request.audio.clone(),
                    audio_fallback: false,
                },
            );

        let spawned_id = task_id.clone();
        runtime.spawn(async move {
            let result = run_download(engine, request, spawned_id.clone()).await;
            if let Err(error) = result {
                update_download(&spawned_id, |status| {
                    status.status = "failed".to_string();
                    status.error = Some(error);
                });
            }
        });

        Ok::<_, String>(json_ok(TaskId { task_id }))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

async fn run_download(
    engine: &'static Engine,
    request: DownloadRequest,
    task_id: String,
) -> Result<(), String> {
    update_download(&task_id, |status| {
        status.status = "resolving".to_string();
    });

    let provider = engine
        .get(&request.anime.provider)
        .or_else(|| engine.by_url(&request.anime.url).ok())
        .ok_or_else(|| format!("unknown provider {}", request.anime.provider))?;
    let streams = provider
        .streams(&engine.http, &request.anime, &request.episode)
        .await
        .map_err(|error| error.to_string())?;
    let spec = request.quality.as_deref().unwrap_or("best");
    let (mut selected, _qualities, audio_fallback) =
        choose_stream(&streams, spec, request.audio.as_deref().unwrap_or("sub"))?;
    update_download(&task_id, |status| {
        status.audio = Some(selected.audio.label().to_string());
        status.audio_fallback = audio_fallback;
    });

    // The AnimeToki cloud rejects unprotected/open-ended downloads. Its
    // local range proxy presents a normal 200 stream to the downloader.
    let range_proxy = if selected.url.contains("cloud.animetoki.com") {
        Some(
            RangeProxy::start(selected.url.clone(), selected.headers.clone())
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    if let Some(proxy) = &range_proxy {
        selected.url = proxy.url();
        selected.headers.clear();
        selected.flix = None;
    }

    update_download(&task_id, |status| {
        status.status = "downloading".to_string();
    });

    let directory = PathBuf::from(&request.directory);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| format!("could not create download folder: {error}"))?;

    let progress_id = task_id.clone();
    let callback = ProgressCallback::new(move |progress: DownloadProgress| {
        update_download(&progress_id, |status| {
            status.downloaded_bytes = progress.downloaded_bytes;
            if progress.total_bytes.is_some() {
                status.total_bytes = progress.total_bytes;
            }
            if progress.segments_done.is_some() {
                status.segments_done = progress.segments_done;
                status.segments_total = progress.segments_total;
            }
        });
    });

    let options = DownloadOptions {
        ffmpeg: "ffmpeg".to_string(),
        merge_video: false,
        merge_subtitles: true,
        xor_mask: Vec::new(),
        progress: Some(callback),
    };
    let downloaded = download_episode(
        &engine.http,
        &selected,
        &request.anime.title,
        request.episode.number,
        &directory,
        &options,
    )
    .await
    .map_err(|error| error.to_string())?;

    update_download(&task_id, |status| {
        status.status = "completed".to_string();
        status.downloaded_bytes = std::fs::metadata(&downloaded.path)
            .map(|metadata| metadata.len())
            .unwrap_or(status.downloaded_bytes);
        status.path = Some(downloaded.path.to_string_lossy().to_string());
        status.subtitles = downloaded
            .subtitles
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
    });
    Ok(())
}

fn update_download(id: &str, update: impl FnOnce(&mut DownloadStatus)) {
    if let Ok(mut map) = downloads().lock() {
        if let Some(status) = map.get_mut(id) {
            update(status);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeDownloads(
    mut env: JNIEnv,
    _this: JObject,
) -> jstring {
    let response = (|| {
        let mut values: Vec<DownloadStatus> = downloads()
            .lock()
            .map_err(|_| "download state lock poisoned".to_string())?
            .values()
            .cloned()
            .collect();
        values.sort_by(|left, right| right.id.cmp(&left.id));
        Ok::<_, String>(json_ok(values))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_ap0dl_anyomi_MobileEngine_nativeDownloadRemove(
    mut env: JNIEnv,
    _this: JObject,
    task_id: JString,
) -> jstring {
    let response = (|| {
        let task_id = required_string(&mut env, &task_id)?;
        downloads()
            .lock()
            .map_err(|_| "download state lock poisoned".to_string())?
            .remove(&task_id);
        Ok::<_, String>(json_ok(true))
    })()
    .unwrap_or_else(json_error);
    to_jstring(&mut env, response)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(_vm: jni::JavaVM, _reserved: *mut std::ffi::c_void) -> jint {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("ap0dl"),
    );
    jni::sys::JNI_VERSION_1_6
}
