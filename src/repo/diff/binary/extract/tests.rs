use super::pdf::{direct_length, each_content_stream, text_operators};
use super::*;
use crate::repo::diff::binary::zip::tests::{make_zip, with_method};

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
    let doc = format!("<?xml version=\"1.0\"?><w:document><w:body>{body}</w:body></w:document>");
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
        "— Slide 1 —\nfirst\n— Slide 2 —\nsecond\n— Slide 3 —\ntenth\n"
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

/// Slides in the order the presentation shows them, not their file names'; one it names
/// but hasn't is a gap.
#[test]
fn slides_in_show_order() -> Result<(), String> {
    let slide = |t: &str| format!("<p:sld><a:p><a:r><a:t>{t}</a:t></a:r></a:p></p:sld>");
    let (s1, s2) = (slide("made first"), slide("made second"));
    let presentation = "<p:presentation><p:sldIdLst><p:sldId id=\"257\" r:id=\"rId3\"/><p:sldId id=\"256\" r:id=\"rId2\"/><p:sldId id=\"258\" r:id=\"rId4\"/></p:sldIdLst></p:presentation>";
    let rels = "<Relationships><Relationship Id=\"rId2\" Type=\"slide\" Target=\"slides/slide1.xml\"/><Relationship Id=\"rId3\" Type=\"slide\" Target=\"/ppt/slides/slide2.xml\"/><Relationship Id=\"rId4\" Type=\"slide\" Target=\"slides/slide9.xml\"/></Relationships>";
    let pptx = make_zip(&[
        ("ppt/presentation.xml", presentation.as_bytes(), true),
        ("ppt/_rels/presentation.xml.rels", rels.as_bytes(), true),
        ("ppt/slides/slide1.xml", s1.as_bytes(), true),
        ("ppt/slides/slide2.xml", s2.as_bytes(), true),
    ]);
    let t = document_text("p.pptx", &pptx).ok_or("no text")?;
    if t.text != "— Slide 1 —\nmade second\n— Slide 2 —\nmade first\n" {
        return Err(t.text);
    }
    if t.gaps != ["slide 3 couldn't be read"] {
        return Err(format!("{:?}", t.gaps));
    }
    Ok(())
}

/// A sheet that can't be read is a gap, not cells silently gone; a damaged directory too.
#[test]
fn unread_sheets_are_gaps() -> Result<(), String> {
    let sheet =
        "<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>";
    let xlsx = make_zip(&[
        ("xl/worksheets/sheet1.xml", sheet.as_bytes(), true),
        ("xl/worksheets/sheet2.xml", sheet.as_bytes(), true),
    ]);
    let xlsx = with_method(xlsx, "xl/worksheets/sheet2.xml", 99);
    let t = document_text("b.xlsx", &xlsx).ok_or("no text")?;
    if t.text != "— Sheet 1 —\nA1: 1\n" || t.gaps != ["sheet 2 couldn't be read"] {
        return Err(format!("{:?} {:?}", t.text, t.gaps));
    }
    let mut cut = make_zip(&[("xl/worksheets/sheet1.xml", sheet.as_bytes(), true)]);
    let n = cut.len();
    cut[n - 12..n - 10].copy_from_slice(&2u16.to_le_bytes());
    let t = document_text("b.xlsx", &cut).ok_or("no text")?;
    if !t.gaps.iter().any(|g| g.contains("directory is damaged")) {
        return Err(format!("{:?}", t.gaps));
    }
    Ok(())
}

/// A formula's cell shows the formula next to its cached value.
#[test]
fn formulas_with_their_values() -> Result<(), String> {
    let sheet = "<c r=\"A1\"><f>B1*2</f><v>84</v></c><c r=\"A2\"><f t=\"shared\" ref=\"A2:A3\" si=\"0\">B2+1</f><v>3</v></c><c r=\"A3\"><f t=\"shared\" si=\"0\"/><v>4</v></c><c r=\"A4\"><f>NOW()</f></c>";
    let got = cells(sheet, &[]);
    let want: Vec<(String, String)> = [
        ("A1", "84 (=B1*2)"),
        ("A2", "3 (=B2+1)"),
        ("A3", "4"),
        ("A4", "=NOW()"),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
    .collect();
    if got != want {
        return Err(format!("{got:?}"));
    }
    Ok(())
}

/// A stream's end is where its `/Length` says, though its data holds `endstream`; a
/// reference for its length falls back to the search.
#[test]
fn pdf_streams_end_where_their_length_says() -> Result<(), String> {
    let content = b"BT (one endstream two) Tj ET";
    let mut pdf = b"%PDF-1.4\n4 0 obj << /Length ".to_vec();
    pdf.extend(content.len().to_string().as_bytes());
    pdf.extend(b" >>\nstream\n");
    pdf.extend(content);
    pdf.extend(b"\nendstream\nendobj\n%%EOF\n");
    if pdf_text(&pdf).as_deref() != Some("one endstream two") {
        return Err(format!("{:?}", pdf_text(&pdf)));
    }
    if direct_length("<< /Length 12 0 R >>").is_some()
        || direct_length("<< /Length1 5 /Length 7/Filter /FlateDecode >>") != Some(7)
    {
        return Err("lengths misread".into());
    }
    Ok(())
}

/// A `TJ` of many strings keeps at most [`MAX_TEXT`]; streams left for the budget are said.
#[test]
fn pdf_strings_and_streams_bounded() -> Result<(), String> {
    let mut stream = b"BT [".to_vec();
    for _ in 0..1_200_000 {
        stream.extend(b"(abcd)");
    }
    stream.extend(b"] TJ ET");
    let out = text_operators(&stream);
    if out.len() > MAX_TEXT + 1 {
        return Err(format!("{}", out.len()));
    }
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(&[b'a'; 1000], 6);
    let mut pdf = b"%PDF-1.4\n4 0 obj << /Filter /FlateDecode >>\nstream\n".to_vec();
    pdf.extend(&packed);
    pdf.extend(b"\nendstream\nendobj\n");
    let mut budget = 10;
    if !each_content_stream(&pdf, &mut budget, |_| true) {
        return Err("an exhausted budget not told".into());
    }
    let mut budget = 10_000;
    if each_content_stream(&pdf, &mut budget, |_| true) {
        return Err("a budget that lasted told exhausted".into());
    }
    Ok(())
}
