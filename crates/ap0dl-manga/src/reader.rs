use std::collections::BTreeMap;
use std::path::Path;

use ap0dl_core::error::{Error, Result};
use ap0dl_core::player::{find_player, is_mpv_like, is_windows_player, is_wsl};

use crate::models::Page;

/// Picks the reader binary. The configured default `mpv` follows the anime
/// player rule: on WSL it becomes Windows `mpv.exe` when that exists.
pub fn resolve_reader(configured: &str) -> String {
    let configured = configured.trim();
    if configured.is_empty() || configured == "mpv" {
        if let Some(found) = find_player().filter(|player| is_mpv_like(player)) {
            return found;
        }
    }
    if configured.is_empty() {
        "mpv".to_string()
    } else {
        configured.to_string()
    }
}

/// Whether the reader can open remote image URLs itself (mpv family).
/// Other viewers are given a local folder of downloaded pages.
pub fn reads_urls(reader: &str) -> bool {
    is_mpv_like(&reader.to_ascii_lowercase())
}

/// Opens the chapter's image URLs as an mpv playlist: each page stays on
/// screen until the user presses `>`/`<` (or Enter/PgDn), and the CDN
/// headers ride along so Referer-protected images load.
pub fn read_urls(reader: &str, title: &str, pages: &[Page], extra_args: &[String]) -> Result<()> {
    let headers: BTreeMap<String, String> = pages
        .iter()
        .find(|page| !page.headers.is_empty())
        .map(|page| page.headers.clone())
        .unwrap_or_default();
    let mut args = mpv_args(reader, title, &headers, extra_args);
    args.extend(pages.iter().map(|page| page.image_url.clone()));
    run(reader, &args)
}

/// Opens a local chapter (folder or CBZ) with any viewer. mpv-like readers
/// get the same image-viewer options as [`read_urls`].
pub fn read_path(reader: &str, title: &str, path: &Path, extra_args: &[String]) -> Result<()> {
    let mut args = if reads_urls(reader) {
        mpv_args(reader, title, &BTreeMap::new(), extra_args)
    } else {
        extra_args.to_vec()
    };
    args.push(path.display().to_string());
    run(reader, &args)
}

pub(crate) fn mpv_args(
    reader: &str,
    title: &str,
    headers: &BTreeMap<String, String>,
    extra_args: &[String],
) -> Vec<String> {
    let mut args = vec![
        format!("--force-media-title={title}"),
        "--image-display-duration=inf".to_string(),
        "--force-window=yes".to_string(),
        "--keep-open=yes".to_string(),
        "--loop-playlist=no".to_string(),
    ];
    if is_windows_player(reader) {
        args.push("--terminal=no".to_string());
    } else if is_wsl() && !extra_args.iter().any(|arg| arg.starts_with("--vo")) {
        args.push("--vo=x11".to_string());
    }
    if let Some(referer) = headers.get("Referer") {
        args.push(format!("--referrer={referer}"));
    }
    let fields: Vec<String> = headers
        .iter()
        .filter(|(name, _)| name.as_str() != "Referer")
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    if !fields.is_empty() {
        args.push(format!("--http-header-fields={}", fields.join(",")));
    }
    args.extend(extra_args.iter().cloned());
    args
}

fn run(reader: &str, args: &[String]) -> Result<()> {
    if which::which(reader).is_err() {
        return Err(Error::Player(format!(
            "{reader:?} was not found in PATH; install it or choose another --reader"
        )));
    }
    let status = std::process::Command::new(reader)
        .args(args)
        .status()
        .map_err(|error| Error::Player(format!("could not launch {reader}: {error}")))?;
    if !status.success() {
        return Err(Error::Player(format!(
            "{reader} exited with {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpv_gets_image_viewer_flags_and_referer() {
        let headers =
            BTreeMap::from([("Referer".to_string(), "https://mangapill.com/".to_string())]);
        let args = mpv_args("mpv", "Bocchi - Chapter 1", &headers, &[]);
        assert!(args.contains(&"--image-display-duration=inf".to_string()));
        assert!(args.contains(&"--referrer=https://mangapill.com/".to_string()));
        assert!(
            args.iter()
                .all(|arg| !arg.starts_with("--http-header-fields"))
        );
        let args = mpv_args("mpv.exe", "x", &BTreeMap::new(), &[]);
        assert!(args.contains(&"--terminal=no".to_string()));
    }

    #[test]
    fn only_mpv_family_reads_urls() {
        assert!(reads_urls("mpv"));
        assert!(reads_urls("mpv.exe"));
        assert!(!reads_urls("feh"));
        assert!(!reads_urls("zathura"));
    }
}
