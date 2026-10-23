//! Two-page spreads: reads image sizes from file headers, pairs pages the
//! way a printed volume is laid out, and joins each pair into one image
//! with `ffmpeg` so any image viewer can show it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use ap0dl_core::error::{Error, Result};

/// One page per frame, or two portrait pages side by side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageLayout {
    Single,
    #[default]
    Double,
}

impl PageLayout {
    pub fn toggled(self) -> Self {
        match self {
            Self::Single => Self::Double,
            Self::Double => Self::Single,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "single page",
            Self::Double => "double page",
        }
    }
}

impl std::str::FromStr for PageLayout {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "single" | "1" | "one" => Ok(Self::Single),
            "double" | "2" | "two" | "spread" => Ok(Self::Double),
            other => Err(format!(
                "unknown page layout {other:?}; use single or double"
            )),
        }
    }
}

/// Page order inside a spread and which arrow key turns forward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadingDirection {
    /// Right to left (Japanese manga): the first page sits on the right.
    #[default]
    Rtl,
    /// Left to right (comics, most manhua).
    Ltr,
}

impl std::str::FromStr for ReadingDirection {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rtl" | "right-to-left" | "manga" => Ok(Self::Rtl),
            "ltr" | "left-to-right" | "comic" => Ok(Self::Ltr),
            other => Err(format!(
                "unknown reading direction {other:?}; use rtl or ltr"
            )),
        }
    }
}

/// Width and height from an image's header (PNG, JPEG, WebP or GIF).
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        return Some((be32(&bytes[16..20]), be32(&bytes[20..24])));
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        return Some((le16(&bytes[6..8]), le16(&bytes[8..10])));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return webp_size(bytes);
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return jpeg_size(bytes);
    }
    None
}

/// [`image_size`] of a file on disk.
pub fn file_size(path: &Path) -> Option<(u32, u32)> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| image_size(&bytes))
}

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn le16(bytes: &[u8]) -> u32 {
    u32::from(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let chunk = bytes.get(12..16)?;
    match chunk {
        b"VP8 " => {
            // Frame tag (3 bytes) and start code (3 bytes), then 14-bit sizes.
            let data = bytes.get(26..30)?;
            Some((le16(&data[0..2]) & 0x3FFF, le16(&data[2..4]) & 0x3FFF))
        }
        b"VP8L" => {
            let data = bytes.get(21..25)?;
            let bits = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" => {
            let data = bytes.get(24..30)?;
            let width = u32::from_le_bytes([data[0], data[1], data[2], 0]) + 1;
            let height = u32::from_le_bytes([data[3], data[4], data[5], 0]) + 1;
            Some((width, height))
        }
        _ => None,
    }
}

fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut offset = 2;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xFF {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        if marker == 0xFF {
            offset += 1;
            continue;
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            offset += 2;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]));
        let is_frame = (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            let data = bytes.get(offset + 5..offset + 9)?;
            let height = u32::from(u16::from_be_bytes([data[0], data[1]]));
            let width = u32::from(u16::from_be_bytes([data[2], data[3]]));
            return Some((width, height));
        }
        offset += 2 + length;
    }
    None
}

/// Pages that are never paired: unknown sizes, pages already drawn as a
/// spread (landscape) and webtoon strips.
fn stands_alone(size: Option<(u32, u32)>) -> bool {
    match size {
        Some((width, height)) => width >= height || height > width.saturating_mul(5) / 2,
        None => true,
    }
}

/// Groups page indexes into frames. In double layout portrait pages are
/// paired in order; `offset` shows the first page alone (a cover) so the
/// following pairs shift by one.
pub fn frames(sizes: &[Option<(u32, u32)>], layout: PageLayout, offset: bool) -> Vec<Vec<usize>> {
    if layout == PageLayout::Single {
        return (0..sizes.len()).map(|index| vec![index]).collect();
    }
    let mut frames = Vec::new();
    let mut index = 0;
    while index < sizes.len() {
        let alone = (offset && index == 0)
            || stands_alone(sizes[index])
            || index + 1 >= sizes.len()
            || stands_alone(sizes[index + 1]);
        if alone {
            frames.push(vec![index]);
            index += 1;
        } else {
            frames.push(vec![index, index + 1]);
            index += 2;
        }
    }
    frames
}

/// Joins two pages into one image at the taller page's height. In RTL
/// order `first` is placed on the right.
pub async fn compose(
    ffmpeg: &Path,
    first: &Path,
    second: &Path,
    height: u32,
    direction: ReadingDirection,
    output: &Path,
) -> Result<()> {
    let height = height.max(2) & !1;
    let order = match direction {
        ReadingDirection::Rtl => "[b][a]",
        ReadingDirection::Ltr => "[a][b]",
    };
    let filter = format!(
        "[0:v]scale=-2:{height},format=rgb24[a];[1:v]scale=-2:{height},format=rgb24[b];{order}hstack=inputs=2"
    );
    let result = tokio::process::Command::new(ffmpeg)
        .args(["-nostdin", "-y", "-loglevel", "error", "-i"])
        .arg(first)
        .arg("-i")
        .arg(second)
        .args(["-filter_complex", &filter, "-frames:v", "1", "-q:v", "2"])
        .arg(output)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| Error::Player(format!("could not run ffmpeg: {error}")))?;
    if !result.status.success() || !output.exists() {
        return Err(Error::Player(format!(
            "ffmpeg could not join {} and {}: {}",
            first.display(),
            second.display(),
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(())
}

/// Where the joined image for pages `first` and `second` is cached.
pub fn spread_path(
    directory: &Path,
    first: usize,
    second: usize,
    direction: ReadingDirection,
) -> PathBuf {
    let order = match direction {
        ReadingDirection::Rtl => "rtl",
        ReadingDirection::Ltr => "ltr",
    };
    directory.join(format!("spread-{first:03}-{second:03}-{order}.jpg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sizes_from_headers() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(800u32.to_be_bytes());
        png.extend(1200u32.to_be_bytes());
        assert_eq!(image_size(&png), Some((800, 1200)));

        let gif = b"GIF89a\x20\x03\xb0\x04";
        assert_eq!(image_size(gif), Some((800, 1200)));

        // SOI, an APP0 segment, then SOF0 with height 1200 and width 800.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x04,
            0xB0, 0x03, 0x20, 0x03,
        ];
        assert_eq!(image_size(&jpeg), Some((800, 1200)));

        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        webp.extend([0x1F, 0x03, 0x00, 0xAF, 0x04, 0x00]);
        assert_eq!(image_size(&webp), Some((800, 1200)));

        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        let bits: u32 = 799 | (1199 << 14);
        lossless.extend(bits.to_le_bytes());
        assert_eq!(image_size(&lossless), Some((800, 1200)));

        assert_eq!(image_size(b"not an image"), None);
    }

    #[test]
    fn pairs_portrait_pages_and_keeps_wide_pages_alone() {
        let portrait = Some((800, 1200));
        let wide = Some((1600, 1200));
        let sizes = [portrait, portrait, wide, portrait, portrait, portrait];
        assert_eq!(
            frames(&sizes, PageLayout::Double, false),
            vec![vec![0, 1], vec![2], vec![3, 4], vec![5]]
        );
        assert_eq!(
            frames(&sizes, PageLayout::Double, true),
            vec![vec![0], vec![1], vec![2], vec![3, 4], vec![5]]
        );
        assert_eq!(frames(&sizes, PageLayout::Single, false).len(), 6);
    }

    #[test]
    fn two_page_chapter_is_one_spread_and_strips_stay_single() {
        let portrait = Some((800, 1200));
        assert_eq!(
            frames(&[portrait, portrait], PageLayout::Double, false),
            vec![vec![0, 1]]
        );
        let strip = Some((800, 4000));
        assert_eq!(
            frames(&[strip, strip, None], PageLayout::Double, false),
            vec![vec![0], vec![1], vec![2]]
        );
    }

    #[test]
    fn layout_and_direction_parse() {
        assert_eq!("double".parse::<PageLayout>(), Ok(PageLayout::Double));
        assert_eq!("1".parse::<PageLayout>(), Ok(PageLayout::Single));
        assert_eq!("LTR".parse::<ReadingDirection>(), Ok(ReadingDirection::Ltr));
        assert!("sideways".parse::<ReadingDirection>().is_err());
        assert_eq!(PageLayout::Single.toggled(), PageLayout::Double);
    }
}
