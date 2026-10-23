//! Minimal CBZ (ZIP) writer and `ComicInfo.xml` builder.
//!
//! Page images are already compressed, so entries are written with the ZIP
//! "stored" method. That needs only a CRC-32 and the three ZIP record types,
//! which keeps the plugin on the workspace's existing dependency set.

use std::io::{self, Write};

use chrono::{Datelike, Local, Timelike};

use crate::models::{Chapter, Manga};

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const VERSION: u16 = 20;
/// General-purpose flag bit 11: file names are UTF-8.
const UTF8_NAMES: u16 = 0x0800;

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// Streams stored entries into `W` and writes the central directory on
/// [`CbzWriter::finish`].
pub struct CbzWriter<W: Write> {
    out: W,
    offset: u64,
    entries: Vec<Entry>,
    time: u16,
    date: u16,
}

impl<W: Write> CbzWriter<W> {
    pub fn new(out: W) -> Self {
        let now = Local::now();
        let time =
            ((now.hour() as u16) << 11) | ((now.minute() as u16) << 5) | (now.second() as u16 / 2);
        let year = now.year().clamp(1980, 2107) as u16 - 1980;
        let date = (year << 9) | ((now.month() as u16) << 5) | now.day() as u16;
        Self {
            out,
            offset: 0,
            entries: Vec::new(),
            time,
            date,
        }
    }

    pub fn add(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let size = u32::try_from(data.len()).map_err(|_| too_large())?;
        let offset = u32::try_from(self.offset).map_err(|_| too_large())?;
        let crc = crc32(data);
        let name_bytes = name.as_bytes();

        let mut header = Vec::with_capacity(30 + name_bytes.len());
        header.extend_from_slice(&LOCAL_HEADER.to_le_bytes());
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.extend_from_slice(&UTF8_NAMES.to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes()); // stored
        header.extend_from_slice(&self.time.to_le_bytes());
        header.extend_from_slice(&self.date.to_le_bytes());
        header.extend_from_slice(&crc.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes());
        header.extend_from_slice(name_bytes);
        self.out.write_all(&header)?;
        self.out.write_all(data)?;
        self.offset += header.len() as u64 + data.len() as u64;
        self.entries.push(Entry {
            name: name.to_string(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<W> {
        let start = u32::try_from(self.offset).map_err(|_| too_large())?;
        let mut directory = Vec::new();
        for entry in &self.entries {
            let name = entry.name.as_bytes();
            directory.extend_from_slice(&CENTRAL_HEADER.to_le_bytes());
            directory.extend_from_slice(&VERSION.to_le_bytes()); // made by
            directory.extend_from_slice(&VERSION.to_le_bytes()); // needed
            directory.extend_from_slice(&UTF8_NAMES.to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes());
            directory.extend_from_slice(&self.time.to_le_bytes());
            directory.extend_from_slice(&self.date.to_le_bytes());
            directory.extend_from_slice(&entry.crc.to_le_bytes());
            directory.extend_from_slice(&entry.size.to_le_bytes());
            directory.extend_from_slice(&entry.size.to_le_bytes());
            directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes()); // extra
            directory.extend_from_slice(&0u16.to_le_bytes()); // comment
            directory.extend_from_slice(&0u16.to_le_bytes()); // disk
            directory.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
            directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            directory.extend_from_slice(&entry.offset.to_le_bytes());
            directory.extend_from_slice(name);
        }
        let count = u16::try_from(self.entries.len()).map_err(|_| too_large())?;
        let size = u32::try_from(directory.len()).map_err(|_| too_large())?;
        directory.extend_from_slice(&END_OF_CENTRAL_DIRECTORY.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&count.to_le_bytes());
        directory.extend_from_slice(&count.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&start.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        self.out.write_all(&directory)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

fn too_large() -> io::Error {
    io::Error::other("chapter archive exceeds the 4 GiB / 65535-entry ZIP limits")
}

/// CRC-32 (IEEE 802.3, reflected, polynomial 0xEDB88320).
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, slot) in table.iter_mut().enumerate() {
            let mut value = index as u32;
            for _ in 0..8 {
                value = if value & 1 == 1 {
                    0xEDB8_8320 ^ (value >> 1)
                } else {
                    value >> 1
                };
            }
            *slot = value;
        }
        table
    });
    !data.iter().fold(!0u32, |crc, byte| {
        table[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8)
    })
}

/// `ComicInfo.xml` in the Anansi schema that Mihon/Aniyomi, Komga and
/// Kavita read from CBZ files.
pub fn comic_info(manga: &Manga, chapter: &Chapter, page_count: usize) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ComicInfo xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n",
    );
    let mut field = |name: &str, value: Option<&str>| {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            xml.push_str(&format!("  <{name}>{}</{name}>\n", escape(value)));
        }
    };
    let number = (chapter.number >= 0.0).then(|| {
        if chapter.number.fract() == 0.0 {
            format!("{}", chapter.number as i64)
        } else {
            chapter.number.to_string()
        }
    });
    let genres = manga.genres.join(", ");
    let pages = page_count.to_string();
    field("Title", Some(&chapter.name));
    field("Series", Some(&manga.title));
    field("Number", number.as_deref());
    field("Volume", chapter.volume.as_deref());
    field("Summary", manga.description.as_deref());
    field("Writer", manga.author.as_deref());
    field("Penciller", manga.artist.as_deref());
    field("Translator", chapter.scanlator.as_deref());
    field("Genre", Some(&genres));
    field("Web", Some(&chapter.url));
    field("PageCount", Some(&pages));
    field("LanguageISO", chapter.language.as_deref());
    field("Manga", Some("Yes"));
    xml.push_str("</ComicInfo>\n");
    xml
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn crc32_matches_known_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"hello"), 0x3610_a686);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414f_a339
        );
    }

    #[test]
    fn writes_a_readable_stored_zip() {
        let mut writer = CbzWriter::new(Vec::new());
        writer.add("001.jpg", b"hello").unwrap();
        writer.add("ComicInfo.xml", b"<ComicInfo/>").unwrap();
        let bytes = writer.finish().unwrap();

        assert_eq!(u32_at(&bytes, 0), LOCAL_HEADER);
        assert_eq!(u32_at(&bytes, 14), 0x3610_a686);
        assert_eq!(&bytes[30..37], b"001.jpg");
        assert_eq!(&bytes[37..42], b"hello");

        let end = bytes.len() - 22;
        assert_eq!(u32_at(&bytes, end), END_OF_CENTRAL_DIRECTORY);
        assert_eq!(u16_at(&bytes, end + 10), 2);
        let directory = u32_at(&bytes, end + 16) as usize;
        assert_eq!(u32_at(&bytes, directory), CENTRAL_HEADER);
        assert_eq!(u32_at(&bytes, directory + 42), 0);
        let second_offset = u32_at(&bytes, directory + 46 + 7 + 42) as usize;
        assert_eq!(u32_at(&bytes, second_offset), LOCAL_HEADER);
        assert_eq!(
            &bytes[second_offset + 30..second_offset + 43],
            b"ComicInfo.xml"
        );
    }

    #[test]
    fn comic_info_escapes_and_skips_empty_fields() {
        let mut manga = Manga::new("test", "1", "Bocchi & Friends", "https://example.test/1");
        manga.genres = vec!["Comedy".to_string(), "Music".to_string()];
        let chapter = Chapter {
            id: "c".to_string(),
            url: "https://example.test/c?a=1&b=2".to_string(),
            name: "Ch.2 - <Live>".to_string(),
            number: 2.0,
            volume: None,
            scanlator: None,
            date_upload: None,
            language: Some("en".to_string()),
            pages: None,
        };
        let xml = comic_info(&manga, &chapter, 11);
        assert!(xml.contains("<Series>Bocchi &amp; Friends</Series>"));
        assert!(xml.contains("<Title>Ch.2 - &lt;Live&gt;</Title>"));
        assert!(xml.contains("<Number>2</Number>"));
        assert!(xml.contains("<PageCount>11</PageCount>"));
        assert!(xml.contains("<Genre>Comedy, Music</Genre>"));
        assert!(!xml.contains("<Volume>"));
        assert!(xml.contains("?a=1&amp;b=2"));
    }
}
