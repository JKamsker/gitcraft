//! TOML read into a [`Node`] tree with the line of each key and item, from `toml_edit`'s
//! document, which keeps where everything is.

use toml_edit::{Document, Item as TItem, Table, Value};

use super::tree::{Field, Item, MAX_DEPTH, Node, Ty, line_at};

/// Reads `text`, or says what is wrong and where.
pub fn parse(text: &str) -> Result<Node, String> {
    let doc = Document::parse(text).map_err(|e| {
        let line = e.span().map_or(1, |s| line_at(text, s.start));
        format!("line {line}: {}", e.message().trim())
    })?;
    let mut r = Reader { text, depth: 0 };
    r.table(doc.as_table())
}

struct Reader<'a> {
    text: &'a str,
    depth: usize,
}

impl Reader<'_> {
    fn line(&self, span: Option<std::ops::Range<usize>>) -> u32 {
        span.map_or(0, |s| line_at(self.text, s.start))
    }

    fn deeper(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(format!("nested deeper than {MAX_DEPTH} levels"));
        }
        Ok(())
    }

    fn table(&mut self, table: &Table) -> Result<Node, String> {
        self.deeper()?;
        let mut fields = Vec::new();
        for (key, item) in table.iter() {
            let (k, _) = table.get_key_value(key).ok_or("a key went missing")?;
            let line = match self.line(k.span()) {
                0 => self.line(item.span()),
                n => n,
            };
            fields.push(Field {
                key: key.to_owned(),
                node: self.item(item)?,
                line,
            });
        }
        self.depth -= 1;
        Ok(Node::Map(fields))
    }

    fn item(&mut self, item: &TItem) -> Result<Node, String> {
        match item {
            TItem::None => Ok(Node::Scalar {
                text: String::new(),
                ty: Ty::Null,
            }),
            TItem::Value(v) => self.value(v),
            TItem::Table(t) => self.table(t),
            TItem::ArrayOfTables(a) => {
                self.deeper()?;
                let mut items = Vec::new();
                for t in a.iter() {
                    let line = self.line(t.span());
                    items.push(Item {
                        node: self.table(t)?,
                        line,
                    });
                }
                self.depth -= 1;
                Ok(Node::List(items))
            }
        }
    }

    fn value(&mut self, v: &Value) -> Result<Node, String> {
        let scalar = |text: String, ty| Ok(Node::Scalar { text, ty });
        match v {
            Value::String(s) => scalar(s.value().clone(), Ty::String),
            Value::Integer(i) => scalar(i.value().to_string(), Ty::Number),
            Value::Float(f) => scalar(f.value().to_string(), Ty::Number),
            Value::Boolean(b) => scalar(b.value().to_string(), Ty::Bool),
            Value::Datetime(d) => scalar(d.value().to_string(), Ty::Date),
            Value::Array(a) => {
                self.deeper()?;
                let mut items = Vec::new();
                for v in a.iter() {
                    let line = self.line(v.span());
                    items.push(Item {
                        node: self.value(v)?,
                        line,
                    });
                }
                self.depth -= 1;
                Ok(Node::List(items))
            }
            Value::InlineTable(t) => {
                self.deeper()?;
                let mut fields = Vec::new();
                for (key, value) in t.iter() {
                    let (k, _) = t.get_key_value(key).ok_or("a key went missing")?;
                    let line = self.line(k.span().or_else(|| value.span()));
                    fields.push(Field {
                        key: key.to_owned(),
                        node: self.value(value)?,
                        line,
                    });
                }
                self.depth -= 1;
                Ok(Node::Map(fields))
            }
        }
    }
}
