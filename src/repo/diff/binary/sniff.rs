//! What a binary file is, from its first bytes, and what can be told about it: a PDF's version
//! and pages, an archive's members, an image's size, an executable's kind.

use super::zip;

/// One fact about a file: (what, value).
pub type Fact = (String, String);

/// What kind of file `bytes` are, in words ("PDF document", "ZIP archive"…), and what is known
/// about them.
pub fn describe(path: &str, bytes: &[u8]) -> (String, Vec<Fact>) {
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if bytes.starts_with(b"%PDF-") {
        return ("PDF document".to_owned(), pdf_facts(bytes));
    }
    if zip::is_zip(bytes) {
        let kind = match ext.as_str() {
            "docx" | "docm" => "Word document",
            "xlsx" | "xlsm" => "Excel workbook",
            "pptx" | "ppsx" => "PowerPoint presentation",
            "odt" => "OpenDocument text",
            "ods" => "OpenDocument spreadsheet",
            "odp" => "OpenDocument presentation",
            "jar" => "Java archive",
            "apk" => "Android package",
            "epub" => "EPUB book",
            "unitypackage" => "Unity package",
            _ => "ZIP archive",
        };
        let members = zip::members(bytes).unwrap_or_default();
        let unpacked: u64 = members.iter().map(|m| m.size).sum();
        let facts = vec![
            ("Members".to_owned(), members.len().to_string()),
            ("Unpacked".to_owned(), crate::git::human_size(unpacked)),
        ];
        return (kind.to_owned(), facts);
    }
    if let Some((kind, facts)) = image_facts(bytes) {
        return (kind, facts);
    }
    let kind = MAGIC
        .iter()
        .find(|(magic, _)| bytes.starts_with(magic))
        .map(|(_, kind)| *kind)
        .unwrap_or("Binary file");
    (kind.to_owned(), Vec::new())
}

const MAGIC: &[(&[u8], &str)] = &[
    (b"\x7fELF", "ELF executable or library"),
    (b"MZ", "Windows executable or library"),
    (b"\xcf\xfa\xed\xfe", "Mach-O executable"),
    (b"\xca\xfe\xba\xbe", "Mach-O universal binary or Java class"),
    (b"SQLite format 3\0", "SQLite database"),
    (b"\x1f\x8b", "gzip archive"),
    (b"BZh", "bzip2 archive"),
    (b"\xfd7zXZ\0", "xz archive"),
    (b"7z\xbc\xaf\x27\x1c", "7-Zip archive"),
    (b"Rar!\x1a\x07", "RAR archive"),
    (b"\x28\xb5\x2f\xfd", "Zstandard archive"),
    (b"OggS", "Ogg audio or video"),
    (b"fLaC", "FLAC audio"),
    (b"ID3", "MP3 audio"),
    (b"\0asm", "WebAssembly module"),
    (
        b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1",
        "Office 97–2003 document",
    ),
    (b"wOFF", "WOFF font"),
    (b"wOF2", "WOFF2 font"),
    (b"\0\x01\0\0", "TrueType font"),
    (b"OTTO", "OpenType font"),
    (b"glTF", "glTF binary model"),
    (b"Kaydara FBX Binary", "FBX model"),
    (b"8BPS", "Photoshop image"),
    (b"RIFF", "RIFF media (WAV, AVI…)"),
];

/// The size of a PNG, GIF, BMP or JPEG, and its colour type where it is in the header.
fn image_facts(b: &[u8]) -> Option<(String, Vec<Fact>)> {
    let dims = |w: u32, h: u32| ("Dimensions".to_owned(), format!("{w} × {h}"));
    if b.starts_with(b"\x89PNG\r\n\x1a\n") && b.len() >= 26 {
        let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
        let color = match b[25] {
            0 => "grey",
            2 => "RGB",
            3 => "indexed",
            4 => "grey with alpha",
            6 => "RGB with alpha",
            _ => "unknown",
        };
        let depth = format!("{} bits, {color}", b[24]);
        return Some((
            "PNG image".to_owned(),
            vec![dims(w, h), ("Colour".to_owned(), depth)],
        ));
    }
    if (b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) && b.len() >= 10 {
        let w = u16::from_le_bytes([b[6], b[7]]);
        let h = u16::from_le_bytes([b[8], b[9]]);
        return Some(("GIF image".to_owned(), vec![dims(w.into(), h.into())]));
    }
    if b.starts_with(b"BM") && b.len() >= 26 {
        let w = i32::from_le_bytes(b[18..22].try_into().ok()?);
        let h = i32::from_le_bytes(b[22..26].try_into().ok()?);
        return Some((
            "BMP image".to_owned(),
            vec![dims(w.unsigned_abs(), h.unsigned_abs())],
        ));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        // The first start-of-frame marker has the size.
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]);
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]);
                return Some(("JPEG image".to_owned(), vec![dims(w.into(), h.into())]));
            }
            i += 2 + len;
        }
        return Some(("JPEG image".to_owned(), Vec::new()));
    }
    None
}

/// A PDF's version, page count and the title and author its Info dictionary gives as plain
/// strings. Read from the bytes as they are (a PDF is mostly not text).
fn pdf_facts(b: &[u8]) -> Vec<Fact> {
    let mut facts = Vec::new();
    let version: String = b[5..]
        .iter()
        .take(4)
        .take_while(|c| c.is_ascii_digit() || **c == b'.')
        .map(|&c| char::from(c))
        .collect();
    if !version.is_empty() {
        facts.push(("Version".to_owned(), version));
    }
    if let Some(pages) = page_count(b) {
        facts.push(("Pages".to_owned(), pages.to_string()));
    }
    // The Info dictionary the last trailer names (an incremental update appends a new one),
    // when it is plainly in the file; else the first strings found.
    let info = info_object(b).unwrap_or(b);
    for (key, label) in [
        ("/Title", "Title"),
        ("/Author", "Author"),
        ("/Producer", "Made with"),
    ] {
        if let Some(value) = info_string(info, key.as_bytes()) {
            facts.push((label.to_owned(), value));
        }
    }
    facts
}

fn find_from(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// The last `needle` in `hay[from..before]`.
fn rfind_in(hay: &[u8], from: usize, before: usize, needle: &[u8]) -> Option<usize> {
    hay.get(from..before)?
        .windows(needle.len())
        .rposition(|w| w == needle)
        .map(|i| i + from)
}

/// The object around offset `i`: from its `N G obj` to its `endobj`, when they are within
/// reach; else a few hundred bytes either side.
fn object_around(b: &[u8], i: usize) -> &[u8] {
    const REACH: usize = 4096;
    let from = i.saturating_sub(REACH);
    // The last `obj` before it, unless that is the `endobj` of the object before.
    let start = rfind_in(b, from, i, b"obj").filter(|&o| o < 3 || &b[o - 3..o] != b"end");
    let end = b[i..(i + REACH).min(b.len())]
        .windows(6)
        .position(|w| w == b"endobj")
        .map(|e| i + e);
    match (start, end) {
        (Some(s), Some(e)) => &b[s..e],
        _ => &b[i.saturating_sub(300)..(i + 300).min(b.len())],
    }
}

/// The pages: the `/Count` of the root of the page tree (a `/Type /Pages` dictionary without
/// a `/Parent`), the last one in the file: an incremental update appends the new root after the
/// old. Counting `/Page`s would be fooled by those updates and by object streams.
fn page_count(b: &[u8]) -> Option<u64> {
    let mut last = None;
    let mut at = 0;
    while let Some(i) = find_from(b, at, b"/Count") {
        at = i + 6;
        let object = object_around(b, i);
        let has = |key: &[u8]| object.windows(key.len()).any(|w| w == key);
        if !has(b"/Pages") || has(b"/Parent") {
            continue;
        }
        let digits: String = b[at..]
            .iter()
            .skip_while(|c| c.is_ascii_whitespace())
            .take_while(|c| c.is_ascii_digit())
            .map(|&c| char::from(c))
            .collect();
        if let Ok(n) = digits.parse::<u64>() {
            last = Some(n);
        }
    }
    last
}

/// The Info dictionary the last `/Info N G R` names, from its object's last `N G obj` to its
/// `endobj`; `None` when it isn't plainly in the file (in an object stream…).
fn info_object(b: &[u8]) -> Option<&[u8]> {
    let i = rfind_in(b, 0, b.len(), b"/Info")?;
    let words: Vec<&[u8]> = b[i + 5..]
        .split(|c| c.is_ascii_whitespace())
        .filter(|w| !w.is_empty())
        .take(3)
        .collect();
    let [n, g, r] = words.as_slice() else {
        return None;
    };
    if !r.starts_with(b"R") || !n.iter().chain(*g).all(u8::is_ascii_digit) {
        return None;
    }
    let head = [*n, b" ", *g, b" obj"].concat();
    let mut before = b.len();
    let start = loop {
        let o = rfind_in(b, 0, before, &head)?;
        // `15 0 obj` isn't `5 0 obj`.
        if o == 0 || !b[o - 1].is_ascii_digit() {
            break o;
        }
        before = o;
    };
    let end = find_from(b, start, b"endobj").unwrap_or(b.len());
    Some(&b[start..end])
}

/// `/Title (Report)` or `/Title(Report)`: the plain string after `key`, as Latin-1 or, when
/// marked, UTF-16.
fn info_string(b: &[u8], key: &[u8]) -> Option<String> {
    let mut at = 0;
    while let Some(i) = find_from(b, at, key) {
        at = i + key.len();
        let rest = &b[at..];
        let skip = rest.iter().take_while(|c| c.is_ascii_whitespace()).count();
        if rest.get(skip) != Some(&b'(') {
            continue;
        }
        let mut out = Vec::new();
        let mut j = skip + 1;
        while let Some(&c) = rest.get(j) {
            match c {
                b')' => break,
                b'\\' => {
                    let digits: Vec<u8> = rest[j + 1..]
                        .iter()
                        .take(3)
                        .take_while(|c| (b'0'..=b'7').contains(c))
                        .copied()
                        .collect();
                    if digits.is_empty() {
                        if let Some(&next) = rest.get(j + 1) {
                            out.push(next);
                        }
                        j += 2;
                    } else {
                        out.push(digits.iter().fold(0u32, |n, d| n * 8 + u32::from(d - b'0')) as u8);
                        j += 1 + digits.len();
                    }
                    continue;
                }
                c => out.push(c),
            }
            j += 1;
            if out.len() > 400 {
                break;
            }
        }
        let text = super::extract::latin1(&out).trim().to_owned();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::diff::binary::extract::tests::{make_docx, make_pdf};

    #[test]
    fn kinds_and_facts() {
        let pdf = make_pdf(&["x"], true);
        let (kind, facts) = describe("a.pdf", &pdf);
        assert_eq!(kind, "PDF document");
        let get = |k: &str| facts.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone());
        assert_eq!(get("Version").as_deref(), Some("1.4"));
        assert_eq!(get("Pages").as_deref(), Some("1"));
        assert_eq!(get("Title").as_deref(), Some("Report"));
        assert_eq!(get("Author").as_deref(), Some("Zoë"));
        let (kind, facts) = describe("x.docx", &make_docx(&["a"]));
        assert_eq!(kind, "Word document");
        assert_eq!(facts[0], ("Members".to_owned(), "2".to_owned()));
        assert_eq!(
            describe("lib.so", b"\x7fELF\x02").0,
            "ELF executable or library"
        );
        assert_eq!(describe("blob", b"\x00\x01\x02").0, "Binary file");
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(640u32.to_be_bytes());
        png.extend(480u32.to_be_bytes());
        png.extend([8, 6, 0, 0, 0]);
        let (kind, facts) = describe("i.png", &png);
        assert_eq!(kind, "PNG image");
        assert_eq!(facts[0].1, "640 × 480");
        assert_eq!(facts[1].1, "8 bits, RGB with alpha");
        let gif = b"GIF89a\x20\x00\x10\x00";
        assert_eq!(describe("g.gif", gif).1[0].1, "32 × 16");
        assert_eq!(describe("e.pdf", b"%PDF-").0, "PDF document");
        // Without a space, in UTF-16, and the page count of the root, not every /Page.
        let pdf = b"%PDF-1.7 /Title(\xfe\xff\x00R\x00\xe9) /Type /Pages /Kids [] /Count 12 /Type /Page /Type /Page";
        let facts = describe("u.pdf", pdf).1;
        assert!(
            facts.contains(&("Title".to_owned(), "R\u{e9}".to_owned())),
            "{facts:?}"
        );
        assert!(
            facts.contains(&("Pages".to_owned(), "12".to_owned())),
            "{facts:?}"
        );
    }

    /// After an incremental update, the pages and title are the update's: the last root of
    /// the page tree (not a branch of it), the Info the last trailer names (not a bookmark's
    /// title, not an object whose number ends the same).
    #[test]
    fn pdf_facts_of_the_last_update() -> Result<(), String> {
        let pdf = b"%PDF-1.4\n\
            1 0 obj << /Type /Outlines /Title (Bookmark) >> endobj\n\
            2 0 obj << /Type /Pages /Kids [6 0 R] /Count 10 >> endobj\n\
            5 0 obj << /Title (Draft) >> endobj\n\
            trailer << /Root 3 0 R /Info 5 0 R >>\n%%EOF\n\
            2 0 obj << /Type /Pages /Kids [6 0 R 7 0 R] /Count 9 >> endobj\n\
            6 0 obj << /Type /Pages /Parent 2 0 R /Kids [] /Count 4 >> endobj\n\
            5 0 obj << /Title (Final) /Author (Zo) >> endobj\n\
            15 0 obj << /Title (Other) >> endobj\n\
            trailer << /Root 3 0 R /Info 5 0 R /Prev 9 >>\n%%EOF\n";
        let facts = describe("u.pdf", pdf).1;
        let get = |k: &str| facts.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone());
        if get("Pages").as_deref() != Some("9") || get("Title").as_deref() != Some("Final") {
            return Err(format!("{facts:?}"));
        }
        Ok(())
    }
}
