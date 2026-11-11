//! Normalize tool output and separate commit summaries from descriptions.
/// A tool's answer as text: terminal colours and carriage returns out, and the code fence it
/// may have been wrapped in.
pub fn clean_answer(raw: &str) -> String {
    let plain = strip_escapes(raw).replace('\r', "");
    let mut text = plain.trim();
    // Only one block wrapping it all: a review's Markdown may start and end with blocks of
    // its own.
    if text.starts_with("```")
        && text.ends_with("```")
        && text.len() > 6
        && !text[3..text.len() - 3].contains("```")
    {
        let inner = &text[3..text.len() - 3];
        // The fence's language word, on its line.
        text = inner
            .split_once('\n')
            .map_or(inner, |(_, rest)| rest)
            .trim();
    }
    text.to_owned()
}

/// `s` without terminal escape sequences (colours, cursor moves, titles).
pub(super) fn strip_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: up to a final byte.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A commit message answered by a tool, as the summary and the description of the commit box.
pub fn split_message(answer: &str) -> (String, String) {
    let mut lines = answer
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .peekable();
    // "Commit message:" before the message itself.
    if lines.peek().is_some_and(|first| {
        let first = first.trim().to_lowercase();
        first.ends_with(':') && first.contains("commit message")
    }) {
        lines.next();
        while lines.peek().is_some_and(|l| l.trim().is_empty()) {
            lines.next();
        }
    }
    let summary = lines.next().unwrap_or_default().trim();
    // Quotes or backticks around the whole of it, not one at an end (`fix: handle `None``).
    let summary = ['`', '"']
        .iter()
        .find_map(|&q| summary.strip_prefix(q)?.strip_suffix(q))
        .unwrap_or(summary)
        .trim();
    let body: Vec<&str> = lines.collect();
    let body = body.join("\n");
    (summary.to_owned(), body.trim().to_owned())
}
