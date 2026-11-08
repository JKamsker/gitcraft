//! A light reading of a Rust file: where its items (functions, types, traits, impl blocks,
//! constants, modules, macros) start and end, which are `pub` and which are tests. Comments,
//! strings, raw strings and character literals are blanked out first, so what they hold
//! is never taken for code. No type checking and no macro expansion: what a macro generates
//! isn't seen.

/// What an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemKind {
    Fn,
    Struct,
    Enum,
    Union,
    Trait,
    Impl,
    Const,
    Static,
    Type,
    Mod,
    Macro,
}

impl ItemKind {
    pub fn word(self) -> &'static str {
        match self {
            ItemKind::Fn => "fn",
            ItemKind::Struct => "struct",
            ItemKind::Enum => "enum",
            ItemKind::Union => "union",
            ItemKind::Trait => "trait",
            ItemKind::Impl => "impl",
            ItemKind::Const => "const",
            ItemKind::Static => "static",
            ItemKind::Type => "type",
            ItemKind::Mod => "mod",
            ItemKind::Macro => "macro_rules!",
        }
    }
}

/// An item of a file. Lines count from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub kind: ItemKind,
    /// Its name (an impl block's: the type it is for), which other code refers to it by.
    pub name: String,
    /// Its name within the items around it (`Repo::new`, `tests::it_works`, `impl Display for
    /// Repo`): tells same-named items of a file apart.
    pub path: String,
    /// Its first line, its doc comment and attributes included.
    pub start: u32,
    /// The line its name is on.
    pub line: u32,
    pub end: u32,
    /// Declared `pub` (not `pub(crate)` or narrower).
    pub public: bool,
    /// A test, or inside one: `#[test]`, `#[cfg(test)]` or a module named `tests`.
    pub test: bool,
}

impl Item {
    pub fn contains(&self, line: u32) -> bool {
        (self.start..=self.end).contains(&line)
    }
}

/// The innermost item `line` is in.
pub fn innermost(items: &[Item], line: u32) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.contains(line))
        .min_by_key(|(_, item)| item.end - item.start)
        .map(|(i, _)| i)
}

/// Whether `line` is in a test.
pub fn in_test(items: &[Item], line: u32) -> bool {
    items.iter().any(|item| item.test && item.contains(line))
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// `text` with what its comments, strings and character literals hold turned into spaces,
/// their delimiters too; line breaks are kept, so lines stay where they were.
pub fn mask(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let blank = |out: &mut String, from: &[char]| {
        for &c in from {
            out.push(if c == '\n' { '\n' } else { ' ' });
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let after_ident = i > 0 && is_ident(chars[i - 1]);
        let end = if c == '/' && next == Some('/') {
            Some(
                chars[i..]
                    .iter()
                    .position(|&c| c == '\n')
                    .map_or(chars.len(), |p| i + p),
            )
        } else if c == '/' && next == Some('*') {
            Some(block_comment_end(&chars, i))
        } else if c == '"' {
            Some(string_end(&chars, i + 1))
        } else if after_ident {
            None
        } else if let Some(end) = raw_string_end(&chars, i) {
            Some(end)
        } else if matches!(c, 'b' | 'c') && next == Some('"') {
            Some(string_end(&chars, i + 2))
        } else if c == 'b' && next == Some('\'') {
            char_end(&chars, i + 1)
        } else if c == '\'' {
            char_end(&chars, i)
        } else {
            None
        };
        match end {
            Some(end) => {
                blank(&mut out, &chars[i..end]);
                i = end;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Where the block comment starting at `i` ends (after its `*/`), comments in it nested.
fn block_comment_end(chars: &[char], i: usize) -> usize {
    let mut depth = 0;
    let mut j = i;
    while j < chars.len() {
        if chars[j] == '/' && chars.get(j + 1) == Some(&'*') {
            depth += 1;
            j += 2;
        } else if chars[j] == '*' && chars.get(j + 1) == Some(&'/') {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return j;
            }
        } else {
            j += 1;
        }
    }
    chars.len()
}

/// Where the string whose text starts at `from` ends (after its closing quote).
fn string_end(chars: &[char], from: usize) -> usize {
    let mut j = from;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            '"' => return j + 1,
            _ => j += 1,
        }
    }
    chars.len()
}

/// Where a raw string (`r"…"`, `br#"…"#`, `cr##"…"##`) starting at `i` ends; `None` when none
/// starts there (a raw identifier `r#name` is not one).
fn raw_string_end(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if matches!(chars.get(j), Some('b' | 'c')) {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if chars.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    while j < chars.len() {
        if chars[j] == '"' && (1..=hashes).all(|k| chars.get(j + k) == Some(&'#')) {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(chars.len())
}

/// Where the character literal starting with the quote at `i` ends; `None` when it is a
/// lifetime or a label (`'a`, `'static`).
fn char_end(chars: &[char], i: usize) -> Option<usize> {
    match chars.get(i + 1) {
        Some('\\') => {
            // `'\n'`, `'\''`, `'\u{1F600}'`: the escaped character, then up to the quote.
            let close = chars
                .get(i + 3..)?
                .iter()
                .take(10)
                .position(|&c| c == '\'')?;
            Some(i + 3 + close + 1)
        }
        Some(&c) if c != '\'' && chars.get(i + 2) == Some(&'\'') => Some(i + 3),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Punct(char),
    /// A lifetime or a label.
    Life,
}

/// The tokens of masked text, each with its line. Numbers are left out.
fn tokens(masked: &str) -> Vec<(Tok, u32)> {
    let chars: Vec<char> = masked.chars().collect();
    let mut out = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '\'' && chars.get(i + 1).is_some_and(|&n| is_ident_start(n)) {
            i += 1;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            out.push((Tok::Life, line));
        } else if c.is_ascii_digit() {
            while i < chars.len() && (is_ident(chars[i]) || chars[i] == '.') {
                // A range (`0..n`) isn't part of the number.
                if chars[i] == '.' && chars.get(i + 1) == Some(&'.') {
                    break;
                }
                i += 1;
            }
        } else if is_ident_start(c) {
            let mut j = i;
            while j < chars.len() && is_ident(chars[j]) {
                j += 1;
            }
            let mut name: String = chars[i..j].iter().collect();
            // A raw identifier, `r#match`.
            if name == "r"
                && chars.get(j) == Some(&'#')
                && chars.get(j + 1).is_some_and(|&n| is_ident_start(n))
            {
                let k = j + 1;
                j = k;
                while j < chars.len() && is_ident(chars[j]) {
                    j += 1;
                }
                name = chars[k..j].iter().collect();
            }
            out.push((Tok::Ident(name), line));
            i = j;
        } else {
            out.push((Tok::Punct(c), line));
            i += 1;
        }
    }
    out
}

/// For each opening bracket, where it closes.
fn pairs(toks: &[(Tok, u32)]) -> Vec<Option<usize>> {
    let mut pair = vec![None; toks.len()];
    let mut open: Vec<(char, usize)> = Vec::new();
    for (i, (tok, _)) in toks.iter().enumerate() {
        let Tok::Punct(c) = tok else {
            continue;
        };
        let opener = match c {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            '(' | '[' | '{' => {
                open.push((*c, i));
                continue;
            }
            _ => continue,
        };
        if let Some(at) = open.iter().rposition(|(o, _)| *o == opener) {
            pair[open[at].1] = Some(i);
            open.truncate(at);
        }
    }
    pair
}

/// Words that may come before an item's keyword.
const MODIFIERS: [&str; 6] = ["async", "unsafe", "extern", "default", "safe", "auto"];

/// What has been read of an item before its keyword: its attributes and modifiers.
#[derive(Default)]
struct Pending {
    start: Option<u32>,
    test: bool,
    public: bool,
}

/// The items of the Rust file `text`, in the order they start.
pub fn scan(text: &str) -> Vec<Item> {
    let masked = mask(text);
    let toks = tokens(&masked);
    let pair = pairs(&toks);
    let last_line = toks.last().map_or(1, |(_, l)| *l);
    let ident = |i: usize| match toks.get(i) {
        Some((Tok::Ident(s), _)) => Some(s.as_str()),
        _ => None,
    };
    let punct = |i: usize, c: char| matches!(toks.get(i), Some((Tok::Punct(p), _)) if *p == c);
    let end_line = |i: Option<usize>| i.and_then(|i| toks.get(i)).map_or(last_line, |(_, l)| *l);
    let lines: Vec<&str> = text.lines().collect();
    let mut items = Vec::new();
    let mut pending = Pending::default();
    // Whether an item may start here: after `;`, a block's brace, an attribute or a modifier.
    let mut at_start = true;
    let mut i = 0;
    while i < toks.len() {
        let line = toks[i].1;
        if punct(i, '#') && (punct(i + 1, '[') || (punct(i + 1, '!') && punct(i + 2, '['))) {
            let open = if punct(i + 1, '[') { i + 1 } else { i + 2 };
            let close = pair[open].unwrap_or(toks.len() - 1);
            if open == i + 1 {
                pending.start.get_or_insert(line);
                // Unclosed at the end of the file, it has nothing inside.
                pending.test |= test_attribute(toks.get(open + 1..close).unwrap_or(&[]));
            }
            i = close + 1;
            at_start = true;
            continue;
        }
        let word = ident(i);
        if at_start && word == Some("pub") {
            pending.start.get_or_insert(line);
            if punct(i + 1, '(') {
                i = pair[i + 1].map_or(i + 2, |c| c + 1);
            } else {
                pending.public = true;
                i += 1;
            }
            continue;
        }
        let is_const_modifier = word == Some("const")
            && matches!(ident(i + 1), Some("fn" | "unsafe" | "async" | "extern"));
        if at_start && (word.is_some_and(|w| MODIFIERS.contains(&w)) || is_const_modifier) {
            pending.start.get_or_insert(line);
            i += 1;
            continue;
        }
        if at_start && let Some(found) = item_at(&toks, &pair, i) {
            let (kind, name, name_at, end_at) = found;
            let mut start = pending.start.unwrap_or(line);
            // Its doc comment, above it.
            while start > 1
                && lines
                    .get(start as usize - 2)
                    .is_some_and(|l| l.trim_start().starts_with("///"))
            {
                start -= 1;
            }
            items.push(Item {
                kind,
                path: name.clone(),
                name,
                start,
                line: toks[name_at].1,
                end: end_line(end_at),
                public: pending.public,
                test: pending.test,
            });
            pending = Pending::default();
            // Into the item: what is inside it is read for items too.
            i = name_at + 1;
            at_start = false;
            continue;
        }
        at_start = punct(i, ';') || punct(i, '{') || punct(i, '}');
        pending = Pending::default();
        i += 1;
    }
    nest(&mut items);
    items
}

/// The item whose keyword is at `i`: its kind, name, where its name is and where it ends.
fn item_at(
    toks: &[(Tok, u32)],
    pair: &[Option<usize>],
    i: usize,
) -> Option<(ItemKind, String, usize, Option<usize>)> {
    let ident = |i: usize| match toks.get(i) {
        Some((Tok::Ident(s), _)) => Some(s.as_str()),
        _ => None,
    };
    let punct = |i: usize, c: char| matches!(toks.get(i), Some((Tok::Punct(p), _)) if *p == c);
    let named = |at: usize| ident(at).filter(|n| *n != "_").map(|n| (n.to_owned(), at));
    let (kind, name, name_at) = match ident(i)? {
        "fn" => {
            let (n, at) = named(i + 1)?;
            (ItemKind::Fn, n, at)
        }
        "struct" | "enum" | "trait" | "mod" | "type" | "const" => {
            let kind = match ident(i)? {
                "struct" => ItemKind::Struct,
                "enum" => ItemKind::Enum,
                "trait" => ItemKind::Trait,
                "mod" => ItemKind::Mod,
                "type" => ItemKind::Type,
                _ => ItemKind::Const,
            };
            let (n, at) = named(i + 1)?;
            (kind, n, at)
        }
        "union" if punct(i + 2, '{') || punct(i + 2, '<') => {
            let (n, at) = named(i + 1)?;
            (ItemKind::Union, n, at)
        }
        "static" => {
            let at = if ident(i + 1) == Some("mut") {
                i + 2
            } else {
                i + 1
            };
            let (n, at) = named(at)?;
            (ItemKind::Static, n, at)
        }
        "macro_rules" if punct(i + 1, '!') => {
            let (n, at) = named(i + 2)?;
            (ItemKind::Macro, n, at)
        }
        "impl" => {
            let (n, label) = impl_header(toks, i)?;
            let end = body_end(toks, pair, i + 1, false);
            return Some((ItemKind::Impl, format!("{n}\u{0}{label}"), i, end));
        }
        _ => return None,
    };
    let till_semicolon = matches!(kind, ItemKind::Const | ItemKind::Static | ItemKind::Type);
    let end = if kind == ItemKind::Macro {
        let open = name_at + 1;
        match toks.get(open) {
            Some((Tok::Punct('{'), _)) => pair[open],
            Some((Tok::Punct('(' | '['), _)) => {
                pair[open].map(|c| if punct(c + 1, ';') { c + 1 } else { c })
            }
            _ => Some(name_at),
        }
    } else {
        body_end(toks, pair, name_at + 1, till_semicolon)
    };
    Some((kind, name, name_at, end))
}

/// Where an item whose header goes on from `from` ends: at its body's closing brace, or at
/// the `;` that ends it (for `till_semicolon`, the first one outside brackets of any kind).
fn body_end(
    toks: &[(Tok, u32)],
    pair: &[Option<usize>],
    from: usize,
    till_semicolon: bool,
) -> Option<usize> {
    let mut k = from;
    while k < toks.len() {
        match toks[k].0 {
            Tok::Punct('(' | '[') => k = pair[k]? + 1,
            Tok::Punct('{') if till_semicolon => k = pair[k]? + 1,
            Tok::Punct('{') => return pair[k],
            Tok::Punct(';') => return Some(k),
            // A `}` closing what the item is in: it ended without one.
            Tok::Punct('}') => return Some(k.saturating_sub(1)),
            _ => k += 1,
        }
    }
    None
}

/// The type an `impl` at `i` is for, and how it reads (`impl Display for Repo`).
fn impl_header(toks: &[(Tok, u32)], i: usize) -> Option<(String, String)> {
    let mut k = i + 1;
    // Its generics.
    if matches!(toks.get(k), Some((Tok::Punct('<'), _))) {
        let mut depth = 0;
        while k < toks.len() {
            match toks[k].0 {
                Tok::Punct('<') => depth += 1,
                Tok::Punct('>') if !matches!(toks.get(k - 1), Some((Tok::Punct('-'), _))) => {
                    depth -= 1;
                    if depth == 0 {
                        k += 1;
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
    }
    // The path before its own generics, or `for`, or the body.
    let path_end = |mut k: usize| -> (Option<String>, usize) {
        let mut last = None;
        let mut angle = 0;
        while k < toks.len() {
            match &toks[k].0 {
                Tok::Punct('{' | ';') => break,
                Tok::Ident(w) if angle == 0 && (w == "for" || w == "where") => break,
                Tok::Punct('<') => angle += 1,
                Tok::Punct('>') if !matches!(toks.get(k - 1), Some((Tok::Punct('-'), _))) => {
                    angle -= 1
                }
                Tok::Ident(w) if angle == 0 && !matches!(w.as_str(), "dyn" | "mut" | "const") => {
                    last = Some(w.clone());
                }
                _ => {}
            }
            k += 1;
        }
        (last, k)
    };
    let (first, k) = path_end(k);
    let first = first?;
    if matches!(toks.get(k), Some((Tok::Ident(w), _)) if w == "for") {
        let (ty, _) = path_end(k + 1);
        let ty = ty?;
        let label = format!("impl {first} for {ty}");
        return Some((ty, label));
    }
    Some((first.clone(), format!("impl {first}")))
}

/// Whether an attribute's tokens make what follows a test: `test`, `tokio::test`,
/// `cfg(test)`, `cfg(all(test, …))`, not `cfg(not(test))`.
fn test_attribute(inner: &[(Tok, u32)]) -> bool {
    let words: Vec<&str> = inner
        .iter()
        .filter_map(|(t, _)| match t {
            Tok::Ident(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    match words.first() {
        Some(&"cfg") => words.contains(&"test") && !words.contains(&"not"),
        Some(_) => {
            // The attribute's path, up to its arguments.
            let path_len = inner
                .iter()
                .position(|(t, _)| matches!(t, Tok::Punct('(' | '=')))
                .unwrap_or(inner.len());
            inner[..path_len]
                .iter()
                .rev()
                .find_map(|(t, _)| match t {
                    Tok::Ident(s) => Some(s == "test"),
                    _ => None,
                })
                .unwrap_or(false)
        }
        None => false,
    }
}

/// Gives each item its path within the items around it, and makes everything in a test a
/// test.
fn nest(items: &mut [Item]) {
    // An impl's name came with its label: `Type\0impl Trait for Type`.
    let labels: Vec<Option<String>> = items
        .iter_mut()
        .map(|item| {
            let (name, label) = item.name.split_once('\u{0}')?;
            let (name, label) = (name.to_owned(), label.to_owned());
            item.name = name;
            Some(label)
        })
        .collect();
    let snapshot: Vec<Item> = items.to_vec();
    for (i, item) in items.iter_mut().enumerate() {
        if item.kind == ItemKind::Mod && item.name == "tests" {
            item.test = true;
        }
        // The items it is in, outermost first.
        let mut chain: Vec<&Item> = snapshot
            .iter()
            .enumerate()
            .filter(|&(j, p)| {
                j != i
                    && p.start <= item.start
                    && item.end <= p.end
                    && (p.start, p.end) != (item.start, item.end)
            })
            .map(|(_, p)| p)
            .collect();
        chain.sort_by_key(|p| std::cmp::Reverse(p.end - p.start));
        if chain
            .iter()
            .any(|p| p.test || (p.kind == ItemKind::Mod && p.name == "tests"))
        {
            item.test = true;
        }
        let mut path: Vec<String> = chain
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    ItemKind::Mod | ItemKind::Impl | ItemKind::Trait | ItemKind::Fn
                )
            })
            .map(|p| p.name.clone())
            .collect();
        match &labels[i] {
            Some(label) => {
                path.push(label.clone());
            }
            None => path.push(item.name.clone()),
        }
        item.path = path.join("::");
    }
}
