use std::path::{Path, PathBuf};

use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::header::HeaderMap;
use url::Url;

use crate::error::{Error, Result};
use crate::http::Http;

type Aes128CbcDec = cbc::Decryptor<Aes128>;

#[derive(Debug, Clone)]
pub struct HlsOptions {
    pub ffmpeg: String,
    pub merge: bool,
    pub xor_mask: Vec<u8>,
}

impl Default for HlsOptions {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".to_string(),
            merge: true,
            xor_mask: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct Variant {
    uri: String,
    quality: Option<u32>,
}

#[derive(Debug, Clone)]
struct Key {
    uri: String,
    iv: Option<[u8; 16]>,
}

#[derive(Debug, Clone)]
struct Segment {
    uri: String,
}

#[derive(Debug, Default)]
struct MediaPlaylist {
    segments: Vec<Segment>,
    key: Option<Key>,
    media_sequence: u64,
    init: Option<String>,
}

/// Downloads an HLS stream to a local file. Handles master playlists,
/// AES-128 keys, flixcloud fake image headers and XOR-masked segments.
pub async fn download_hls(
    http: &Http,
    url: &str,
    headers: &HeaderMap,
    destination: &Path,
    options: &HlsOptions,
) -> Result<PathBuf> {
    let text = http.get_text(url, headers).await?;

    if text.contains("#EXT-X-STREAM-INF") {
        let variants = parse_master(&text, url)?;
        let Some(variant) = variants
            .into_iter()
            .max_by_key(|variant| variant.quality.unwrap_or(0))
        else {
            return Err(Error::Playlist(
                url.to_string(),
                "master playlist has no variants".to_string(),
            ));
        };
        let downloaded = Box::pin(download_hls(
            http,
            &variant.uri,
            headers,
            destination,
            options,
        ))
        .await?;
        return Ok(downloaded);
    }

    let playlist = parse_media(&text, url)?;
    if playlist.segments.is_empty() {
        return Err(Error::Playlist(
            url.to_string(),
            "media playlist has no segments".to_string(),
        ));
    }

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let media_path = destination.with_extension("ts");
    let index_path = destination.with_extension("ts.idx");
    let mut start_index = read_index(&index_path).await.unwrap_or(0);
    if start_index > playlist.segments.len() {
        start_index = 0;
    }

    let key = match &playlist.key {
        Some(key) => Some(
            http.get_bytes(&key.uri, headers)
                .await
                .map_err(|error| Error::download(format!("could not fetch HLS key: {error}")))?,
        ),
        None => None,
    };

    let mut file = if start_index == 0 {
        tokio::fs::File::create(&media_path).await?
    } else {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&media_path)
            .await?
    };

    if start_index == 0 {
        if let Some(init) = &playlist.init {
            let init_data = http.get_bytes(init, headers).await?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &init_data).await?;
        }
    }

    let progress = ProgressBar::new(playlist.segments.len() as u64);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {msg} [{bar:36.cyan/blue}] {pos}/{len} segments ({eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=>-"),
    );
    progress.set_message("HLS download");
    progress.enable_steady_tick(std::time::Duration::from_millis(250));
    progress.set_position(start_index as u64);
    let mut downloaded_bytes = 0u64;

    for (index, segment) in playlist.segments.iter().enumerate().skip(start_index) {
        let data = http.get_bytes(&segment.uri, headers).await?;
        let iv = key
            .as_ref()
            .and(playlist.key.as_ref())
            .and_then(|key| key.iv)
            .unwrap_or_else(|| default_iv(playlist.media_sequence + index as u64));
        let decoded = decode_segment(&data, key.as_deref(), &iv, &options.xor_mask);
        tokio::io::AsyncWriteExt::write_all(&mut file, &decoded).await?;
        write_index(&index_path, index + 1).await?;
        downloaded_bytes += decoded.len() as u64;
        progress.set_message(format!("HLS {}", indicatif::HumanBytes(downloaded_bytes)));
        progress.set_position((index + 1) as u64);
    }
    progress.finish_and_clear();

    let output = if options.merge && which::which(&options.ffmpeg).is_ok() {
        merge_ts(&options.ffmpeg, &media_path, destination).await?
    } else {
        media_path.clone()
    };
    let _ = tokio::fs::remove_file(&index_path).await;
    Ok(output)
}

async fn merge_ts(ffmpeg: &str, input: &Path, destination: &Path) -> Result<PathBuf> {
    let output = destination.with_extension("mp4");
    let status = tokio::process::Command::new(ffmpeg)
        .arg("-y")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(input)
        .arg("-c")
        .arg("copy")
        .arg("-bsf:a")
        .arg("aac_adtstoasc")
        .arg(&output)
        .status()
        .await
        .map_err(|error| Error::download(format!("could not run ffmpeg: {error}")))?;
    if status.success() {
        let _ = tokio::fs::remove_file(input).await;
        Ok(output)
    } else {
        Ok(input.to_path_buf())
    }
}

fn parse_master(text: &str, base: &str) -> Result<Vec<Variant>> {
    let mut variants = Vec::new();
    let mut pending: Option<u32> = None;
    for line in text.lines().map(str::trim) {
        if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending = attr_value(attrs, "RESOLUTION")
                .and_then(|resolution| {
                    resolution
                        .rsplit_once('x')
                        .map(|(_, height)| height.to_string())
                })
                .and_then(|height| height.parse::<u32>().ok());
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if pending.is_some() || variants.is_empty() {
            variants.push(Variant {
                uri: resolve(base, line)?,
                quality: pending.take(),
            });
        }
    }
    Ok(variants)
}

fn parse_media(text: &str, base: &str) -> Result<MediaPlaylist> {
    let mut playlist = MediaPlaylist::default();
    let mut pending_segment = false;
    let iv_regex = Regex::new(r"(?i)IV=0x([0-9a-f]+)").unwrap();

    for line in text.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            playlist.media_sequence = value.trim().parse().unwrap_or(0);
            continue;
        }
        if let Some(attrs) = line.strip_prefix("#EXT-X-KEY:") {
            let method = attr_value(attrs, "METHOD").unwrap_or_default();
            if method.eq_ignore_ascii_case("NONE") {
                playlist.key = None;
                continue;
            }
            if method.eq_ignore_ascii_case("AES-128") {
                if let Some(uri) = attr_value(attrs, "URI") {
                    let iv = iv_regex
                        .captures(attrs)
                        .and_then(|captures| hex_to_iv(captures.get(1)?.as_str()));
                    playlist.key = Some(Key {
                        uri: resolve(base, &uri)?,
                        iv,
                    });
                }
            }
            continue;
        }
        if let Some(attrs) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = attr_value(attrs, "URI") {
                playlist.init = Some(resolve(base, &uri)?);
            }
            continue;
        }
        if line.starts_with("#EXTINF:") {
            pending_segment = true;
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if pending_segment {
            playlist.segments.push(Segment {
                uri: resolve(base, line)?,
            });
            pending_segment = false;
        }
    }
    Ok(playlist)
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let pattern = format!(r#"(?i)(?:^|,)\s*{name}=("([^"]*)"|[^,]*)"#);
    let regex = Regex::new(&pattern).ok()?;
    let captures = regex.captures(attrs)?;
    let value = captures.get(2).or_else(|| captures.get(1))?.as_str();
    Some(value.trim().trim_matches('"').to_string())
}

fn resolve(base: &str, value: &str) -> Result<String> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(value.to_string());
    }
    if value.starts_with("//") {
        return Ok(format!("https:{value}"));
    }
    Url::parse(base)
        .and_then(|base| base.join(value))
        .map(|url| url.to_string())
        .map_err(|error| Error::Url(base.to_string(), error))
}

fn hex_to_iv(value: &str) -> Option<[u8; 16]> {
    let value = value.trim_start_matches("0x");
    let padded = format!("{value:0>32}");
    let bytes = (0..16)
        .map(|index| u8::from_str_radix(&padded[index * 2..index * 2 + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    bytes.try_into().ok()
}

fn default_iv(sequence: u64) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[8..].copy_from_slice(&sequence.to_be_bytes());
    iv
}

fn decode_segment(data: &[u8], key: Option<&[u8]>, iv: &[u8; 16], xor_mask: &[u8]) -> Vec<u8> {
    let (skip, masked) = detect_image_header(data);
    let payload = &data[skip..];

    if let Some(key) = key {
        if key.len() == 16 && payload.len() % 16 == 0 {
            let mut buffer = payload.to_vec();
            if let Ok(decryptor) = Aes128CbcDec::new_from_slices(key, iv) {
                if let Ok(plain) = decryptor.decrypt_padded_mut::<Pkcs7>(&mut buffer) {
                    if is_media(plain) {
                        return plain.to_vec();
                    }
                }
            }
        }
    }

    if masked && !xor_mask.is_empty() {
        let mut decoded = payload.to_vec();
        for (index, byte) in decoded.iter_mut().enumerate() {
            *byte ^= xor_mask[index % xor_mask.len()];
        }
        if is_media(&decoded) || key.is_none() {
            return decoded;
        }
    }

    payload.to_vec()
}

fn is_media(data: &[u8]) -> bool {
    data.first().is_some_and(|byte| *byte == 0x47)
        || data.get(4..8).is_some_and(|marker| marker == b"ftyp")
}

fn detect_image_header(data: &[u8]) -> (usize, bool) {
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return (12, true);
    }
    if data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        return (8, true);
    }
    (0, false)
}

async fn read_index(path: &Path) -> Option<usize> {
    let value = tokio::fs::read_to_string(path).await.ok()?;
    value.trim().parse().ok()
}

async fn write_index(path: &Path, value: usize) -> Result<()> {
    tokio::fs::write(path, value.to_string()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_master() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=100,RESOLUTION=1280x720\n720.m3u8\n#EXT-X-STREAM-INF:RESOLUTION=1920x1080\n1080.m3u8\n";
        let variants = parse_master(text, "https://example.test/master.m3u8").unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].quality, Some(720));
        assert_eq!(variants[1].quality, Some(1080));
        assert!(variants[1].uri.ends_with("/1080.m3u8"));
    }

    #[test]
    fn parses_media_with_key() {
        let text = "#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:7\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\",IV=0x00000000000000000000000000000001\n#EXTINF:10,\nseg1.ts\n#EXTINF:9,\nseg2.ts\n";
        let playlist = parse_media(text, "https://example.test/video.m3u8").unwrap();
        assert_eq!(playlist.segments.len(), 2);
        assert_eq!(playlist.media_sequence, 7);
        assert_eq!(
            playlist.key.as_ref().unwrap().uri,
            "https://example.test/key.bin"
        );
        assert_eq!(playlist.key.as_ref().unwrap().iv.unwrap()[15], 1);
        assert!(playlist.segments[0].uri.ends_with("/seg1.ts"));
    }

    #[test]
    fn derives_default_iv() {
        let iv = default_iv(1);
        assert_eq!(iv[15], 1);
        assert_eq!(iv[0], 0);
    }

    #[test]
    fn detects_fake_headers() {
        assert_eq!(detect_image_header(b"\x89PNG\r\n\x1a\nabc"), (8, true));
        assert_eq!(detect_image_header(b"RIFF0000WEBPabc"), (12, true));
        assert_eq!(detect_image_header(b"\x47abc"), (0, false));
    }

    #[test]
    fn decrypts_aes128_segments() {
        use cbc::cipher::{BlockEncryptMut, block_padding::Pkcs7};
        let key = [0x11u8; 16];
        let iv = [0u8; 16];
        let plaintext = vec![0x47u8; 376];
        let encrypted = cbc::Encryptor::<Aes128>::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_vec_mut::<Pkcs7>(&plaintext);
        let decoded = decode_segment(&encrypted, Some(&key), &iv, &[]);
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn strips_image_header_and_xor_mask() {
        let mask = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let plaintext = vec![0x47u8; 376];
        let masked: Vec<u8> = plaintext
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % mask.len()])
            .collect();
        let mut segment = b"\x89PNG\r\n\x1a\n".to_vec();
        segment.extend_from_slice(&masked);
        let decoded = decode_segment(&segment, None, &[0u8; 16], &mask);
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn attr_parsing_handles_quotes() {
        let attrs = r#"METHOD=AES-128,URI="key.bin",IV=0x01"#;
        assert_eq!(attr_value(attrs, "METHOD").as_deref(), Some("AES-128"));
        assert_eq!(attr_value(attrs, "URI").as_deref(), Some("key.bin"));
        assert_eq!(attr_value(attrs, "IV").as_deref(), Some("0x01"));
    }
}
