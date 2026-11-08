//! YAML read into a [`Node`] tree with the line of each key and item, from the parser's
//! events. A stream of several documents is a list of them.

use std::collections::HashMap;

use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

use super::tree::{Field, Item, MAX_DEPTH, Node, Ty};

/// A map or list being read: what it holds so far, and a key waiting for its value.
enum Open {
    Map {
        fields: Vec<Field>,
        key: Option<(String, u32)>,
    },
    List(Vec<Item>),
}

#[derive(Default)]
struct Builder {
    /// Maps and lists being read: each with its line and its anchor (0: none).
    open: Vec<(Open, u32, usize)>,
    /// The documents read, with the line each starts on.
    docs: Vec<(Node, u32)>,
    doc_line: u32,
    too_deep: Option<u32>,
    /// What each anchor (`&name`) holds, for the aliases (`*name`) that repeat it.
    anchors: HashMap<usize, Node>,
    /// Values still allowed to be copied for aliases: an alias of aliases of aliases… could
    /// otherwise grow without end.
    copies: usize,
}

/// How many values may be copied in all for aliases.
const MAX_ALIAS_COPIES: usize = 200_000;

fn size(node: &Node) -> usize {
    match node {
        Node::Scalar { .. } => 1,
        Node::Map(fields) => 1 + fields.iter().map(|f| size(&f.node)).sum::<usize>(),
        Node::List(items) => 1 + items.iter().map(|i| size(&i.node)).sum::<usize>(),
    }
}

/// Reads `text`, or says what is wrong and where.
pub fn parse(text: &str) -> Result<Node, String> {
    let mut b = Builder {
        copies: MAX_ALIAS_COPIES,
        ..Builder::default()
    };
    Parser::new_from_str(text).load(&mut b, true).map_err(|e| {
        crate::i18n::message(
            "repo-core-2281",
            &[
                ("line", format!("{}", e.marker().line()).into()),
                ("info", format!("{}", e.info()).into()),
            ],
        )
    })?;
    if let Some(line) = b.too_deep {
        return Err(crate::i18n::message(
            "repo-core-2282",
            &[
                ("line", (line).into()),
                ("MAX-DEPTH", format!("{}", MAX_DEPTH).into()),
            ],
        ));
    }
    Ok(match b.docs.len() {
        0 => Node::Map(Vec::new()),
        1 => b.docs.remove(0).0,
        _ => Node::List(
            b.docs
                .into_iter()
                .map(|(node, line)| Item { node, line })
                .collect(),
        ),
    })
}

/// A plain (unquoted) scalar's type, as YAML 1.2's core schema reads it.
fn plain_type(text: &str) -> Ty {
    let unsigned = text.trim_start_matches(['-', '+']);
    let numeric = unsigned.starts_with(|c: char| c.is_ascii_digit())
        || (unsigned.starts_with('.') && unsigned[1..].starts_with(|c: char| c.is_ascii_digit()));
    match text {
        "" | "~" | "null" | "Null" | "NULL" => Ty::Null,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => Ty::Bool,
        _ if matches!(unsigned, ".inf" | ".Inf" | ".INF")
            || matches!(text, ".nan" | ".NaN" | ".NAN") =>
        {
            Ty::Number
        }
        _ if numeric && text.parse::<f64>().is_ok() => Ty::Number,
        _ if text.starts_with("0x")
            && text.len() > 2
            && text[2..].bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            Ty::Number
        }
        _ if text.starts_with("0o")
            && text.len() > 2
            && text[2..].bytes().all(|b| (b'0'..=b'7').contains(&b)) =>
        {
            Ty::Number
        }
        _ if is_date(text) => Ty::Date,
        _ => Ty::String,
    }
}

/// `2024-01-02`, optionally with a time after a `T`, a `t` or a space.
fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    let day = b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit);
    day && (b.len() == 10
        || (matches!(b[10], b'T' | b't' | b' ') && b.len() > 11 && b[11].is_ascii_digit()))
}

impl Builder {
    /// Puts a finished value where it belongs: as a key or a key's value, a list's item, or a
    /// document.
    /// Keeps what `anchor` holds, for its aliases.
    fn anchor(&mut self, anchor: usize, node: &Node) {
        if anchor > 0 {
            self.anchors.insert(anchor, node.clone());
        }
    }

    fn place(&mut self, node: Node, line: u32) {
        match self.open.last_mut() {
            Some((Open::Map { fields, key }, ..)) => match key.take() {
                Some((k, kl)) => fields.push(Field {
                    key: k,
                    node,
                    line: kl,
                }),
                None => {
                    // A key that is a map or a list is named by all it holds.
                    let k = match node {
                        Node::Scalar { text, .. } => text,
                        other => canonical(&other),
                    };
                    *key = Some((k, line));
                }
            },
            Some((Open::List(items), ..)) => items.push(Item { node, line }),
            None => self.docs.push((node, self.doc_line.max(1))),
        }
    }
}

impl MarkedEventReceiver for Builder {
    fn on_event(&mut self, ev: Event, mark: Marker) {
        if self.too_deep.is_some() {
            return;
        }
        let line = mark.line() as u32;
        match ev {
            Event::DocumentStart => self.doc_line = line,
            Event::Scalar(text, style, anchor, tag) => {
                // `!!str 1` is a string; another tag leaves the type as written.
                let str_tag = tag.as_ref().is_some_and(|t| {
                    t.suffix == "str" && (t.handle == "!!" || t.handle.contains("yaml.org"))
                });
                let ty = if style == TScalarStyle::Plain && !str_tag {
                    plain_type(&text)
                } else {
                    Ty::String
                };
                let node = Node::Scalar { text, ty };
                self.anchor(anchor, &node);
                self.place(node, line);
            }
            // An alias is what its anchor holds, so a change to the anchor shows where it is
            // used, and renaming anchors changes nothing.
            Event::Alias(id) => {
                let node = match self.anchors.get(&id) {
                    Some(n) if size(n) <= self.copies => {
                        self.copies -= size(n);
                        n.clone()
                    }
                    _ => Node::Scalar {
                        text: crate::i18n::text("repo-core-2299").to_owned(),
                        ty: Ty::Alias,
                    },
                };
                self.place(node, line);
            }
            Event::MappingStart(anchor, _) | Event::SequenceStart(anchor, _) => {
                if self.open.len() >= MAX_DEPTH {
                    self.too_deep = Some(line);
                    return;
                }
                let open = if matches!(ev, Event::MappingStart(..)) {
                    Open::Map {
                        fields: Vec::new(),
                        key: None,
                    }
                } else {
                    Open::List(Vec::new())
                };
                self.open.push((open, line, anchor));
            }
            Event::MappingEnd | Event::SequenceEnd => {
                if let Some((open, start, anchor)) = self.open.pop() {
                    let node = match open {
                        Open::Map { fields, .. } => Node::Map(fields),
                        Open::List(items) => Node::List(items),
                    };
                    self.anchor(anchor, &node);
                    self.place(node, start);
                }
            }
            _ => {}
        }
    }
}

/// A map or list written out in full on one line, to name a key that is one.
fn canonical(node: &Node) -> String {
    match node {
        Node::Scalar { text, .. } => text.clone(),
        Node::Map(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.key, canonical(&f.node)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        Node::List(items) => {
            let parts: Vec<String> = items.iter().map(|i| canonical(&i.node)).collect();
            format!("[{}]", parts.join(", "))
        }
    }
}
