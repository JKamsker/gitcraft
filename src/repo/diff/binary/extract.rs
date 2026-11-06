//! The text of documents, taken out of them here: Word, Excel and PowerPoint files and their
//! OpenDocument cousins (XML in a ZIP archive), and PDFs as far as their text is plainly in
//! them. What it can't read it leaves alone: the diff then shows the rest (sizes, members, hex).
//! Every read is bounded: what an archive or a PDF may inflate, and the text kept.

use super::zip::{self, Member, Reader};

/// The most text kept of a document; past it, a line says it was cut.
pub const MAX_TEXT: usize = 4 * 1024 * 1024;
/// The most bytes a PDF's streams may inflate to, all together.
const PDF_BUDGET: usize = 48 * 1024 * 1024;

/// `text` cut to [`MAX_TEXT`] (at a character), with a line saying so.
fn capped(mut text: String) -> String {
    if text.len() > MAX_TEXT {
        let mut at = MAX_TEXT;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.truncate(at);
        text.push_str("\n… (the rest of the text isn't shown)");
    }
    text
}

/// The text of a document, a paragraph a line, if gitcraft can read its kind.
pub fn text_of(path: &str, bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(b"%PDF-") {
        return pdf_text(bytes).map(capped);
    }
    if !zip::is_zip(bytes) {
        return None;
    }
    let members = zip::members(bytes)?;
    let mut reader = Reader::new(bytes);
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let mut read = |name: &str| {
        reader
            .read_named(&members, name)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    let text = match ext.as_str() {
        "docx" | "docm" | "dotx" => read("word/document.xml").map(|x| xml_text(&x, &Rules::WORD)),
        "xlsx" | "xlsm" => Some(workbook(&members, &mut reader)),
        "pptx" | "ppsx" => Some(slides(&members, &mut reader)),
        "odt" | "ods" | "odp" | "odg" | "ott" => {
            read("content.xml").map(|x| xml_text(&x, &Rules::OPEN))
        }
        _ => None,
    }?;
    Some(capped(text))
}

/// How the text is taken out of an XML document: the elements whose characters are text
/// (any characters when empty), those ending a line, those that are a tab or a line break, a
/// run of spaces (`text:s`), and those whose insides are skipped (a text box's fallback copy).
pub struct Rules {
    pub runs: &'static [&'static str],
    pub blocks: &'static [&'static str],
    pub breaks: &'static [&'static str],
    pub spaces: Option<&'static str>,
    pub skip: &'static [&'static str],
}

impl Rules {
    const WORD: Rules = Rules {
        runs: &["w:t"],
        blocks: &["w:p"],
        breaks: &["w:tab", "w:br"],
        spaces: None,
        skip: &["mc:Fallback"],
    };
    const OPEN: Rules = Rules {
        runs: &[],
        blocks: &["text:p", "text:h"],
        breaks: &["text:tab", "text:line-break"],
        spaces: Some("text:s"),
        skip: &[],
    };
    const SLIDE: Rules = Rules {
        runs: &["a:t"],
        blocks: &["a:p"],
        breaks: &["a:br"],
        spaces: None,
        skip: &["mc:Fallback"],
    };
    const STRINGS: Rules = Rules {
        runs: &["t"],
        blocks: &["si"],
        breaks: &[],
        spaces: None,
        skip: &["rPh"],
    };
}

/// The text of each slide of a presentation, in order, under a line naming it.
fn slides(members: &[Member], reader: &mut Reader<'_>) -> String {
    let mut slides: Vec<(u32, &Member)> = members
        .iter()
        .filter_map(|m| {
            let n = m
                .name
                .strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?;
            Some((n.parse().ok()?, m))
        })
        .collect();
    slides.sort_by_key(|(n, _)| *n);
    let mut out = String::new();
    for (n, m) in slides {
        let Some(xml) = reader.read(m) else { continue };
        let text = xml_text(&String::from_utf8_lossy(&xml), &Rules::SLIDE);
        out.push_str(&format!("— Slide {n} —\n{text}\n"));
        if out.len() > MAX_TEXT {
            break;
        }
    }
    out
}

/// The cells of a workbook, sheet by sheet: `A1: value` a line, shared strings looked up,
/// inline strings and numbers as they are.
fn workbook(members: &[Member], reader: &mut Reader<'_>) -> String {
    let strings: Vec<String> = reader
        .read_named(members, "xl/sharedStrings.xml")
        .map(|x| shared_strings(&String::from_utf8_lossy(&x)))
        .unwrap_or_default();
    let mut sheets: Vec<(u32, &Member)> = members
        .iter()
        .filter_map(|m| {
            let n = m
                .name
                .strip_prefix("xl/worksheets/sheet")?
                .strip_suffix(".xml")?;
            Some((n.parse().ok()?, m))
        })
        .collect();
    sheets.sort_by_key(|(n, _)| *n);
    let mut out = String::new();
    for (n, m) in sheets {
        let Some(xml) = reader.read(m) else { continue };
        out.push_str(&format!("— Sheet {n} —\n"));
        for (cell, value) in cells(&String::from_utf8_lossy(&xml), &strings) {
            out.push_str(&format!("{cell}: {value}\n"));
            if out.len() > MAX_TEXT {
                return out;
            }
        }
    }
    out
}

/// The shared strings of a workbook, in order (one may span lines).
pub fn shared_strings(xml: &str) -> Vec<String> {
    xml.split("<si")
        .skip(1)
        .map(|si| {
            let si = si.split("</si>").next().unwrap_or(si);
            xml_text(&format!("<si{si}</si>"), &Rules::STRINGS)
        })
        .collect()
}

/// The cells of a worksheet with a value: (reference, value).
pub fn cells(xml: &str, strings: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<c ") {
        rest = &rest[at..];
        let head_end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let end = if rest[..head_end].ends_with("/>") {
            head_end
        } else {
            rest.find("</c>").map_or(rest.len(), |e| e + 4)
        };
        let cell = &rest[..end];
        rest = &rest[end..];
        let head = cell.split('>').next().unwrap_or("");
        let attr = |name: &str| {
            let key = format!(" {name}=\"");
            head.find(&key)
                .and_then(|i| head[i + key.len()..].split('"').next())
        };
        let reference = attr("r").unwrap_or("?").to_owned();
        let inner = |tag: &str| {
            let open = format!("<{tag}>");
            let i = cell.find(&open)? + open.len();
            let j = cell[i..].find(&format!("</{tag}>"))?;
            Some(decode_entities(&cell[i..i + j]))
        };
        let value = match attr("t") {
            Some("s") => inner("v")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .and_then(|i| strings.get(i).cloned()),
            Some("inlineStr") => Some(xml_text(
                cell,
                &Rules {
                    runs: &["t"],
                    blocks: &[],
                    breaks: &[],
                    spaces: None,
                    skip: &[],
                },
            )),
            _ => inner("v").or_else(|| inner("f").map(|f| format!("={f}"))),
        };
        if let Some(v) = value.filter(|v| !v.is_empty()) {
            out.push((reference, v));
        }
    }
    out
}

/// The text of an XML document, as `rules` say. Entities decoded.
pub fn xml_text(xml: &str, rules: &Rules) -> String {
    let mut out = String::new();
    let mut in_run = rules.runs.is_empty();
    let mut skipping = 0usize;
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        if in_run && skipping == 0 {
            out.push_str(&decode_entities(&rest[..lt]));
        }
        let Some(gt) = rest[lt..].find('>') else {
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        rest = &rest[lt + gt + 1..];
        let closing = tag.starts_with('/');
        let empty = tag.ends_with('/');
        let name = tag
            .trim_start_matches('/')
            .split([' ', '/', '\t', '\n', '\r'])
            .next()
            .unwrap_or("");
        if rules.skip.contains(&name) && !empty {
            if closing {
                skipping = skipping.saturating_sub(1);
            } else {
                skipping += 1;
            }
            continue;
        }
        if skipping > 0 {
            continue;
        }
        if rules.runs.contains(&name) {
            in_run = !closing && !empty;
        }
        if closing && rules.blocks.contains(&name) {
            out.push('\n');
        }
        if !closing && rules.breaks.contains(&name) {
            out.push(if name.ends_with("tab") { '\t' } else { '\n' });
        }
        if !closing && rules.spaces == Some(name) {
            let n = tag
                .split("text:c=\"")
                .nth(1)
                .and_then(|v| v.split('"').next())
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1);
            out.push_str(&" ".repeat(n.min(1000)));
        }
        if out.len() > MAX_TEXT {
            break;
        }
    }
    let lines: Vec<&str> = out.lines().map(str::trim_end).collect();
    lines.join("\n").trim_matches('\n').to_owned()
}

/// `&amp;`, `&lt;`, `&#233;`, `&#xE9;`… as the characters they stand for.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        // An entity is short: its `;` is within a dozen bytes, or it is none.
        let Some(semi) = after.bytes().take(12).position(|b| b == b';') else {
            out.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..semi];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text a PDF shows, as far as it is plainly in it: the strings of the text operators
/// (`Tj`, `TJ`, `'`, `"`) of its content streams, inflated when compressed, one stream at a
/// time within [`PDF_BUDGET`]. Fonts with their own encodings (much of what is made today)
/// come out as nothing, and then there is no text.
pub fn pdf_text(bytes: &[u8]) -> Option<String> {
    let mut text = String::new();
    let mut budget = PDF_BUDGET;
    each_content_stream(bytes, &mut budget, |stream| {
        let ops = text_operators(stream);
        if !ops.trim().is_empty() {
            text.push_str(&ops);
            text.push('\n');
        }
        text.len() < MAX_TEXT
    });
    let text = text.trim().to_owned();
    // Mostly odd characters: an encoding of its own, not text.
    let readable = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count();
    (!text.is_empty() && readable * 10 >= text.chars().count() * 7).then_some(text)
}

/// Calls `each` with every content stream of a PDF (`stream … endstream`, inflated when its
/// dictionary says FlateDecode), until it says stop or `budget` (bytes inflated) runs out.
/// Pictures, fonts, cross-reference and object streams are left out.
fn each_content_stream(bytes: &[u8], budget: &mut usize, mut each: impl FnMut(&[u8]) -> bool) {
    const SKIP: [&str; 7] = [
        "/Image",
        "/Length1",
        "/FontFile",
        "/XRef",
        "/ObjStm",
        "/Subtype",
        "/Metadata",
    ];
    let mut at = 0;
    while let Some(i) = find(&bytes[at..], b"stream") {
        let start_kw = at + i;
        let mut start = start_kw + 6;
        if bytes.get(start) == Some(&b'\r') {
            start += 1;
        }
        if bytes.get(start) == Some(&b'\n') {
            start += 1;
        }
        let Some(len) = find(&bytes[start..], b"endstream") else {
            break;
        };
        let data = &bytes[start..start + len];
        at = start + len + 9;
        // `endstream` itself matched `stream`: the next search starts past it.
        if start_kw >= 3 && &bytes[start_kw - 3..start_kw] == b"end" {
            continue;
        }
        // Its own dictionary: from the last `obj` before it, within reach.
        let window = &bytes[start_kw.saturating_sub(1024)..start_kw];
        let dict = window
            .windows(3)
            .rposition(|w| w == b"obj")
            .map_or(window, |i| &window[i..]);
        let dict = String::from_utf8_lossy(dict);
        if SKIP.iter().any(|k| dict.contains(k)) {
            continue;
        }
        let go_on = if dict.contains("FlateDecode") {
            if *budget == 0 {
                return;
            }
            match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, *budget) {
                Ok(inflated) => {
                    *budget = budget.saturating_sub(inflated.len().max(1));
                    each(&inflated)
                }
                Err(_) => true,
            }
        } else if !dict.contains("Decode") {
            each(data)
        } else {
            true
        };
        if !go_on {
            return;
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The strings shown by a content stream's text operators, a line per `BT … ET` block or
/// line move; in a `TJ` array, a large gap between strings is a space.
fn text_operators(stream: &[u8]) -> String {
    let mut out = String::new();
    let mut strings: Vec<String> = Vec::new();
    let mut i = 0;
    while i < stream.len() {
        match stream[i] {
            b'(' => {
                let (s, end) = literal_string(stream, i + 1);
                strings.push(s);
                i = end;
            }
            b'<' if stream.get(i + 1) != Some(&b'<') => {
                let end = stream[i..]
                    .iter()
                    .position(|&b| b == b'>')
                    .map_or(stream.len(), |p| i + p);
                strings.push(hex_string(&stream[i + 1..end]));
                i = end + 1;
            }
            b'%' => {
                while i < stream.len() && stream[i] != b'\n' {
                    i += 1;
                }
            }
            b'-' if !strings.is_empty() => {
                // A kerning number in a TJ array: past this much, a gap between words.
                let start = i;
                i += 1;
                while i < stream.len() && (stream[i].is_ascii_digit() || stream[i] == b'.') {
                    i += 1;
                }
                let n: f32 = std::str::from_utf8(&stream[start..i])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                if n < -200.0 {
                    strings.push(" ".to_owned());
                }
                continue;
            }
            c if c.is_ascii_alphabetic() || c == b'\'' || c == b'"' => {
                let start = i;
                while i < stream.len()
                    && (stream[i].is_ascii_alphabetic() || matches!(stream[i], b'\'' | b'"' | b'*'))
                {
                    i += 1;
                }
                match &stream[start..i] {
                    b"Tj" | b"TJ" | b"'" | b"\"" => {
                        out.push_str(&strings.concat());
                        strings.clear();
                    }
                    b"Td" | b"TD" | b"T*" | b"ET" => {
                        strings.clear();
                        if !out.ends_with('\n') && !out.is_empty() {
                            out.push('\n');
                        }
                    }
                    _ => strings.clear(),
                }
                if out.len() > MAX_TEXT {
                    break;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// A PDF literal string from after its `(`: the text and where it ends.
fn literal_string(b: &[u8], mut i: usize) -> (String, usize) {
    let mut out = Vec::new();
    let mut depth = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                i += 1;
                match b.get(i) {
                    Some(b'n') => out.push(b'\n'),
                    Some(b't') => out.push(b'\t'),
                    Some(b'r' | b'\n') => {}
                    Some(b'0'..=b'7') => {
                        let digits: Vec<u8> = b[i..]
                            .iter()
                            .take(3)
                            .take_while(|c| (b'0'..=b'7').contains(c))
                            .copied()
                            .collect();
                        let n = digits.iter().fold(0u32, |n, d| n * 8 + u32::from(d - b'0'));
                        out.push(n as u8);
                        i += digits.len() - 1;
                    }
                    Some(&c) => out.push(c),
                    None => {}
                }
            }
            b'(' => {
                depth += 1;
                out.push(b'(');
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return (latin1(&out), i + 1);
                }
                out.push(b')');
            }
            c => out.push(c),
        }
        i += 1;
    }
    (latin1(&out), i)
}

/// PDF strings in the standard encodings read near enough as Latin-1; UTF-16 when marked.
pub fn latin1(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    b.iter().map(|&c| char::from(c)).collect()
}

fn hex_string(h: &[u8]) -> String {
    let digits: Vec<u8> = h.iter().copied().filter(u8::is_ascii_hexdigit).collect();
    let bytes: Vec<u8> = digits
        .chunks(2)
        .filter_map(|p| {
            let s = std::str::from_utf8(p).ok()?;
            u8::from_str_radix(&format!("{s:0<2}"), 16).ok()
        })
        .collect();
    latin1(&bytes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::repo::diff::binary::zip::tests::make_zip;

    /// A small PDF with one page showing `lines`, its content stream deflated or not.
    pub(crate) fn make_pdf(lines: &[&str], deflate: bool) -> Vec<u8> {
        let mut content = String::from("BT /F1 12 Tf 72 720 Td\n");
        for l in lines {
            let escaped = l
                .replace('\\', "\\\\")
                .replace('(', "\\(")
                .replace(')', "\\)");
            content.push_str(&format!("({escaped}) Tj 0 -14 Td\n"));
        }
        content.push_str("ET\n");
        let (data, filter) = if deflate {
            (
                miniz_oxide::deflate::compress_to_vec_zlib(content.as_bytes(), 6),
                " /Filter /FlateDecode",
            )
        } else {
            (content.into_bytes(), "")
        };
        let mut pdf = b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
            2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
            3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n"
            .to_vec();
        pdf.extend(format!("4 0 obj << /Length {}{filter} >>\nstream\n", data.len()).as_bytes());
        pdf.extend(&data);
        pdf.extend(b"\nendstream\nendobj\n5 0 obj << /Title (Report) /Author (Zo\\353) >> endobj\ntrailer << /Root 1 0 R /Info 5 0 R >>\n%%EOF\n");
        pdf
    }

    pub(crate) fn make_docx(paragraphs: &[&str]) -> Vec<u8> {
        let body: String = paragraphs
            .iter()
            .map(|p| {
                format!(
                    "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    p.replace('&', "&amp;").replace('<', "&lt;")
                )
            })
            .collect();
        let doc =
            format!("<?xml version=\"1.0\"?><w:document><w:body>{body}</w:body></w:document>");
        make_zip(&[
            ("[Content_Types].xml", b"<Types/>", true),
            ("word/document.xml", doc.as_bytes(), true),
        ])
    }

    #[test]
    fn word_text_a_paragraph_a_line() {
        let docx = make_docx(&["Hello, wörld", "a < b & c", ""]);
        assert_eq!(
            text_of("Report.DOCX", &docx).as_deref(),
            Some("Hello, wörld\na < b & c")
        );
    }

    #[test]
    fn opendocument_and_spreadsheets_and_slides() {
        let content = "<office:text><text:h>Title</text:h><text:p>One<text:tab/>two<text:s text:c=\"2\"/>sp</text:p><text:p>x &#233; &#xE9;</text:p></office:text>";
        let odt = make_zip(&[
            (
                "mimetype",
                b"application/vnd.oasis.opendocument.text",
                false,
            ),
            ("content.xml", content.as_bytes(), true),
        ]);
        assert_eq!(
            text_of("a.odt", &odt).as_deref(),
            Some("Title\nOne\ttwo  sp\nx é é")
        );
        let strings = "<sst><si><t>Name</t></si><si><r><t>Bo</t></r><r><t>b</t></r></si><si><t>two\nlines</t></si></sst>";
        let sheet = "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\"/><c r=\"C1\" t=\"s\"><v>2</v></c></row>\
            <row r=\"2\"><c r=\"A2\" t=\"s\"><v>1</v></c><c r=\"B2\"><v>42.5</v></c><c r=\"C2\" t=\"inlineStr\"><is><t>inline &amp; more</t></is></c><c r=\"D2\"><f>B2*2</f></c></row></sheetData></worksheet>";
        let xlsx = make_zip(&[
            ("xl/sharedStrings.xml", strings.as_bytes(), true),
            ("xl/worksheets/sheet1.xml", sheet.as_bytes(), true),
        ]);
        assert_eq!(
            text_of("b.xlsx", &xlsx).as_deref(),
            Some(
                "— Sheet 1 —\nA1: Name\nC1: two\nlines\nA2: Bob\nB2: 42.5\nC2: inline & more\nD2: =B2*2\n"
            )
        );
        let slide = |t: &str| format!("<p:sld><a:p><a:r><a:t>{t}</a:t></a:r></a:p></p:sld>");
        let (s1, s10, s2) = (slide("first"), slide("tenth"), slide("second"));
        let pptx = make_zip(&[
            ("ppt/slides/slide10.xml", s10.as_bytes(), true),
            ("ppt/slides/slide1.xml", s1.as_bytes(), true),
            ("ppt/slides/slide2.xml", s2.as_bytes(), true),
        ]);
        let text = text_of("c.pptx", &pptx).unwrap_or_default();
        assert_eq!(
            text,
            "— Slide 1 —\nfirst\n— Slide 2 —\nsecond\n— Slide 10 —\ntenth\n"
        );
        // A ZIP of another kind, and not a ZIP at all: no text.
        assert_eq!(text_of("d.zip", &odt), None);
        assert_eq!(text_of("e.docx", b"not a zip"), None);
    }

    #[test]
    fn pdf_text_plain_and_deflated() {
        for deflate in [false, true] {
            let pdf = make_pdf(&["Quarterly (draft)", "Total: 42 \\ items"], deflate);
            assert_eq!(
                text_of("r.pdf", &pdf).as_deref(),
                Some("Quarterly (draft)\nTotal: 42 \\ items"),
                "{deflate}"
            );
        }
        assert_eq!(pdf_text(b"%PDF-1.7\n%%EOF"), None);
        // Hex strings, octal escapes and UTF-16.
        let stream = b"BT <48656c6c6f> Tj T* (caf\\351) Tj T* (\xfe\xff\x00h\x00i) Tj ET";
        assert_eq!(text_operators(stream), "Hello\ncafé\nhi\n");
    }

    #[test]
    fn hostile_inputs_stay_bounded() {
        // Entities without a `;`: linear, not quadratic.
        let amps = "&".repeat(300_000);
        let started = std::time::Instant::now();
        assert_eq!(decode_entities(&amps).len(), 300_000);
        assert!(started.elapsed().as_secs() < 2);
        // `\8` isn't an octal escape.
        assert_eq!(text_operators(b"BT (a\\8b) Tj ET"), "a8b\n");
        // One deflated member listed many times is read once; the budget caps the rest.
        let big = vec![b'A'; 2 * 1024 * 1024];
        let slide = [
            b"<p:sld><a:p><a:r><a:t>".as_slice(),
            &big,
            b"</a:t></a:r></a:p></p:sld>",
        ]
        .concat();
        let names: Vec<String> = (1..=3)
            .map(|i| format!("ppt/slides/slide{i}.xml"))
            .collect();
        let files: Vec<(&str, &[u8], bool)> = names
            .iter()
            .map(|n| (n.as_str(), slide.as_slice(), true))
            .collect();
        let pptx = make_zip(&files);
        let text = text_of("bomb.pptx", &pptx).unwrap_or_default();
        assert!(text.len() <= MAX_TEXT + 64, "{}", text.len());
        // Kerning gaps are spaces.
        assert_eq!(
            text_operators(b"BT [(Hello) -250 (world) -10 (!)] TJ ET"),
            "Hello world!\n"
        );
    }

    #[test]
    fn entities() {
        assert_eq!(
            decode_entities("a &amp; b &lt;c&gt; &#65;&#x42; &bogus; &"),
            "a & b <c> AB &bogus; &"
        );
    }
}
