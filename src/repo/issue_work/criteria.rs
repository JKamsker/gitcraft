//! What of an issue's description stays in view while working on it: its acceptance criteria
//! when it has a section of them, else its task list, else the whole description; cut to a size
//! that fits a card (and a value in git's config, where it's kept).

/// At most this many characters are kept.
const MAX_CHARS: usize = 4000;
/// At most this many lines.
const MAX_LINES: usize = 60;
/// What the end of a description that was cut says.
const CUT: &str = "…";

/// Which part of the description was kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Kept {
    /// The section headed "Acceptance criteria" (or "Definition of done").
    #[default]
    Criteria,
    /// Its task list, gathered from wherever it is.
    Tasks,
    /// All of it.
    Description,
}

impl Kept {
    /// How it's kept in git's config.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Kept::Criteria => "criteria",
            Kept::Tasks => "tasks",
            Kept::Description => "description",
        }
    }

    /// What [`Kept::key`] gave. A link written before there was one says nothing: the card
    /// calls what it kept the criteria then, as it always did.
    pub(crate) fn from_key(key: &str) -> Kept {
        match key.trim() {
            "tasks" => Kept::Tasks,
            "description" => Kept::Description,
            _ => Kept::Criteria,
        }
    }

    /// "the acceptance criteria", for the card's tooltips.
    pub(crate) fn what(self) -> &'static str {
        match self {
            Kept::Criteria => crate::i18n::text("repo-workflows-issue_work-what-68e64f0f"),
            Kept::Tasks => crate::i18n::text("repo-workflows-issue_work-what-3e69f7c1"),
            Kept::Description => crate::i18n::text("repo-workflows-issue_work-what-734b09bc"),
        }
    }
}

/// The part of `body` (an issue's Markdown) to keep in view, and which part it is: the section
/// headed "Acceptance criteria" (a `#` heading, a line in bold or one ending in `:`), else every
/// task-list item (`- [ ] …`), else all of it; HTML comments (which issue templates are full of)
/// left out. Empty when there is nothing.
pub(crate) fn extract(body: &str) -> (Kept, String) {
    let body = without_comments(&body.replace("\r\n", "\n"));
    let lines: Vec<&str> = body.lines().collect();
    let (kept, picked) = match criteria_section(&lines) {
        Some(section) => (Kept::Criteria, section),
        None => match tasks(&lines) {
            Some(items) => (Kept::Tasks, items),
            None => (Kept::Description, lines.clone()),
        },
    };
    (kept, cap(&trim_blank(&picked).join("\n")))
}

/// Whether a task is ticked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tick {
    Open,
    Done,
    /// GitLab's `- [~]`: it no longer applies.
    Dropped,
}

/// One line of what is kept, as the card shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// A task-list item.
    Task {
        tick: Tick,
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
    if let Some((tick, rest)) = task(text) {
        return Line::Task {
            tick,
            text: rest.to_owned(),
        };
    }
    if let Some(rest) = list_item(text) {
        return Line::Bullet(rest.to_owned());
    }
    Line::Text(text.trim_start_matches('#').trim().to_owned())
}

/// `markdown` (one line of it) as the text it reads as: links as their text, no `**`, `_` or
/// backticks around words. As it was when Markdown makes nothing of it (`---`).
pub(crate) fn plain(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser};
    let mut out = String::new();
    for event in Parser::new_ext(markdown, crate::pull_request::markdown_options()) {
        match event {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    if out.trim().is_empty() {
        markdown.to_owned()
    } else {
        out
    }
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

/// Whether `text` is a task-list item, and if so how it's ticked and what it says.
fn task(text: &str) -> Option<(Tick, &str)> {
    let rest = list_item(text.trim_start())?;
    let tick = match rest.get(..3)? {
        "[ ]" => Tick::Open,
        "[x]" | "[X]" => Tick::Done,
        "[~]" => Tick::Dropped,
        _ => return None,
    };
    Some((tick, rest[3..].trim()))
}

/// The rank of a line in bold: below every `#` heading.
const BOLD: usize = 7;
/// The rank of a line ending in `:`, below all: one can start the section ("Acceptance
/// criteria:"), but none ends one ("When offline:" is a line of it).
const COLON: usize = 8;

/// The lines under the "Acceptance criteria" heading, up to the next heading of its kind (one
/// of the same level or higher, for a `#` heading; a `#` heading or a line in bold, for the
/// others). `None` when there is no such section, or nothing in it.
fn criteria_section<'a>(lines: &[&'a str]) -> Option<Vec<&'a str>> {
    let headings = headings(lines);
    let (start, level) = headings
        .iter()
        .find(|(i, _)| is_criteria_title(&heading_text(lines[*i])))
        .copied()?;
    let ends_at = level.min(BOLD);
    let end = headings
        .iter()
        .find(|(i, l)| *i > start && *l <= ends_at)
        .map_or(lines.len(), |(i, _)| *i);
    let section = trim_blank(&lines[start + 1..end]);
    (!section.is_empty()).then_some(section)
}

/// Whether each of `lines` is in fenced code (its fences too).
fn fenced(lines: &[&str]) -> Vec<bool> {
    let mut in_fence = false;
    lines
        .iter()
        .map(|raw| {
            let text = raw.trim();
            if text.starts_with("```") || text.starts_with("~~~") {
                in_fence = !in_fence;
                return true;
            }
            in_fence
        })
        .collect()
}

/// The headings among `lines`, outside fenced code: (line, level). A `#` heading has its
/// number of `#`s; a line all in bold, or one ending in `:` that isn't a list item, ranks below
/// them all ([`BOLD`], [`COLON`]).
fn headings(lines: &[&str]) -> Vec<(usize, usize)> {
    let code = fenced(lines);
    let mut found = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        if code[i] {
            continue;
        }
        let text = raw.trim();
        if let Some(level) = hash_level(text) {
            found.push((i, level));
        } else if is_bold_line(text) {
            found.push((i, BOLD));
        } else if text.ends_with(':') && list_item(text).is_none() {
            found.push((i, COLON));
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

/// Wherever it says so in the heading: `3. Acceptance criteria`, `✅ Definition of Done`.
fn is_criteria_title(text: &str) -> bool {
    ["acceptance criteri", "definition of done"]
        .iter()
        .any(|title| text.contains(title))
}

/// Every task-list item outside fenced code, or `None` without one.
fn tasks<'a>(lines: &[&'a str]) -> Option<Vec<&'a str>> {
    let code = fenced(lines);
    let items: Vec<&str> = lines
        .iter()
        .zip(code)
        .filter(|(l, code)| !code && task(l).is_some())
        .map(|(l, _)| *l)
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
pub(super) fn cap(text: &str) -> String {
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

    fn text(body: &str) -> String {
        extract(body).1
    }

    #[test]
    fn nothing_gives_nothing() {
        assert_eq!(text(""), "");
        assert_eq!(text("  \n\r\n \n"), "");
        assert_eq!(text("<!-- Describe the bug -->\n"), "");
        assert!(lines("").is_empty());
    }

    #[test]
    fn the_acceptance_criteria_section() {
        let body = "## Problem\nLogin crashes.\n\n## Acceptance criteria\n\n- [ ] no crash\n\
                    - [x] a test\n\n## Notes\n- [ ] not this one\n";
        assert_eq!(
            extract(body),
            (Kept::Criteria, "- [ ] no crash\n- [x] a test".to_owned())
        );
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_its_level() {
        let body = "# Story\n## Acceptance Criteria:\nIt works.\n### Edge cases\n* empty\n\
                    ## Design\nlater";
        assert_eq!(text(body), "It works.\n### Edge cases\n* empty");
    }

    #[test]
    fn the_heading_can_say_more() {
        let numbered = "## 2. Steps\nx\n## 3. Acceptance criteria\n- [ ] works\n## 4. Notes\ny";
        assert_eq!(text(numbered), "- [ ] works");
        let done = "### ✅ Definition of Done\n- tested\n### Other\nz";
        assert_eq!(text(done), "- tested");
    }

    #[test]
    fn a_bold_or_colon_heading_counts_too() {
        let bold = "Intro\n\n**Acceptance criteria**\n1. one\n2. two\n\n**Out of scope**\nthree";
        assert_eq!(text(bold), "1. one\n2. two");
        let colon = "Acceptance criteria:\r\n- works\r\n**Other**\r\n- no";
        assert_eq!(text(colon), "- works");
    }

    #[test]
    fn a_line_ending_in_a_colon_is_part_of_the_section() {
        let bold = "**Acceptance criteria**\nWhen logged in:\n- [ ] no crash\nWhen offline:\n\
                    - [ ] a message\n## Notes\nlater";
        assert_eq!(
            text(bold),
            "When logged in:\n- [ ] no crash\nWhen offline:\n- [ ] a message"
        );
        let hash = "## Acceptance criteria\nGiven a user:\n- works\n## Next\nx";
        assert_eq!(text(hash), "Given a user:\n- works");
    }

    #[test]
    fn a_heading_in_code_is_no_heading() {
        let body = "## Acceptance criteria\n```\n# not a heading\n```\n- [ ] done\n## Next\nx";
        assert_eq!(text(body), "```\n# not a heading\n```\n- [ ] done");
    }

    #[test]
    fn an_empty_section_falls_back_to_the_tasks() {
        let body = "## Acceptance criteria\n\n## Steps\n- [ ] a\n  - [X] b\ntext\n+ [ ] c";
        assert_eq!(
            extract(body),
            (Kept::Tasks, "- [ ] a\n  - [X] b\n+ [ ] c".to_owned())
        );
    }

    #[test]
    fn tasks_in_code_are_no_tasks() {
        let body = "Steps:\n```md\n- [ ] an example\n```\n- [ ] real\n- [~] dropped";
        assert_eq!(text(body), "- [ ] real\n- [~] dropped");
        let only_code = "```\n- [ ] an example\n```";
        assert_eq!(extract(only_code).0, Kept::Description);
    }

    #[test]
    fn without_criteria_or_tasks_the_whole_body() {
        let body = "\n\nIt crashes when I log in.\n<!-- template hint -->\nSee logs.\n\n";
        assert_eq!(
            extract(body),
            (
                Kept::Description,
                "It crashes when I log in.\n\nSee logs.".to_owned()
            )
        );
    }

    #[test]
    fn what_was_kept_is_kept_by_its_key() {
        for kept in [Kept::Criteria, Kept::Tasks, Kept::Description] {
            assert_eq!(Kept::from_key(kept.key()), kept);
        }
        assert_eq!(Kept::from_key(""), Kept::Criteria);
    }

    #[test]
    fn a_huge_body_is_cut() {
        let body = "word ".repeat(100_000);
        let kept = text(&body);
        assert!(kept.ends_with(CUT));
        assert!(kept.chars().count() <= MAX_CHARS + 2);
        let many = "- [ ] item\n".repeat(1000);
        let kept = text(&many);
        assert_eq!(kept.lines().count(), MAX_LINES + 1);
        // On a character's boundary, whatever the script.
        let wide = text(&"日本語".repeat(5000));
        assert_eq!(wide.chars().count(), MAX_CHARS + 2);
    }

    #[test]
    fn lines_as_the_card_shows_them() {
        let text = "- [ ] no crash\n* [x] a test\n- [~] not any more\n\n## Edge\n- empty\n\
                    3) third\nplain";
        let task = |tick, text: &str| Line::Task {
            tick,
            text: text.into(),
        };
        assert_eq!(
            lines(text),
            [
                task(Tick::Open, "no crash"),
                task(Tick::Done, "a test"),
                task(Tick::Dropped, "not any more"),
                Line::Blank,
                Line::Text("Edge".into()),
                Line::Bullet("empty".into()),
                Line::Bullet("third".into()),
                Line::Text("plain".into()),
            ]
        );
        assert_eq!(lines("-not a list"), [Line::Text("-not a list".into())]);
    }

    #[test]
    fn markdown_reads_as_its_text() {
        assert_eq!(
            plain("**No** crash in [login](https://x/y) with `--force` and _emphasis_"),
            "No crash in login with --force and emphasis"
        );
        assert_eq!(plain("plain"), "plain");
        assert_eq!(plain("---"), "---");
    }
}
