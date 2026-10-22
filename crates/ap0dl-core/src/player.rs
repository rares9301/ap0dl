use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{Error, Result};

/// Supported media players. Unknown names are passed through as executables,
/// which keeps custom setups working.
pub fn launch(
    player: &str,
    url: &str,
    title: &str,
    headers: &BTreeMap<String, String>,
    subtitles: &[std::path::PathBuf],
    extra_args: &[String],
) -> Result<()> {
    let executable = player.trim();
    if executable.is_empty() {
        return Err(Error::Player("player name is empty".to_string()));
    }
    if which::which(executable).is_err() {
        return Err(Error::Player(format!(
            "{executable:?} was not found in PATH; install it or choose another --player"
        )));
    }

    let mut command = std::process::Command::new(executable);
    let lower = executable.to_ascii_lowercase();
    if is_mpv_like(&lower) {
        command.arg(format!("--force-media-title={title}"));
        let windows = is_windows_player(executable);
        if windows {
            // A Windows mpv gives WSL users the native, hardware-accelerated
            // window instead of the slow/indirect X11 path.
            command.arg("--terminal=no");
        } else if is_wsl() && !extra_args.iter().any(|arg| arg.starts_with("--vo")) {
            // Fallback for systems without mpv.exe: plain X11 is more reliable
            // than mpv's Vulkan -> vo_x11 fallback under WSL.
            command.arg("--vo=x11").arg("--hwdec=no");
        }
        if let Some(referrer) = headers.get("Referer") {
            command.arg(format!("--referrer={referrer}"));
        }
        if !headers.is_empty() {
            command.arg(format!("--http-header-fields={}", header_fields(headers)));
        }
        for subtitle in subtitles {
            command.arg(format!("--sub-file={}", subtitle.display()));
        }
    } else if lower.contains("vlc") {
        command.arg("--play-and-exit");
        if let Some(referer) = headers.get("Referer") {
            command.arg(format!("--http-referrer={referer}"));
        }
        if let Some(user_agent) = headers.get("User-Agent") {
            command.arg(format!("--http-user-agent={user_agent}"));
        }
        for subtitle in subtitles {
            command.arg(format!("--sub-file={}", subtitle.display()));
        }
    } else if lower.contains("ffplay") {
        if !headers.is_empty() {
            command.arg("-headers").arg(headers_for_ffmpeg(headers));
        }
        if let Some(subtitle) = subtitles.first() {
            command
                .arg("-vf")
                .arg(format!("subtitles={}", subtitle.display()));
        }
    } else {
        for subtitle in subtitles {
            command.arg(format!("--sub-file={}", subtitle.display()));
        }
    }

    command.args(extra_args);
    command.arg(url);

    let status = command
        .status()
        .map_err(|error| Error::Player(format!("could not launch {executable}: {error}")))?;
    if !status.success() {
        return Err(Error::Player(format!(
            "{executable} exited with {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

fn header_fields(headers: &BTreeMap<String, String>) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn headers_for_ffmpeg(headers: &BTreeMap<String, String>) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>()
}

pub fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || std::fs::read_to_string("/proc/version")
            .map(|version| version.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

pub fn is_windows_player(name: &str) -> bool {
    name.trim().to_ascii_lowercase().ends_with(".exe")
}

pub fn is_mpv_like(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("mpv") || name.contains("celluloid") || name.contains("iina")
}

pub fn find_player() -> Option<String> {
    // On WSL, prefer the Windows mpv. It uses the native Windows GPU stack and
    // avoids the laggy/odd X11 window users get from Linux mpv under WSLg.
    if is_wsl() && which::which("mpv.exe").is_ok() {
        return Some("mpv.exe".to_string());
    }
    ["mpv", "vlc", "celluloid", "iina", "ffplay"]
        .into_iter()
        .find(|name| which::which(name).is_ok())
        .map(str::to_string)
}

pub fn subtitle_paths(paths: &[String]) -> Vec<std::path::PathBuf> {
    paths.iter().map(std::path::PathBuf::from).collect()
}

pub fn prepare_subtitle_paths(paths: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    paths.iter().filter(|path| path.exists()).cloned().collect()
}

pub fn _unused(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_windows_mpv() {
        assert!(is_windows_player("mpv.exe"));
        assert!(is_windows_player(r"C:\Program Files\mpv\mpv.exe"));
        assert!(!is_windows_player("mpv"));
        assert!(is_mpv_like("mpv.exe"));
        assert!(is_mpv_like("celluloid"));
        assert!(!is_mpv_like("vlc"));
    }
}
