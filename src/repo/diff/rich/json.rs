//! JSON read into a [`Node`] tree with the line of each key and item. Strict JSON, plus the
//! comments and trailing commas of the JSON-with-comments files editors keep (tsconfig,
//! VS Code settings). An error says on which line it is.

use super::tree::{Field, Item, MAX_DEPTH, Node, Ty};

struct Reader<'a> {
    text: &'a str,
    pos: usize,
    line: u32,
    depth: usize,
}

/// Reads `text`, or says what is wrong and where.
pub fn parse(text: &str) -> Result<Node, String> {
    let mut r = Reader {
        text: text.strip_prefix('\u{feff}').unwrap_or(text),
        pos: 0,
        line: 1,
        depth: 0,
    };
    r.space()?;
    let node = r.value()?;
    r.space()?;
    if r.pos < r.text.len() {
        return Err(r.error("more after the end of the document"));
    }
    Ok(node)
}

impl Reader<'_> {
    fn error(&self, what: &str) -> String {
        format!("line {}: {what}", self.line)
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
        }
        Some(b)
    }

    /// Skips whitespace and comments.
    fn space(&mut self) -> Result<(), String> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => {
                    self.bump();
                }
                Some(b'/') if self.text[self.pos..].starts_with("//") => {
                    while self.peek().is_some_and(|b| b != b'\n') {
                        self.bump();
                    }
                }
                Some(b'/') if self.text[self.pos..].starts_with("/*") => {
                    let Some(end) = self.text[self.pos + 2..].find("*/") else {
                        return Err(self.error("a comment that never ends"));
                    };
                    let stop = self.pos + 2 + end + 2;
                    while self.pos < stop {
                        self.bump();
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.bump();
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", b as char)))
        }
    }

    fn value(&mut self) -> Result<Node, String> {
        match self.peek() {
            Some(b'{') => self.nested(Self::object),
            Some(b'[') => self.nested(Self::array),
            Some(b'"') => Ok(Node::Scalar {
                text: self.string()?,
                ty: Ty::String,
            }),
            Some(b't') => self.word("true", Ty::Bool),
            Some(b'f') => self.word("false", Ty::Bool),
            Some(b'n') => self.word("null", Ty::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("expected a value")),
            None => Err(self.error("the document ends where a value should be")),
        }
    }

    fn nested(&mut self, read: fn(&mut Self) -> Result<Node, String>) -> Result<Node, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error(&format!("nested deeper than {MAX_DEPTH} levels")));
        }
        let node = read(self);
        self.depth -= 1;
        node
    }

    fn word(&mut self, word: &str, ty: Ty) -> Result<Node, String> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(Node::Scalar {
                text: word.to_owned(),
                ty,
            })
        } else {
            Err(self.error("expected a value"))
        }
    }

    fn number(&mut self) -> Result<Node, String> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.pos += 1;
        }
        let text = &self.text[start..self.pos];
        if text.parse::<f64>().is_err() {
            return Err(self.error(&format!("`{text}` isn't a number")));
        }
        Ok(Node::Scalar {
            text: text.to_owned(),
            ty: Ty::Number,
        })
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.pos..];
            let Some(at) = rest.find(['"', '\\', '\n']) else {
                return Err(self.error("a string that never ends"));
            };
            out.push_str(&rest[..at]);
            self.pos += at;
            match self.bump() {
                Some(b'"') => return Ok(out),
                Some(b'\\') => out.push(self.escape()?),
                _ => return Err(self.error("a line break inside a string")),
            }
        }
    }

    fn escape(&mut self) -> Result<char, String> {
        let c = match self.bump() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                let hi = self.hex4()?;
                if (0xD800..0xDC00).contains(&hi) && self.text[self.pos..].starts_with("\\u") {
                    self.pos += 2;
                    let lo = self.hex4()?;
                    let code = 0x10000 + ((hi - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                    char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER)
                } else {
                    char::from_u32(hi).unwrap_or(char::REPLACEMENT_CHARACTER)
                }
            }
            _ => return Err(self.error("an unknown escape in a string")),
        };
        Ok(c)
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.text.get(self.pos..self.pos + 4);
        let value = digits.and_then(|d| u32::from_str_radix(d, 16).ok());
        match value {
            Some(v) if digits.is_some_and(|d| d.bytes().all(|b| b.is_ascii_hexdigit())) => {
                self.pos += 4;
                Ok(v)
            }
            _ => Err(self.error("a `\\u` escape without four hex digits")),
        }
    }

    fn object(&mut self) -> Result<Node, String> {
        self.expect(b'{')?;
        let mut fields = Vec::new();
        loop {
            self.space()?;
            if self.peek() == Some(b'}') {
                self.bump();
                return Ok(Node::Map(fields));
            }
            let line = self.line;
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a key in quotes or `}`"));
            }
            let key = self.string()?;
            self.space()?;
            self.expect(b':')?;
            self.space()?;
            let node = self.value()?;
            fields.push(Field { key, node, line });
            self.space()?;
            match self.peek() {
                Some(b',') => {
                    self.bump();
                }
                Some(b'}') => {}
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn array(&mut self) -> Result<Node, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        loop {
            self.space()?;
            if self.peek() == Some(b']') {
                self.bump();
                return Ok(Node::List(items));
            }
            let line = self.line;
            let node = self.value()?;
            items.push(Item { node, line });
            self.space()?;
            match self.peek() {
                Some(b',') => {
                    self.bump();
                }
                Some(b']') => {}
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }
}
