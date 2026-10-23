use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bytes::Bytes;
use futures::stream::unfold;
use reqwest::Client;
use std::net::SocketAddr;
use url::Url;

use crate::error::{Error, Result};
use crate::models::FlixInfo;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// A tiny local HLS proxy for flixcloud.
///
/// The upstream master playlist is encrypted and the segment/key URLs need
/// `Referer`/`Origin` headers. This proxy:
///
/// - fetches the plaintext playlist through the enc-dec proxy;
/// - rewrites every manifest, key and segment URI to a local URL;
/// - adds the flixcloud headers to upstream requests;
/// - strips fake PNG/WebP segment headers and applies the XOR mask when
///   present, leaving AES-128 ciphertext for the player to decrypt normally.
pub struct FlixProxy {
    addr: SocketAddr,
    handle: tokio::task::JoinHandle<()>,
    info: FlixInfo,
}

impl FlixProxy {
    pub async fn start(info: FlixInfo) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(Error::Io)?;
        let addr = listener.local_addr().map_err(Error::Io)?;
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| Error::config(format!("proxy client: {error}")))?;

        let state = ProxyState {
            client,
            addr,
            info: info.clone(),
        };
        let app = Router::new()
            .route("/proxy", get(handle_proxy))
            .with_state(state);
        let handle = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                tracing::warn!(%error, "flixcloud proxy stopped");
            }
        });

        Ok(Self { addr, handle, info })
    }

    pub fn address(&self) -> SocketAddr {
        self.addr
    }

    /// Local URL for the stream's master playlist.
    pub fn url(&self) -> String {
        self.local_url(&self.info.master_url, &self.info.w_payload, None)
    }

    fn local_url(&self, upstream: &str, w_payload: &str, base: Option<&str>) -> String {
        let mut url = format!(
            "http://{}/proxy?url={}&w_payload={}",
            self.addr,
            urlencoding::encode(upstream),
            urlencoding::encode(w_payload)
        );
        if let Some(base) = base {
            url.push_str("&base=");
            url.push_str(&urlencoding::encode(base));
        }
        url
    }
}

impl Drop for FlixProxy {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[derive(Clone)]
struct ProxyState {
    client: Client,
    addr: SocketAddr,
    info: FlixInfo,
}

#[derive(Debug, serde::Deserialize)]
struct ProxyParams {
    url: String,
    #[serde(default)]
    w_payload: Option<String>,
    #[serde(default)]
    base: Option<String>,
}

async fn handle_proxy(
    State(state): State<ProxyState>,
    Query(params): Query<ProxyParams>,
) -> Response {
    tracing::debug!(url = %params.url, base = ?params.base, manifest = is_manifest(&params.url), key = is_key(&params.url), "proxy request");
    let w_payload = params
        .w_payload
        .unwrap_or_else(|| state.info.w_payload.clone());

    if is_manifest(&params.url) {
        return match fetch_text(&state, &params.url).await {
            Ok(text) => {
                let base = params
                    .base
                    .clone()
                    .or_else(|| decode_proxy_url(&params.url))
                    .unwrap_or_else(|| params.url.clone());
                let rewritten = rewrite_manifest(&state, &text, &base, &w_payload);
                (
                    [(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")],
                    rewritten,
                )
                    .into_response()
            }
            Err(error) => error_response(error),
        };
    }

    if is_key(&params.url) {
        return match fetch_bytes(&state, &params.url).await {
            Ok(bytes) => {
                ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
            }
            Err(error) => error_response(error),
        };
    }

    match fetch_bytes(&state, &params.url).await {
        Ok(bytes) => {
            let decoded = decode_segment(bytes, &state.info.xor_mask);
            ([(header::CONTENT_TYPE, "video/mp2t")], decoded).into_response()
        }
        Err(error) => error_response(error),
    }
}

fn error_response(error: Error) -> Response {
    let body = format!("ap0dl proxy: {error}");
    (
        StatusCode::BAD_GATEWAY,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

fn is_manifest(url: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    if path.ends_with(".m3u8") {
        return true;
    }
    // enc-dec's parse-flixcloud endpoint returns a rewritten playlist even
    // though its path has no .m3u8 suffix.
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.ends_with("enc-dec.app")))
        .unwrap_or(false)
        && path.contains("parse-flixcloud")
}

fn is_key(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.ends_with("key.bin") || path.ends_with(".key")
}

async fn fetch_text(state: &ProxyState, url: &str) -> Result<String> {
    let response = request(state, url).await?;
    response.text().await.map_err(|source| Error::Http {
        url: url.to_string(),
        source,
    })
}

async fn fetch_bytes(state: &ProxyState, url: &str) -> Result<bytes::Bytes> {
    let response = request(state, url).await?;
    response.bytes().await.map_err(|source| Error::Http {
        url: url.to_string(),
        source,
    })
}

async fn request(state: &ProxyState, url: &str) -> Result<reqwest::Response> {
    let mut request = state.client.get(url).header(header::USER_AGENT, USER_AGENT);
    let host = Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default();
    if host.ends_with("enc-dec.app") {
        request = request.header(header::REFERER, "https://enc-dec.app/");
    } else {
        request = request
            .header(header::REFERER, "https://flixcloud.cc/")
            .header(header::ORIGIN, "https://flixcloud.cc")
            .header(header::ACCEPT, "*/*");
    }
    let response = request.send().await.map_err(|source| Error::Http {
        url: url.to_string(),
        source,
    })?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        return Err(Error::HttpStatus {
            status,
            url: url.to_string(),
        });
    }
    Ok(response)
}

fn rewrite_manifest(state: &ProxyState, text: &str, base: &str, w_payload: &str) -> String {
    let mut out = String::with_capacity(text.len() + 512);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        if trimmed.starts_with("#EXT-X-KEY")
            || trimmed.starts_with("#EXT-X-MAP")
            || trimmed.starts_with("#EXT-X-MEDIA")
        {
            out.push_str(&rewrite_uri_attribute(state, line, base, w_payload));
            out.push('\n');
            continue;
        }

        if trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        let absolute = resolve_uri(base, trimmed);
        let next_base = decode_proxy_url(&absolute).unwrap_or_else(|| absolute.clone());
        out.push_str(&local_url(state, &absolute, w_payload, Some(&next_base)));
        out.push('\n');
    }
    out
}

fn rewrite_uri_attribute(state: &ProxyState, line: &str, base: &str, w_payload: &str) -> String {
    let Some(uri_start) = line.find("URI=") else {
        return line.to_string();
    };
    let after = &line[uri_start + 4..];
    let (quote, rest) = match after.chars().next() {
        Some(quote @ ('"' | '\'')) => (quote, &after[quote.len_utf8()..]),
        _ => return line.to_string(),
    };
    let Some(end) = rest.find(quote) else {
        return line.to_string();
    };
    let uri = &rest[..end];
    let mut absolute = resolve_uri(base, uri);
    // enc-dec sometimes absolutizes key URIs to its own /api/key.bin, which
    // does not exist. The real key lives next to the original manifest.
    if absolute.contains("enc-dec.app/api/key.bin") {
        absolute = resolve_uri(base, "key.bin");
    }
    let local = local_url(state, &absolute, w_payload, Some(base));
    format!(
        "{}URI={quote}{local}{quote}{}",
        &line[..uri_start],
        &rest[end + quote.len_utf8()..]
    )
}

fn local_url(state: &ProxyState, upstream: &str, w_payload: &str, base: Option<&str>) -> String {
    let mut url = format!(
        "http://{}/proxy?url={}&w_payload={}",
        state.addr,
        urlencoding::encode(upstream),
        urlencoding::encode(w_payload)
    );
    if let Some(base) = base {
        url.push_str("&base=");
        url.push_str(&urlencoding::encode(base));
    }
    url
}

fn resolve_uri(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    Url::parse(base)
        .and_then(|base| base.join(href))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| href.to_string())
}

fn decode_proxy_url(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    if !url.path().contains("parse-flixcloud") && !url.path().contains("dec-flixcloud") {
        return None;
    }
    url.query_pairs()
        .find(|(key, _)| key == "url")
        .map(|(_, value)| value.to_string())
}

fn decode_segment(data: bytes::Bytes, mask: &[u8]) -> bytes::Bytes {
    let (skip, should_xor) = detect_image_header(&data);
    if skip == 0 && !should_xor {
        return data;
    }

    let payload = &data[skip..];
    if !should_xor || mask.is_empty() {
        return bytes::Bytes::copy_from_slice(payload);
    }
    let mut decoded = payload.to_vec();
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte ^= mask[index % mask.len()];
    }
    bytes::Bytes::from(decoded)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_png_and_webp() {
        assert_eq!(detect_image_header(b"\x89PNG\r\n\x1a\nrest"), (8, true));
        assert_eq!(detect_image_header(b"RIFF0000WEBPrest"), (12, true));
        assert_eq!(detect_image_header(b"\x47\x40\x11\x10"), (0, false));
    }

    #[test]
    fn resolves_relative_uris() {
        assert_eq!(
            resolve_uri("https://fetch9.flixcloud.cc/a/video.m3u8", "key.bin"),
            "https://fetch9.flixcloud.cc/a/key.bin"
        );
    }

    #[test]
    fn recognizes_enc_dec_manifests() {
        assert!(is_manifest("https://enc-dec.app/api/parse-flixcloud?url=x"));
        assert!(is_manifest(
            "https://fetch9.flixcloud.cc/a/master.m3u8?token=x"
        ));
        assert!(!is_manifest("https://fetch9.flixcloud.cc/a/seg-1.png"));
    }

    #[test]
    fn decodes_proxy_url() {
        let encoded = urlencoding::encode("https://fetch9.flixcloud.cc/a/video.m3u8");
        let value = format!("https://enc-dec.app/api/parse-flixcloud?url={encoded}&w_payload=x");
        assert_eq!(
            decode_proxy_url(&value).as_deref(),
            Some("https://fetch9.flixcloud.cc/a/video.m3u8")
        );
    }

    #[test]
    fn recognizes_at_cloud_quota_pages() {
        let page = b"<title>Download Limit Reached | AnimeToki</title>\
            <p>The AT-Cloud Drive quota for this file has been exceeded due to high traffic.</p>";
        let reason = quota_reason(page, "https://cloud.animetoki.com/?a=download")
            .expect("quota page should be recognized");
        assert!(reason.contains("cloud.animetoki.com"));
        assert!(reason.contains("quota"));
        assert!(quota_reason(b"<title>Not Found</title>", "https://example.com").is_none());
    }

    #[test]
    fn parses_open_ended_byte_ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=90-", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=0-1048575", 500), Some((0, 499)));
        assert_eq!(parse_range("bytes=100-", 100), None);
    }
}

/// A local byte-range proxy for CDNs that reject media players' open-ended
/// range requests. AnimeToki is the current example: it serves 16 MiB bounded
/// ranges but returns 403/404 for `Range: bytes=0-` or huge ranges.
///
/// ExoPlayer and ffmpeg both naturally ask for `bytes=0-`; this proxy turns
/// that request into a sequence of bounded upstream reads while presenting a
/// normal 206 response to the player.
pub struct RangeProxy {
    addr: SocketAddr,
    handle: tokio::task::JoinHandle<()>,
}

const RANGE_CHUNK: u64 = 8 * 1024 * 1024;
const PROBE_BYTES: u64 = 1024 * 1024;

impl RangeProxy {
    pub async fn start(
        url: String,
        headers: std::collections::BTreeMap<String, String>,
    ) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(Error::Io)?;
        let addr = listener.local_addr().map_err(Error::Io)?;
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| Error::config(format!("range proxy client: {error}")))?;

        let mut request_headers = HeaderMap::new();
        for (name, value) in headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|error| Error::config(format!("range proxy header: {error}")))?;
            let value = HeaderValue::from_str(&value)
                .map_err(|error| Error::config(format!("range proxy header value: {error}")))?;
            request_headers.insert(name, value);
        }

        let probe = probe_upstream(&client, &url, &request_headers).await?;
        let state = RangeState {
            client,
            url,
            headers: request_headers,
            total: probe.total,
            content_type: probe.content_type,
            prefix: probe.prefix,
        };
        let app = Router::new()
            .route("/range", axum::routing::get(handle_range))
            .with_state(state);
        let handle = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                tracing::warn!(%error, "range proxy stopped");
            }
        });

        Ok(Self { addr, handle })
    }

    pub fn url(&self) -> String {
        format!("http://{}/range", self.addr)
    }
}

impl Drop for RangeProxy {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[derive(Clone)]
struct RangeState {
    client: Client,
    url: String,
    headers: HeaderMap,
    total: u64,
    content_type: String,
    /// The first PROBE_BYTES fetched during start-up. Serving these bytes from
    /// memory avoids asking the CDN for the same range twice.
    prefix: Bytes,
}

struct Probe {
    total: u64,
    content_type: String,
    prefix: Bytes,
}

async fn probe_upstream(client: &Client, url: &str, headers: &HeaderMap) -> Result<Probe> {
    let response = client
        .get(url)
        .headers(headers.clone())
        .header(
            header::RANGE,
            format!("bytes=0-{}", PROBE_BYTES.saturating_sub(1)),
        )
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|error| Error::provider("range-proxy", error))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    if status != StatusCode::PARTIAL_CONTENT {
        // Keep the error readable: AT-Cloud returns a 403/404 HTML page when a
        // file's download quota is empty, and a bare status code hides that.
        let preview = preview_body(response).await;
        if let Some(detail) = quota_reason(&preview, url) {
            return Err(Error::unavailable(detail));
        }
        return Err(Error::provider(
            "range-proxy",
            format!("upstream range probe returned {status}"),
        ));
    }
    let total = response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit_once('/'))
        .and_then(|(_, total)| total.trim().parse::<u64>().ok())
        .ok_or_else(|| {
            Error::provider(
                "range-proxy",
                format!("upstream range probe returned {status} without a total length"),
            )
        })?;
    let prefix = response
        .bytes()
        .await
        .map_err(|error| Error::provider("range-proxy", format!("reading probe body: {error}")))?;
    if prefix.is_empty() {
        return Err(Error::provider(
            "range-proxy",
            "upstream range probe returned an empty body",
        ));
    }
    Ok(Probe {
        total,
        content_type,
        prefix,
    })
}

async fn preview_body(mut response: reqwest::Response) -> Bytes {
    let mut preview = Vec::new();
    while preview.len() < 16 * 1024 {
        match response.chunk().await {
            Ok(Some(chunk)) => preview.extend_from_slice(&chunk),
            Ok(None) | Err(_) => break,
        }
    }
    Bytes::from(preview)
}

fn quota_reason(body: &[u8], url: &str) -> Option<String> {
    let text = String::from_utf8_lossy(body).to_ascii_lowercase();
    let quota = text.contains("download limit")
        || text.contains("quota")
        || text.contains("exceeded due to high traffic");
    if !quota {
        return None;
    }
    let host = Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "the source".to_string());
    Some(format!(
        "{host} has exhausted its download quota for this file; try another episode or source, or wait for the quota to reset"
    ))
}

async fn handle_range(State(state): State<RangeState>, headers: HeaderMap) -> Response {
    let (start, end, partial) = match headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => match parse_range(value, state.total) {
            Some(range) => (range.0, range.1, true),
            None => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{}", state.total))
                    .body(Body::empty())
                    .unwrap_or_else(|_| StatusCode::RANGE_NOT_SATISFIABLE.into_response());
            }
        },
        None => (0, state.total.saturating_sub(1), false),
    };
    if start > end || start >= state.total {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{}", state.total))
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::RANGE_NOT_SATISFIABLE.into_response());
    }

    let prefix_len = if start == 0 {
        (state.prefix.len() as u64).min(end - start + 1) as usize
    } else {
        0
    };
    let prefix = if prefix_len > 0 {
        state.prefix.slice(0..prefix_len)
    } else {
        Bytes::new()
    };
    let after_prefix = start.saturating_add(prefix_len as u64);
    let stream_state = state.clone();
    let stream = unfold((after_prefix, end, prefix), move |(next, end, pending)| {
        let state = stream_state.clone();
        async move {
            if !pending.is_empty() {
                return Some((Ok(pending), (next, end, Bytes::new())));
            }
            if next > end {
                return None;
            }
            let chunk_end = next.saturating_add(RANGE_CHUNK - 1).min(end);
            let result = fetch_chunk(&state, next, chunk_end).await;
            Some((result, (chunk_end.saturating_add(1), end, Bytes::new())))
        }
    });

    let length = end - start + 1;
    let mut builder = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, state.content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, length);
    if partial {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{}", state.total),
        );
    }
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn fetch_chunk(state: &RangeState, start: u64, end: u64) -> std::io::Result<Bytes> {
    let response = state
        .client
        .get(&state.url)
        .headers(state.headers.clone())
        .header(header::RANGE, format!("bytes={start}-{end}"))
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(std::io::Error::other)?;
    let status = response.status();
    if !status.is_success() {
        let preview = preview_body(response).await;
        let message = quota_reason(&preview, &state.url)
            .unwrap_or_else(|| format!("upstream range {start}-{end} returned {status}"));
        tracing::warn!(%message, "range proxy upstream failed");
        return Err(std::io::Error::other(message));
    }
    response.bytes().await.map_err(std::io::Error::other)
}

fn parse_range(value: &str, total: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 || total == 0 {
            return None;
        }
        let start = total.saturating_sub(suffix);
        return Some((start, total - 1));
    }
    let start = start.parse::<u64>().ok()?;
    let end = if end.is_empty() {
        total.saturating_sub(1)
    } else {
        end.parse::<u64>().ok()?.min(total.saturating_sub(1))
    };
    if start > end || start >= total {
        None
    } else {
        Some((start, end))
    }
}
