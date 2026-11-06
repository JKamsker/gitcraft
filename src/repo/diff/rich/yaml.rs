//! YAML read into a [`Node`] tree with the line of each key and item, from the parser's
//! events. A stream of several documents is a list of them.

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
    open: Vec<(Open, u32)>,
    docs: Vec<Node>,
    too_deep: Option<u32>,
}

/// Reads `text`, or says what is wrong and where.
pub fn parse(text: &str) -> Result<Node, String> {
    let mut b = Builder::default();
    Parser::new_from_str(text)
        .load(&mut b, true)
        .map_err(|e| format!("line {}: {}", e.marker().line(), e.info()))?;
    if let Some(line) = b.too_deep {
        return Err(format!(
            "line {line}: nested deeper than {MAX_DEPTH} levels"
        ));
    }
    Ok(match b.docs.len() {
        0 => Node::Map(Vec::new()),
        1 => b.docs.remove(0),
        _ => Node::List(
            b.docs
                .into_iter()
                .map(|node| Item { node, line: 1 })
                .collect(),
        ),
    })
}

/// A plain (unquoted) scalar's type, as YAML 1.2's core schema reads it.
fn plain_type(text: &str) -> Ty {
    match text {
        "" | "~" | "null" | "Null" | "NULL" => Ty::Null,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => Ty::Bool,
        _ if text.parse::<f64>().is_ok() || text.starts_with("0x") || text.starts_with("0o") => {
            Ty::Number
        }
        _ if text.len() >= 10
            && text.as_bytes()[4] == b'-'
            && text.as_bytes()[7] == b'-'
            && text[..4].bytes().all(|b| b.is_ascii_digit()) =>
        {
            Ty::Date
        }
        _ => Ty::String,
    }
}

impl Builder {
    /// Puts a finished value where it belongs: as a key or a key's value, a list's item, or a
    /// document.
    fn place(&mut self, node: Node, line: u32) {
        match self.open.last_mut() {
            Some((Open::Map { fields, key }, _)) => match key.take() {
                Some((k, kl)) => fields.push(Field {
                    key: k,
                    node,
                    line: kl,
                }),
                None => {
                    let k = match node {
                        Node::Scalar { text, .. } => text,
                        other => other.preview(),
                    };
                    *key = Some((k, line));
                }
            },
            Some((Open::List(items), _)) => items.push(Item { node, line }),
            None => self.docs.push(node),
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
            Event::Scalar(text, style, _, _) => {
                let ty = if style == TScalarStyle::Plain {
                    plain_type(&text)
                } else {
                    Ty::String
                };
                self.place(Node::Scalar { text, ty }, line);
            }
            Event::Alias(id) => self.place(
                Node::Scalar {
                    text: format!("*{id}"),
                    ty: Ty::Alias,
                },
                line,
            ),
            Event::MappingStart(..) | Event::SequenceStart(..) => {
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
                self.open.push((open, line));
            }
            Event::MappingEnd | Event::SequenceEnd => {
                if let Some((open, start)) = self.open.pop() {
                    let node = match open {
                        Open::Map { fields, .. } => Node::Map(fields),
                        Open::List(items) => Node::List(items),
                    };
                    self.place(node, start);
                }
            }
            _ => {}
        }
    }
}
