use std::path::Path;

/// Makes a title safe to use as a file or directory name on every platform.
pub fn sanitize_filename(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => out.push_str(" - "),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let out = out.trim_matches([' ', '.']).to_string();
    let out = out
        .trim_end_matches(" - ")
        .trim_end_matches('-')
        .trim()
        .to_string();
    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
}

/// Removes HTML tags and decodes the handful of entities that appear in
/// anime descriptions.
pub fn strip_html(value: &str) -> String {
    let value = value
        .replace("<br>", " ")
        .replace("<br/>", " ")
        .replace("<br />", " ")
        .replace("</p>", " ");
    let value = value.as_str();
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

pub fn parse_u32(value: &str) -> Option<u32> {
    value
        .trim()
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .ok()
}

pub fn parse_f32(value: &str) -> Option<f32> {
    let filtered: String = value
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    filtered.parse().ok()
}

/// Extracts the last numeric run from a title such as `One Piece 1080`.
pub fn last_number(value: &str) -> Option<f32> {
    let mut found = None;
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            found = current.parse::<f32>().ok().or(found);
            current.clear();
        }
    }
    if !current.is_empty() {
        found = current.parse::<f32>().ok().or(found);
    }
    found
}

pub fn ensure_trailing_slash(url: &str) -> String {
    if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    }
}

pub fn absolutize(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    if let Ok(base) = url::Url::parse(base) {
        if let Ok(url) = base.join(href) {
            return url.to_string();
        }
    }
    let base = ensure_trailing_slash(base);
    format!("{}{}", base, href.trim_start_matches('/'))
}

pub fn file_stem_for(directory: &Path, title: &str, episode: f32) -> std::path::PathBuf {
    let title = sanitize_filename(title);
    let number = if episode.fract() == 0.0 {
        format!("{:02}", episode as u32)
    } else {
        format!("{episode:02}")
    };
    directory.join(title).join(format!("E{number}"))
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(
            sanitize_filename("One Piece: Wano/Arc?"),
            "One Piece - Wano - Arc"
        );
        assert_eq!(sanitize_filename("  ...  "), "untitled");
    }

    #[test]
    fn parses_last_number() {
        assert_eq!(last_number("One Piece 1080"), Some(1080.0));
        assert_eq!(last_number("Episode 12.5 - recap"), Some(12.5));
        assert_eq!(last_number("no number"), None);
    }

    #[test]
    fn strips_html() {
        assert_eq!(strip_html("<p>A &amp; B<br/>C</p>"), "A & B C");
    }
}
