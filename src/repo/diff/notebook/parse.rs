//! Jupyter notebooks (`.ipynb`, nbformat 4) read for diffing: their cells, each with its
//! source, execution count, outputs and metadata. Nothing in them is run.

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

/// A notebook as far as a diff needs it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notebook {
    pub cells: Vec<Cell>,
    /// The notebook's own metadata (kernel, language), pretty-printed.
    pub metadata: String,
}

impl Notebook {
    /// As the strip filter stages it: code cells without outputs or execution counts.
    pub fn stripped(mut self) -> Self {
        for c in self.cells.iter_mut().filter(|c| c.kind == CellKind::Code) {
            c.outputs.clear();
            c.execution_count = None;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    Code,
    Markdown,
    Raw,
}

impl CellKind {
    pub fn label(self) -> &'static str {
        match self {
            CellKind::Code => "code",
            CellKind::Markdown => "markdown",
            CellKind::Raw => "raw",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Its id (nbformat 4.5 and later).
    pub id: Option<String>,
    pub kind: CellKind,
    pub source: String,
    pub execution_count: Option<i64>,
    pub outputs: Vec<Output>,
    /// Its metadata, pretty-printed (`{}` when it has none).
    pub metadata: String,
}

/// One output of a code cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// What it printed (`stdout`, `stderr`).
    Stream { name: String, text: String },
    /// A result or display shown as text (`text/plain`, or what an HTML or LaTeX one says).
    Text(String),
    /// A picture (`image/png`, `image/jpeg`), decoded from base64, and the URI it is shown
    /// under (by its contents: egui keeps an image by its URI).
    Image {
        mime: String,
        bytes: Arc<[u8]>,
        uri: String,
    },
    /// An exception: its name, value and traceback (terminal colours taken out).
    Error {
        name: String,
        value: String,
        traceback: String,
    },
    /// Something with none of the above: its kinds, and a hash of what it holds (so that a
    /// change of it counts).
    Other(Vec<String>, u64),
}

/// The text of a source or output field: a string, or a list of strings to join.
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect(),
        _ => String::new(),
    }
}

/// `json` as pretty JSON, `{}` for nothing.
fn pretty(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "{}".to_owned(),
        Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
    }
}

/// The notebook in `bytes`, or why it isn't one gitcraft can read.
pub fn parse(bytes: &[u8]) -> Result<Notebook, String> {
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("Not a notebook (its JSON doesn't read: {e})"))?;
    let major = doc.get("nbformat").and_then(Value::as_i64).unwrap_or(0);
    if major < 4 {
        return Err(format!(
            "A notebook of format {major}: gitcraft reads format 4"
        ));
    }
    let cells = doc
        .get("cells")
        .and_then(Value::as_array)
        .ok_or("A notebook without cells")?
        .iter()
        .map(cell)
        .collect();
    Ok(Notebook {
        cells,
        metadata: pretty(doc.get("metadata")),
    })
}

fn cell(v: &Value) -> Cell {
    let kind = match v.get("cell_type").and_then(Value::as_str) {
        Some("markdown") => CellKind::Markdown,
        Some("raw") => CellKind::Raw,
        _ => CellKind::Code,
    };
    Cell {
        id: v.get("id").and_then(Value::as_str).map(str::to_owned),
        kind,
        source: v.get("source").map(text).unwrap_or_default(),
        execution_count: v.get("execution_count").and_then(Value::as_i64),
        outputs: v
            .get("outputs")
            .and_then(Value::as_array)
            .map(|o| o.iter().map(output).collect())
            .unwrap_or_default(),
        metadata: pretty(v.get("metadata")),
    }
}

fn output(v: &Value) -> Output {
    match v.get("output_type").and_then(Value::as_str) {
        Some("stream") => Output::Stream {
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("stdout")
                .to_owned(),
            text: cut(v.get("text").map(text).unwrap_or_default()),
        },
        Some("error") => Output::Error {
            name: v
                .get("ename")
                .and_then(Value::as_str)
                .unwrap_or("Error")
                .to_owned(),
            value: v
                .get("evalue")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            traceback: strip_ansi(
                &v.get("traceback")
                    .and_then(Value::as_array)
                    .map(|t| {
                        t.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default(),
            ),
        },
        _ => data(v.get("data")),
    }
}

/// The most lines of an output's text kept; the rest is said to be left out.
const MAX_OUTPUT_LINES: usize = 200;

/// `text` cut to [`MAX_OUTPUT_LINES`].
fn cut(text: String) -> String {
    match text
        .match_indices('\n')
        .nth(MAX_OUTPUT_LINES.saturating_sub(1))
    {
        Some((at, _)) => {
            let more = text[at..].lines().count().saturating_sub(1);
            format!("{}\n… {more} more lines", &text[..at])
        }
        None => text,
    }
}

fn hash_of(v: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// A result's richest kind gitcraft shows: a picture, else its plain text, else the source of
/// its HTML, Markdown, LaTeX or SVG.
fn data(d: Option<&Value>) -> Output {
    let Some(Value::Object(d)) = d else {
        return Output::Other(Vec::new(), 0);
    };
    for mime in ["image/png", "image/jpeg", "image/gif"] {
        if let Some(b64) = d.get(mime) {
            let encoded: String = text(b64).chars().filter(|c| !c.is_whitespace()).collect();
            if let Ok(bytes) = BASE64.decode(encoded) {
                let ext = mime.rsplit('/').next().unwrap_or("png");
                return Output::Image {
                    uri: format!("bytes://notebook/{:016x}.{ext}", hash_of(&bytes)),
                    mime: mime.to_owned(),
                    bytes: Arc::from(bytes.into_boxed_slice()),
                };
            }
        }
    }
    if let Some(t) = d.get("text/plain") {
        return Output::Text(cut(text(t)));
    }
    for mime in ["text/markdown", "text/latex", "text/html", "image/svg+xml"] {
        if let Some(t) = d.get(mime) {
            return Output::Text(cut(text(t)));
        }
    }
    Output::Other(
        d.keys().cloned().collect(),
        hash_of(&serde_json::to_string(d).unwrap_or_default()),
    )
}

/// Text without terminal colour codes (`ESC [ … m`).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 1 × 1 PNG, base64.
    pub(crate) const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    /// A notebook of `cells`: (id, kind, source, execution count, outputs JSON).
    pub(crate) fn notebook(cells: &[(&str, &str, &str, Option<i64>, &str)]) -> String {
        let cells: Vec<String> = cells
            .iter()
            .map(|(id, kind, source, count, outputs)| {
                let source = serde_json::to_string(source).unwrap_or_default();
                let count = count.map_or("null".to_owned(), |c| c.to_string());
                let extra = if *kind == "code" {
                    format!(r#","execution_count":{count},"outputs":{outputs}"#)
                } else {
                    String::new()
                };
                format!(r#"{{"id":"{id}","cell_type":"{kind}","metadata":{{}},"source":{source}{extra}}}"#)
            })
            .collect();
        format!(
            r#"{{"nbformat":4,"nbformat_minor":5,"metadata":{{"kernelspec":{{"name":"python3"}}}},"cells":[{}]}}"#,
            cells.join(",")
        )
    }

    #[test]
    fn cells_and_outputs() -> Result<(), String> {
        let outputs = format!(
            r#"[{{"output_type":"stream","name":"stdout","text":["a\n","b\n"]}},
               {{"output_type":"execute_result","data":{{"text/plain":["42"]}},"execution_count":3}},
               {{"output_type":"display_data","data":{{"image/png":"{PNG}\n","text/plain":"<Figure>"}}}},
               {{"output_type":"error","ename":"ValueError","evalue":"bad","traceback":["\u001b[31mValueError\u001b[0m: bad"]}},
               {{"output_type":"display_data","data":{{"application/vnd.widget":{{}}}}}}]"#
        );
        let nb = parse(
            notebook(&[
                ("a", "markdown", "# Title", None, ""),
                ("b", "code", "x = 1\nprint(x)", Some(3), &outputs),
            ])
            .as_bytes(),
        )?;
        assert_eq!(nb.cells.len(), 2);
        assert_eq!(nb.cells[0].kind, CellKind::Markdown);
        assert_eq!(nb.cells[1].id.as_deref(), Some("b"));
        assert_eq!(nb.cells[1].execution_count, Some(3));
        let o = &nb.cells[1].outputs;
        assert_eq!(
            o[0],
            Output::Stream {
                name: "stdout".into(),
                text: "a\nb\n".into()
            }
        );
        assert_eq!(o[1], Output::Text("42".into()));
        assert!(
            matches!(&o[2], Output::Image { mime, bytes, uri } if mime == "image/png" && bytes.starts_with(b"\x89PNG") && uri.ends_with(".png"))
        );
        assert_eq!(
            o[3],
            Output::Error {
                name: "ValueError".into(),
                value: "bad".into(),
                traceback: "ValueError: bad".into()
            }
        );
        assert!(
            matches!(&o[4], Output::Other(kinds, _) if kinds == &["application/vnd.widget".to_owned()])
        );
        assert!(nb.metadata.contains("kernelspec"));
        Ok(())
    }

    #[test]
    fn long_outputs_cut_and_html_shown() {
        let long: String = (0..500).map(|i| format!("line {i}\n")).collect();
        let out = cut(long);
        assert_eq!(out.lines().count(), 201);
        assert!(
            out.ends_with("… 300 more lines"),
            "{}",
            &out[out.len() - 30..]
        );
        let html = serde_json::json!({"text/html": ["<b>hi</b>"]});
        assert_eq!(data(Some(&html)), Output::Text("<b>hi</b>".into()));
        let a = serde_json::json!({"application/json": {"x": 1}});
        let b = serde_json::json!({"application/json": {"x": 2}});
        assert_ne!(
            data(Some(&a)),
            data(Some(&b)),
            "a change of what it holds counts"
        );
    }

    #[test]
    fn not_notebooks() {
        assert!(parse(b"").is_err());
        assert!(
            parse(b"{\"nbformat\":3,\"worksheets\":[]}").is_err_and(|e| e.contains("format 3"))
        );
        assert!(parse(b"{\"nbformat\":4}").is_err());
        assert!(parse(b"[1,2]").is_err());
    }
}
