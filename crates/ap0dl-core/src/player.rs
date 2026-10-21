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
    if lower.contains("mpv") || lower.contains("celluloid") || lower.contains("iina") {
        command
            .arg("--force-window=yes")
            .arg(format!("--title={title}"));
        // WSL often exposes a software/indirect X11 display without a usable
        // Vulkan device. mpv's default gpu-next/Vulkan path then falls back to
        // X11 and can abort in vo_x11_init. Use the plain X11 output for WSL
        // unless the caller explicitly chose a video output.
        if is_wsl() && !extra_args.iter().any(|arg| arg.starts_with("--vo")) {
            command.arg("--vo=x11").arg("--hwdec=no");
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

fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
        || std::fs::read_to_string("/proc/version")
            .map(|version| version.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

pub fn find_player() -> Option<String> {
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
