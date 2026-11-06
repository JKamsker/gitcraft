//! What of an issue's description stays in view while working on it: its acceptance criteria
//! when it has a section of them, else its task list, else the whole description; cut to a size
//! that fits a card (and a value in git's config, where it's kept).

/// At most this many characters are kept.
const MAX_CHARS: usize = 4000;
/// At most this many lines.
const MAX_LINES: usize = 60;
/// What the end of a description that was cut says.
const CUT: &str = "…";

/// The part of `body` (an issue's Markdown) to keep in view: the section headed "Acceptance
/// criteria" (a `#` heading, a line in bold or one ending in `:`), else every task-list item
/// (`- [ ] …`), else all of it; HTML comments (which issue templates are full of) left out.
/// Empty when there is nothing.
pub(crate) fn extract(body: &str) -> String {
    let body = without_comments(&body.replace("\r\n", "\n"));
    let lines: Vec<&str> = body.lines().collect();
    let picked = criteria_section(&lines)
        .or_else(|| tasks(&lines))
        .unwrap_or_else(|| lines.clone());
    cap(&trim_blank(&picked).join("\n"))
}

/// One line of what is kept, as the card shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// A task-list item, ticked or not.
    Task {
        done: bool,
        text: String,
    },
    /// An item of a plain list.
    Bullet(String),
    Text(String),
    Blank,
}

/// `text` (what [`extract`] kept) line by line, list markers and heading marks taken off.
pub(crate) fn lines(text: &str) -> Vec<Line> {
    text.lines().map(line).collect()
}

fn line(raw: &str) -> Line {
    let text = raw.trim();
    if text.is_empty() {
        return Line::Blank;
    }
    if let Some((done, rest)) = task(text) {
        return Line::Task {
            done,
            text: rest.to_owned(),
        };
    }
    if let Some(rest) = list_item(text) {
        return Line::Bullet(rest.to_owned());
    }
    Line::Text(text.trim_start_matches('#').trim().to_owned())
}

/// What follows a list marker (`-`, `*`, `+`, `1.`, `1)`), if `text` starts with one.
fn list_item(text: &str) -> Option<&str> {
    if let Some(rest) = text.strip_prefix(['-', '*', '+'])
        && rest.starts_with(' ')
    {
        return Some(rest.trim_start());
    }
    let digits = text.find(|c: char| !c.is_ascii_digit())?;
    let rest = text[digits..].strip_prefix(['.', ')'])?;
    (digits > 0 && rest.starts_with(' ')).then(|| rest.trim_start())
}

/// Whether `text` is a task-list item, and if so whether it's ticked and what it says.
fn task(text: &str) -> Option<(bool, &str)> {
    let rest = list_item(text.trim_start())?;
    let done = if rest.starts_with("[ ]") {
        false
    } else if rest.starts_with("[x]") || rest.starts_with("[X]") {
        true
    } else {
        return None;
    };
    Some((done, rest[3..].trim()))
}

/// The lines under the "Acceptance criteria" heading, up to the next heading of its kind (one
/// of the same level or higher, for a `#` heading). `None` when there is no such section, or
/// nothing in it.
fn criteria_section<'a>(lines: &[&'a str]) -> Option<Vec<&'a str>> {
    let headings = headings(lines);
    let (start, level) = headings
        .iter()
        .find(|(i, _)| is_criteria_title(&heading_text(lines[*i])))
        .copied()?;
    let end = headings
        .iter()
        .find(|(i, l)| *i > start && *l <= level)
        .map_or(lines.len(), |(i, _)| *i);
    let section = trim_blank(&lines[start + 1..end]);
    (!section.is_empty()).then_some(section)
}

/// The headings among `lines`, outside fenced code: (line, level). A `#` heading has its
/// number of `#`s; a line all in bold, or one ending in `:` that isn't a list item, ranks below
/// them all.
fn headings(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut in_fence = false;
    for (i, raw) in lines.iter().enumerate() {
        let text = raw.trim();
        if text.starts_with("```") || text.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(level) = hash_level(text) {
            found.push((i, level));
        } else if is_bold_line(text) || (text.ends_with(':') && list_item(text).is_none()) {
            found.push((i, 7));
        }
    }
    found
}

/// `## x` → 2: the level of a `#` heading.
fn hash_level(text: &str) -> Option<usize> {
    let level = text.chars().take_while(|c| *c == '#').count();
    let rest = &text[level..];
    ((1..=6).contains(&level) && (rest.is_empty() || rest.starts_with(' '))).then_some(level)
}

fn is_bold_line(text: &str) -> bool {
    let inner = text
        .strip_prefix("**")
        .and_then(|t| t.strip_suffix("**"))
        .or_else(|| text.strip_prefix("__").and_then(|t| t.strip_suffix("__")));
    inner.is_some_and(|t| !t.trim().is_empty())
}

/// A heading's words: `## Acceptance Criteria:` → `acceptance criteria`.
fn heading_text(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('#')
        .trim_matches(|c: char| c == '*' || c == '_' || c == ':' || c.is_whitespace())
        .to_lowercase()
}

fn is_criteria_title(text: &str) -> bool {
    [
        "acceptance criteria",
        "acceptance criterion",
        "definition of done",
    ]
    .iter()
    .any(|title| text.starts_with(title))
}

/// Every task-list item, or `None` without one.
fn tasks<'a>(lines: &[&'a str]) -> Option<Vec<&'a str>> {
    let items: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| task(l).is_some())
        .collect();
    (!items.is_empty()).then_some(items)
}

/// `lines` without the blank ones at either end.
fn trim_blank<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let blank = |l: &&str| l.trim().is_empty();
    let start = lines.iter().position(|l| !blank(l)).unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !blank(l))
        .map_or(start, |i| i + 1);
    lines[start..end].to_vec()
}

/// `text` without its `<!-- … -->` comments (an unclosed one runs to the end).
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find("-->") {
            Some(end) => &rest[start + end + 3..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// `text` cut to [`MAX_LINES`] lines and [`MAX_CHARS`] characters (on a character's
/// boundary), marked as cut when it was.
fn cap(text: &str) -> String {
    let mut cut = false;
    let mut kept: String = text
        .lines()
        .enumerate()
        .take_while(|(i, _)| {
            cut |= *i >= MAX_LINES;
            *i < MAX_LINES
        })
        .map(|(_, l)| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    if kept.chars().count() > MAX_CHARS {
        kept = kept.chars().take(MAX_CHARS).collect();
        cut = true;
    }
    if cut {
        kept = format!("{}\n{CUT}", kept.trim_end());
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_gives_nothing() {
        assert_eq!(extract(""), "");
        assert_eq!(extract("  \n\r\n \n"), "");
        assert_eq!(extract("<!-- Describe the bug -->\n"), "");
        assert!(lines("").is_empty());
    }

    #[test]
    fn the_acceptance_criteria_section() {
        let body = "## Problem\nLogin crashes.\n\n## Acceptance criteria\n\n- [ ] no crash\n\
                    - [x] a test\n\n## Notes\n- [ ] not this one\n";
        assert_eq!(extract(body), "- [ ] no crash\n- [x] a test");
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_its_level() {
        let body = "# Story\n## Acceptance Criteria:\nIt works.\n### Edge cases\n* empty\n\
                    ## Design\nlater";
        assert_eq!(extract(body), "It works.\n### Edge cases\n* empty");
    }

    #[test]
    fn a_bold_or_colon_heading_counts_too() {
        let bold = "Intro\n\n**Acceptance criteria**\n1. one\n2. two\n\n**Out of scope**\nthree";
        assert_eq!(extract(bold), "1. one\n2. two");
        let colon = "Acceptance criteria:\r\n- works\r\nOther:\r\n- no";
        assert_eq!(extract(colon), "- works");
    }

    #[test]
    fn a_heading_in_code_is_no_heading() {
        let body = "## Acceptance criteria\n```\n# not a heading\n```\n- [ ] done\n## Next\nx";
        assert_eq!(extract(body), "```\n# not a heading\n```\n- [ ] done");
    }

    #[test]
    fn an_empty_section_falls_back_to_the_tasks() {
        let body = "## Acceptance criteria\n\n## Steps\n- [ ] a\n  - [X] b\ntext\n+ [ ] c";
        assert_eq!(extract(body), "- [ ] a\n  - [X] b\n+ [ ] c");
    }

    #[test]
    fn without_criteria_or_tasks_the_whole_body() {
        let body = "\n\nIt crashes when I log in.\n<!-- template hint -->\nSee logs.\n\n";
        assert_eq!(extract(body), "It crashes when I log in.\n\nSee logs.");
    }

    #[test]
    fn a_huge_body_is_cut() {
        let body = "word ".repeat(100_000);
        let kept = extract(&body);
        assert!(kept.ends_with(CUT));
        assert!(kept.chars().count() <= MAX_CHARS + 2);
        let many = "- [ ] item\n".repeat(1000);
        let kept = extract(&many);
        assert_eq!(kept.lines().count(), MAX_LINES + 1);
        // On a character's boundary, whatever the script.
        let wide = extract(&"日本語".repeat(5000));
        assert_eq!(wide.chars().count(), MAX_CHARS + 2);
    }

    #[test]
    fn lines_as_the_card_shows_them() {
        let text = "- [ ] no crash\n* [x] a test\n\n## Edge\n- empty\n3) third\nplain";
        assert_eq!(
            lines(text),
            [
                Line::Task {
                    done: false,
                    text: "no crash".into()
                },
                Line::Task {
                    done: true,
                    text: "a test".into()
                },
                Line::Blank,
                Line::Text("Edge".into()),
                Line::Bullet("empty".into()),
                Line::Bullet("third".into()),
                Line::Text("plain".into()),
            ]
        );
        assert_eq!(lines("-not a list"), [Line::Text("-not a list".into())]);
    }
}
