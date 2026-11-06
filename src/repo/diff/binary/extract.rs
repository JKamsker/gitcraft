//! The text of documents, taken out of them here: Word, Excel and PowerPoint files and their
//! OpenDocument cousins (XML in a ZIP archive), and PDFs as far as their text is plainly in
//! them. What it can't read it leaves alone: the diff then shows the rest (sizes, members, hex).

use super::zip::{self, Member};

/// The text of a document, a paragraph a line, if gitcraft can read its kind.
pub fn text_of(path: &str, bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(b"%PDF-") {
        return pdf_text(bytes);
    }
    if !zip::is_zip(bytes) {
        return None;
    }
    let members = zip::members(bytes)?;
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let read = |name: &str| {
        zip::read_named(bytes, &members, name).map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    match ext.as_str() {
        "docx" | "docm" | "dotx" => {
            read("word/document.xml").map(|x| xml_text(&x, &["w:p"], &["w:t"], &["w:tab", "w:br"]))
        }
        "xlsx" | "xlsm" => read("xl/sharedStrings.xml").map(|x| xml_text(&x, &["si"], &["t"], &[])),
        "pptx" | "ppsx" => Some(slides(bytes, &members)),
        "odt" | "ods" | "odp" | "odg" | "ott" => read("content.xml").map(|x| {
            xml_text(
                &x,
                &["text:p", "text:h"],
                &[],
                &["text:tab", "text:line-break"],
            )
        }),
        _ => None,
    }
}

/// The text of each slide of a presentation, in order, under a line naming it.
fn slides(bytes: &[u8], members: &[Member]) -> String {
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
        let Some(xml) = zip::read(bytes, m) else {
            continue;
        };
        let text = xml_text(
            &String::from_utf8_lossy(&xml),
            &["a:p"],
            &["a:t"],
            &["a:br"],
        );
        out.push_str(&format!("— Slide {n} —\n{text}\n"));
    }
    out
}

/// The text of an XML document: the characters inside `runs` elements (any text when empty),
/// a line break after each `blocks` element, a tab or a line for `breaks`. Entities decoded.
pub fn xml_text(xml: &str, blocks: &[&str], runs: &[&str], breaks: &[&str]) -> String {
    let mut out = String::new();
    let mut in_run = runs.is_empty();
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        if in_run {
            out.push_str(&decode_entities(&rest[..lt]));
        }
        let Some(gt) = rest[lt..].find('>') else {
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        rest = &rest[lt + gt + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split([' ', '/', '\t', '\n'])
            .next()
            .unwrap_or("");
        if runs.contains(&name) {
            in_run = !closing && !tag.ends_with('/');
        }
        if closing && blocks.contains(&name) {
            out.push('\n');
        }
        if !closing && breaks.contains(&name) {
            out.push(if name.ends_with("tab") { '\t' } else { '\n' });
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
        let Some(semi) = after.find(';').filter(|&i| i <= 10) else {
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
/// (`Tj`, `TJ`, `'`, `"`) of its content streams, inflated when compressed. Fonts with their
/// own encodings (much of what is made today) come out as nothing, and then there is no text.
pub fn pdf_text(bytes: &[u8]) -> Option<String> {
    let mut text = String::new();
    for stream in pdf_streams(bytes) {
        let ops = text_operators(&stream);
        if !ops.trim().is_empty() {
            text.push_str(&ops);
            text.push('\n');
        }
    }
    let text = text.trim().to_owned();
    // Mostly odd characters: an encoding of its own, not text.
    let readable = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count();
    (!text.is_empty() && readable * 10 >= text.chars().count() * 7).then_some(text)
}

/// The content of each `stream … endstream` of a PDF, inflated when its dictionary says
/// FlateDecode; others (images…) are left out.
fn pdf_streams(bytes: &[u8]) -> Vec<Vec<u8>> {
    const MAX: usize = 16 * 1024 * 1024;
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = find(&bytes[at..], b"stream") {
        let start_kw = at + i;
        // Its own dictionary: from the last `obj` before it, within reach.
        let window = &bytes[start_kw.saturating_sub(1024)..start_kw];
        let dict = window
            .windows(3)
            .rposition(|w| w == b"obj")
            .map_or(window, |i| &window[i..]);
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
        let dict_text = String::from_utf8_lossy(dict);
        if dict_text.contains("/Image") {
            continue;
        }
        if dict_text.contains("FlateDecode") {
            if let Ok(inflated) = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, MAX)
            {
                out.push(inflated);
            }
        } else if !dict_text.contains("Decode") {
            out.push(data.to_vec());
        }
        if out.len() > 10_000 {
            break;
        }
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The strings shown by a content stream's text operators, a line per `BT … ET` block or
/// line move.
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
                    Some(d) if d.is_ascii_digit() => {
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
fn latin1(b: &[u8]) -> String {
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
        let content = "<office:text><text:h>Title</text:h><text:p>One<text:tab/>two</text:p><text:p>x &#233; &#xE9;</text:p></office:text>";
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
            Some("Title\nOne\ttwo\nx é é")
        );
        let strings = "<sst><si><t>Name</t></si><si><r><t>Bo</t></r><r><t>b</t></r></si></sst>";
        let xlsx = make_zip(&[("xl/sharedStrings.xml", strings.as_bytes(), true)]);
        assert_eq!(text_of("b.xlsx", &xlsx).as_deref(), Some("Name\nBob"));
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
    fn entities() {
        assert_eq!(
            decode_entities("a &amp; b &lt;c&gt; &#65;&#x42; &bogus; &"),
            "a & b <c> AB &bogus; &"
        );
    }
}
