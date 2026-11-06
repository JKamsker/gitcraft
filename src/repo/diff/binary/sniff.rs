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

/// A PDF's version, page count and the title and author its Info dictionary gives in plain
/// text.
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
    let text = String::from_utf8_lossy(b);
    let pages = text.matches("/Type /Page").count() + text.matches("/Type/Page").count()
        - text.matches("/Type /Pages").count()
        - text.matches("/Type/Pages").count();
    if pages > 0 {
        facts.push(("Pages".to_owned(), pages.to_string()));
    }
    for (key, label) in [
        ("/Title", "Title"),
        ("/Author", "Author"),
        ("/Producer", "Made with"),
    ] {
        if let Some(value) = info_string(&text, key) {
            facts.push((label.to_owned(), value));
        }
    }
    facts
}

/// `/Title (Report)`: the plain string after `key`.
fn info_string(text: &str, key: &str) -> Option<String> {
    let at = text.find(&format!("{key} ("))? + key.len() + 2;
    let rest = &text[at..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            ')' => break,
            '\\' => {
                let digits: String = chars
                    .clone()
                    .take(3)
                    .take_while(|c| ('0'..='7').contains(c))
                    .collect();
                if digits.is_empty() {
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                } else {
                    for _ in 0..digits.len() {
                        chars.next();
                    }
                    out.push(char::from(u8::from_str_radix(&digits, 8).unwrap_or(b'?')));
                }
            }
            c => out.push(c),
        }
        if out.len() > 200 {
            break;
        }
    }
    let out = out.trim().to_owned();
    (!out.is_empty()).then_some(out)
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
    }
}
