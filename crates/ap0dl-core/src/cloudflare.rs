use std::sync::OnceLock;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::HeaderMap;
use wreq_util::Emulation;

use crate::error::{Error, Result};

/// A native TLS/HTTP2 impersonation client used only when the normal reqwest
/// client receives a Cloudflare challenge response. This is deliberately not
/// a browser: it does not execute JavaScript or solve CAPTCHAs. It is enough
/// for sites that gate on JA3/JA4 or HTTP/2 fingerprint rather than Turnstile.
static CLIENT: OnceLock<Option<wreq::Client>> = OnceLock::new();

fn client() -> Result<&'static wreq::Client> {
    CLIENT
        .get_or_init(|| {
            wreq::Client::builder()
                .emulation(Emulation::Chrome137)
                .timeout(Duration::from_secs(30))
                .redirect(wreq::redirect::Policy::limited(5))
                .build()
                .ok()
        })
        .as_ref()
        .ok_or_else(|| Error::provider("cloudflare", "could not build TLS-impersonation client"))
}

/// True for the common Cloudflare blocked/challenge statuses. A 403 from a
/// normal origin (authentication, geo-block) is intentionally not retried.
pub fn is_challenge_status(status: u16) -> bool {
    matches!(status, 403 | 503)
}

/// Detects the challenge shell returned by Cloudflare or a generic WAF. A
/// successful HTTP status can still carry this body, so callers must check it.
pub fn is_challenge_body(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    // `challenge-platform` is injected into some ordinary Cloudflare-
    // protected pages too, so it is not a reliable challenge signal on its
    // own. The markers below are specific to an actual challenge shell.
    lower.contains("just a moment...")
        || lower.contains("cf-mitigated")
        || lower.contains("cf-chl-")
        || (lower.contains("<title>attention required!") && lower.contains("cloudflare"))
}

pub async fn get_text(url: &str, headers: &HeaderMap) -> Result<String> {
    let mut request = client()?.get(url);
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            request = request.header(name.as_str(), value);
        }
    }
    let response = request.send().await.map_err(|error| {
        Error::provider(
            "cloudflare",
            format!("TLS fallback request failed: {error}"),
        )
    })?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|error| {
        Error::provider("cloudflare", format!("TLS fallback body failed: {error}"))
    })?;
    if is_challenge_body(&body) {
        return Err(Error::provider(
            "cloudflare",
            "TLS impersonation reached a JavaScript/Turnstile challenge (not bypassable without a browser)",
        ));
    }
    if !(200..300).contains(&status) {
        return Err(Error::HttpStatus {
            status,
            url: format!("{url} (TLS impersonation)"),
        });
    }
    Ok(body)
}

pub async fn get_bytes(url: &str, headers: &HeaderMap) -> Result<Bytes> {
    let mut request = client()?.get(url);
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            request = request.header(name.as_str(), value);
        }
    }
    let response = request.send().await.map_err(|error| {
        Error::provider(
            "cloudflare",
            format!("TLS fallback request failed: {error}"),
        )
    })?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(Error::HttpStatus {
            status,
            url: format!("{url} (TLS impersonation)"),
        });
    }
    response.bytes().await.map_err(|error| {
        Error::provider("cloudflare", format!("TLS fallback body failed: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_challenge_shells() {
        assert!(is_challenge_body("<title>Just a moment...</title>"));
        assert!(is_challenge_body("cf-mitigated: challenge"));
        assert!(!is_challenge_body(
            "<script src=\"/cdn-cgi/challenge-platform/h/b/scripts/jsd/main.js\"></script>"
        ));
        assert!(!is_challenge_body("<html><body>normal page</body></html>"));
        assert!(is_challenge_status(403));
        assert!(!is_challenge_status(401));
    }
}
