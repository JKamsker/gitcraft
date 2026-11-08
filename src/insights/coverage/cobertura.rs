//! Cobertura XML reports (`coverage.xml`, `cobertura.xml`): each `<class filename=…>` lists
//! its `<line number=… hits=…/>`, file names relative to one of the `<source>` folders. Lines
//! repeated under a class's `<method>` are not counted again. Cobertura records no tests.

use std::collections::HashMap;

use super::{FileCov, Format, Report};

/// A tag: its name (with a leading `/` when it closes) and its attributes.
struct Tag<'a> {
    name: &'a str,
    attrs: &'a str,
}

impl Tag<'_> {
    fn attr(&self, key: &str) -> Option<String> {
        let mut rest = self.attrs;
        while let Some(eq) = rest.find('=') {
            let name = rest[..eq].trim();
            let after = rest[eq + 1..].trim_start();
            let quote = after.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let end = after[1..].find(quote)? + 1;
            if name == key {
                return Some(decode(&after[1..end]));
            }
            rest = &after[end + 1..];
        }
        None
    }
}

/// Text with XML's entities replaced.
fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';').filter(|&s| s < 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn parse(text: &str) -> Result<Report, String> {
    let mut report = Report {
        format: Format::Cobertura,
        ..Report::default()
    };
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut root = false;
    let mut class: Option<usize> = None;
    let mut methods = 0usize;
    let mut source_from: Option<usize> = None;
    let mut at = 0;
    while let Some(open) = text[at..].find('<').map(|i| at + i) {
        let rest = &text[open..];
        let skip_to = |end: &str| rest.find(end).map_or(text.len(), |i| open + i + end.len());
        if rest.starts_with("<!--") {
            at = skip_to("-->");
            continue;
        }
        if rest.starts_with("<?") {
            at = skip_to("?>");
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            at = skip_to("]]>");
            continue;
        }
        if rest.starts_with("<!") {
            at = skip_to(">");
            continue;
        }
        let Some(close) = rest.find('>').map(|i| open + i) else {
            break;
        };
        let inner = &text[open + 1..close];
        at = close + 1;
        let self_closing = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let (name, attrs) = inner
            .split_once(|c: char| c.is_whitespace())
            .unwrap_or((inner, ""));
        let tag = Tag { name, attrs };
        match tag.name {
            "coverage" => root = true,
            "source" if !self_closing => source_from = Some(at),
            "/source" => {
                if let Some(from) = source_from.take() {
                    let path = decode(text[from..open].trim());
                    if !path.is_empty() {
                        report.sources.push(path);
                    }
                }
            }
            "class" => {
                let Some(file) = tag.attr("filename") else {
                    continue;
                };
                let index = *by_path.entry(file.clone()).or_insert_with(|| {
                    report.files.push(FileCov {
                        path: file,
                        ..FileCov::default()
                    });
                    report.files.len() - 1
                });
                class = (!self_closing).then_some(index);
                methods = 0;
            }
            "/class" => class = None,
            "method" if !self_closing => methods += 1,
            "/method" => methods = methods.saturating_sub(1),
            "line" if methods == 0 => {
                let (Some(index), Some(number), Some(hits)) = (
                    class,
                    tag.attr("number").and_then(|n| n.parse::<u32>().ok()),
                    tag.attr("hits").and_then(|h| h.parse::<u64>().ok()),
                ) else {
                    continue;
                };
                let entry = report.files[index].lines.entry(number).or_default();
                *entry = (*entry).max(hits);
            }
            _ => {}
        }
    }
    if !root {
        return Err("not a Cobertura report (no <coverage> element)".to_owned());
    }
    Ok(report)
}
