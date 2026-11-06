//! Cutting a query into its pieces: free text, quoted phrases and `operator:value`.

use std::ops::Range;

use super::Operator;

/// What is wrong with how a piece is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenProblem {
    /// A quote opened and never closed.
    Unclosed,
    /// Text right after a closing quote (`"a b"c`).
    AfterQuote,
}

/// One piece of a query.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    /// Where it is in the query, in bytes, quotes included.
    pub span: Range<usize>,
    /// The operator, when it is one.
    pub op: Option<Operator>,
    /// Written with a leading `-` (`-merges:`).
    pub negated: bool,
    /// Its value (or the free text), quotes taken off and `\"` / `\\` read as `"` / `\`.
    pub value: String,
    /// Where the value starts, in bytes: after the colon, or at the start of free text.
    pub value_start: usize,
    pub problem: Option<TokenProblem>,
}

/// The pieces of `query`, in order. Whitespace separates them, except inside quotes.
pub fn tokens(query: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = next_non_space(query, at) {
        let token = read_token(query, start);
        at = token.span.end;
        out.push(token);
    }
    out
}

/// The piece the cursor is in or right after (`cursor` counted in characters, as text fields
/// count it), if any.
pub fn token_at(query: &str, cursor: usize) -> Option<Token> {
    let byte = query
        .char_indices()
        .nth(cursor)
        .map_or(query.len(), |(i, _)| i);
    tokens(query)
        .into_iter()
        .find(|t| t.span.start <= byte && byte <= t.span.end)
}

/// `value` as it is written after an operator: as is, or quoted (with `"` and `\` escaped)
/// when it is empty, has whitespace or a quote in it.
pub fn quote_value(value: &str) -> String {
    let plain = !value.is_empty()
        && !value.starts_with('"')
        && !value.chars().any(|c| c.is_whitespace() || c == '"');
    if plain {
        value.to_owned()
    } else {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    }
}

fn next_non_space(query: &str, from: usize) -> Option<usize> {
    query[from..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| from + i)
}

fn word_end(query: &str, from: usize) -> usize {
    query[from..]
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(query.len(), |(i, _)| from + i)
}

fn read_token(query: &str, start: usize) -> Token {
    if query[start..].starts_with('"') {
        let (value, end, problem) = read_quoted(query, start);
        return Token {
            span: start..end,
            op: None,
            negated: false,
            value,
            value_start: start,
            problem,
        };
    }
    let end = word_end(query, start);
    let word = &query[start..end];
    let Some((op, negated, colon)) = operator_of(word) else {
        return Token {
            span: start..end,
            op: None,
            negated: false,
            value: word.to_owned(),
            value_start: start,
            problem: None,
        };
    };
    let value_start = start + colon + 1;
    let (value, end, problem) = if query[value_start..].starts_with('"') {
        read_quoted(query, value_start)
    } else {
        (query[value_start..end].to_owned(), end, None)
    };
    Token {
        span: start..end,
        op: Some(op),
        negated,
        value,
        value_start,
        problem,
    }
}

/// The operator `word` starts with, whether it is negated, and where its colon is.
fn operator_of(word: &str) -> Option<(Operator, bool, usize)> {
    let colon = word.find(':')?;
    let name = &word[..colon];
    let (negated, name) = match name.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, name),
    };
    Operator::from_name(name).map(|op| (op, negated, colon))
}

/// The quoted text starting at the quote at `start`: its value, where it ends, and what is
/// wrong with it. Text glued after the closing quote belongs to the piece (and is a problem).
fn read_quoted(query: &str, start: usize) -> (String, usize, Option<TokenProblem>) {
    let mut value = String::new();
    let mut chars = query[start + 1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                let after = start + 1 + i + 1;
                let end = word_end(query, after);
                let problem = (end > after).then_some(TokenProblem::AfterQuote);
                return (value, end, problem);
            }
            '\\' => match chars.clone().next() {
                Some((_, next @ ('"' | '\\'))) => {
                    chars.next();
                    value.push(next);
                }
                _ => value.push('\\'),
            },
            c => value.push(c),
        }
    }
    (value, query.len(), Some(TokenProblem::Unclosed))
}
