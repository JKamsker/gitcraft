//! The text of documents, taken out of them here: Word, Excel and PowerPoint files and their
//! OpenDocument cousins (XML in a ZIP archive), and PDFs as far as their text is plainly in
//! them. What it can't read it leaves alone: the diff then shows the rest (sizes, members, hex).
//! Every read is bounded: what an archive or a PDF may inflate, and the text kept.

use super::zip::{self, Member, Reader};

/// The most text kept of a document; past it, a line says it was cut.
pub const MAX_TEXT: usize = 4 * 1024 * 1024;
/// The most bytes a PDF's streams may inflate to, all together.
const PDF_BUDGET: usize = 48 * 1024 * 1024;

/// The text of a document, and what of it couldn't be read (a slide too big, a sheet past the
/// archive's budget…): a comparison of text with gaps isn't the whole story.
#[derive(Debug, Default)]
pub struct Text {
    pub text: String,
    pub gaps: Vec<String>,
}

/// `text` cut to [`MAX_TEXT`] (at a character), with a line saying so.
fn capped(mut text: String) -> String {
    if text.len() > MAX_TEXT {
        let mut at = MAX_TEXT;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.truncate(at);
        text.push_str(crate::i18n::text("repo-core-1563"));
    }
    text
}

/// The text of a document, a paragraph a line, if gitcraft can read its kind.
#[cfg(test)]
pub fn text_of(path: &str, bytes: &[u8]) -> Option<String> {
    document_text(path, bytes).map(|t| t.text)
}

/// The text of a document, a paragraph a line, and what of it couldn't be read, if gitcraft
/// can read its kind.
pub fn document_text(path: &str, bytes: &[u8]) -> Option<Text> {
    let mut gaps = Vec::new();
    if bytes.starts_with(b"%PDF-") {
        let text = pdf_text_with_gaps(bytes, &mut gaps)?;
        return Some(finish(text, gaps));
    }
    if !zip::is_zip(bytes) {
        return None;
    }
    let directory = zip::directory(bytes)?;
    if directory.members.is_empty() && !directory.complete {
        return None;
    }
    let members = directory.members;
    if !directory.complete {
        gaps.push(crate::i18n::text("repo-core-3808").to_owned());
    }
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
        "xlsx" | "xlsm" => Some(workbook(&members, &mut reader, &mut gaps)),
        "pptx" | "ppsx" => Some(slides(&members, &mut reader, &mut gaps)),
        "odt" | "ods" | "odp" | "odg" | "ott" => {
            read("content.xml").map(|x| xml_text(&x, &Rules::OPEN))
        }
        _ => None,
    }?;
    Some(finish(text, gaps))
}

/// The text capped, a gap said when it was cut.
fn finish(text: String, mut gaps: Vec<String>) -> Text {
    if text.len() > MAX_TEXT {
        gaps.push(crate::i18n::message(
            "repo-core-3814",
            &[("MAX-TEXT", format!("{}", MAX_TEXT / (1024 * 1024)).into())],
        ));
    }
    Text {
        text: capped(text),
        gaps,
    }
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

/// The text of each slide of a presentation, in the order it shows them (its
/// `presentation.xml`), under a line naming it; by their file names when that order can't be
/// read. A slide that can't be read is a gap.
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

#[cfg(test)]
pub(crate) mod tests;

mod pdf;
pub use pdf::latin1;
#[cfg(test)]
pub use pdf::pdf_text;
use pdf::pdf_text_with_gaps;

mod office;
#[cfg(test)]
use office::cells;
use office::{slides, workbook};
