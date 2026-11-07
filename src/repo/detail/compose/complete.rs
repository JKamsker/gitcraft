//! Completion while typing a commit message: the names of the changed files anywhere, and at
//! the start of the summary the types and scopes used in earlier commits. Tab (or a click)
//! takes the highlighted one, ↑ / ↓ move, Esc closes the list.

use egui::text::{CCursor, CCursorRange};
use egui::{Key, Modifiers, Response, RichText, TextEdit, Ui};

use super::prefix::TYPES;
use crate::theme::Tokens;

/// The most suggestions listed at once.
const MAX_SHOWN: usize = 6;
/// How many letters of a file name are typed before names are offered.
const MIN_FOR_FILES: usize = 2;

/// What can be completed: the changed files' paths, and the types and scopes used before (the
/// most used first).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Candidates {
    pub files: Vec<String>,
    pub kinds: Vec<String>,
    pub scopes: Vec<String>,
    /// Types and scopes are offered (the picker is on Conventional Commits).
    pub types: bool,
}

/// Which message field is typed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Summary,
    Body,
}

/// The suggestions shown for the word ending at the cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suggestions {
    /// Where the word starts and the cursor is, in characters.
    pub start: usize,
    pub end: usize,
    pub items: Vec<String>,
    pub selected: usize,
}

/// Whether `c` is part of a word that is completed: a path or a type.
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')
}

/// What to offer for `text` with the cursor at character `cursor` in `field`.
pub fn suggest(text: &str, cursor: usize, field: Field, cands: &Candidates) -> Suggestions {
    let chars: Vec<char> = text.chars().collect();
    let end = cursor.min(chars.len());
    let mut start = end;
    while start > 0 && word_char(chars[start - 1]) {
        start -= 1;
    }
    let word: String = chars[start..end].iter().collect();
    let none = Suggestions {
        start,
        end,
        ..Suggestions::default()
    };
    // Only at the end of a word, not in its middle.
    if word.is_empty() || chars.get(end).is_some_and(|c| word_char(*c)) {
        return none;
    }
    let before: String = chars[..start].iter().collect();
    let summary = field == Field::Summary && cands.types;
    let items = if summary && before.is_empty() {
        let kinds = cands.kinds.iter().map(String::as_str);
        let builtin = TYPES.iter().map(|(k, _)| *k);
        matching(&word, kinds.chain(builtin))
    } else if summary && in_scope(&before) {
        matching(&word, cands.scopes.iter().map(String::as_str))
    } else if word.chars().count() >= MIN_FOR_FILES {
        files_matching(&word, &cands.files)
    } else {
        Vec::new()
    };
    Suggestions { items, ..none }
}

/// Whether the text before a word ends inside a summary's `type(` …: its scope.
fn in_scope(before: &str) -> bool {
    let Some(open) = before.rfind('(') else {
        return false;
    };
    let kind = &before[..open];
    let scope_so_far = &before[open + 1..];
    !kind.is_empty()
        && kind
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        && !scope_so_far.contains(')')
        && scope_so_far
            .chars()
            .all(|c| c == ',' || c == ' ' || word_char(c))
}

/// The words of `from` that start with `word` (in any case) and aren't it, each once.
fn matching<'a>(word: &str, from: impl Iterator<Item = &'a str>) -> Vec<String> {
    let lower = word.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for w in from {
        // What is typed in full (in any case) isn't offered: Tab then moves on as usual.
        if !w.eq_ignore_ascii_case(word)
            && w.to_lowercase().starts_with(&lower)
            && !out.iter().any(|o| o == w)
        {
            out.push(w.to_owned());
            if out.len() >= MAX_SHOWN {
                break;
            }
        }
    }
    out
}

/// The changed files whose path or name starts with `word`: the name when it is what was
/// typed, else the path.
fn files_matching(word: &str, files: &[String]) -> Vec<String> {
    let lower = word.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for path in files {
        let name = path.rsplit('/').next().unwrap_or(path);
        let pick = if name.to_lowercase().starts_with(&lower) && !word.contains('/') {
            name
        } else if path.to_lowercase().starts_with(&lower) {
            path.as_str()
        } else {
            continue;
        };
        if pick.to_lowercase() != lower && !out.iter().any(|o| o == pick) {
            out.push(pick.to_owned());
            if out.len() >= MAX_SHOWN {
                break;
            }
        }
    }
    out
}

/// `text` with characters `start..end` replaced by `with`; and where the cursor goes after it.
pub fn replace(text: &str, start: usize, end: usize, with: &str) -> (String, usize) {
    let byte = |i: usize| text.char_indices().nth(i).map_or(text.len(), |(b, _)| b);
    let (a, b) = (byte(start), byte(end.max(start)));
    let out = format!("{}{with}{}", &text[..a], &text[b..]);
    (out, start + with.chars().count())
}

/// The keys the list takes from the field while it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Keys {
    accept: bool,
    up: bool,
    down: bool,
    close: bool,
}

fn take_keys(ui: &Ui) -> Keys {
    ui.input_mut(|i| Keys {
        accept: i.consume_key(Modifiers::NONE, Key::Tab),
        up: i.consume_key(Modifiers::NONE, Key::ArrowUp),
        down: i.consume_key(Modifiers::NONE, Key::ArrowDown),
        close: i.consume_key(Modifiers::NONE, Key::Escape),
    })
}

/// A message field for `text` (`make` builds it) that offers completions from `cands`, the
/// list kept in `open` between frames. Returns the field's response.
pub fn field(
    ui: &mut Ui,
    text: &mut String,
    make: impl for<'a> FnOnce(&'a mut String) -> TextEdit<'a>,
    field: Field,
    cands: &Candidates,
    open: &mut Option<(Field, Suggestions)>,
) -> Response {
    let id = ui.make_persistent_id(("commit-message-field", field));
    let focused = ui.memory(|m| m.has_focus(id));
    let mine = |open: &Option<(Field, Suggestions)>| {
        open.as_ref()
            .is_some_and(|(f, s)| *f == field && !s.items.is_empty())
    };
    let keys = if focused && mine(open) {
        take_keys(ui)
    } else {
        Keys::default()
    };
    let mut out = make(text).id(id).show(ui);
    let response = out.response.response.clone();
    let mut accepted: Option<String> = None;
    if let Some((f, s)) = open.as_mut()
        && *f == field
        && !s.items.is_empty()
    {
        let n = s.items.len();
        if keys.down {
            s.selected = (s.selected + 1) % n;
        }
        if keys.up {
            s.selected = (s.selected + n - 1) % n;
        }
        if keys.accept {
            accepted = s.items.get(s.selected).cloned();
        }
    }
    if keys.close {
        *open = None;
    }
    // A click on the list (which takes the focus from the field for a moment).
    if let Some((f, s)) = open.as_ref()
        && *f == field
        && let Some(clicked) = list(ui, &response, s)
    {
        accepted = Some(clicked);
    }
    if let Some(with) = accepted
        && let Some((_, s)) = open.take()
    {
        let (new, at) = replace(text, s.start, s.end, &with);
        *text = new;
        out.state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(at))));
        out.state.store(ui.ctx(), id);
        ui.memory_mut(|m| m.request_focus(id));
        ui.ctx().request_repaint();
        return response;
    }
    if response.changed() {
        *open = out.cursor_range.and_then(|r| {
            let s = suggest(text, r.primary.index.0, field, cands);
            (!s.items.is_empty()).then_some((field, s))
        });
    } else if let Some((f, s)) = open.as_ref()
        && *f == field
    {
        let moved = out.cursor_range.map(|r| r.primary.index.0) != Some(s.end);
        let let_go = !response.has_focus() && !pointer_over_list(ui, &response);
        // The cursor went elsewhere, or the field was let go of.
        if (moved && response.has_focus()) || let_go {
            *open = None;
        }
    }
    response
}

/// The list under the field; the suggestion clicked, if one was.
fn list(ui: &Ui, field: &Response, s: &Suggestions) -> Option<String> {
    if s.items.is_empty() {
        return None;
    }
    let t = Tokens::get(ui.ctx());
    let mut clicked = None;
    let shown = egui::Area::new(field.id.with("completions"))
        .order(egui::Order::Foreground)
        .fixed_pos(field.rect.left_bottom())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(field.rect.width().min(320.0));
                for (i, item) in s.items.iter().enumerate() {
                    let r = ui.selectable_label(i == s.selected, RichText::new(item).monospace());
                    if r.clicked() {
                        clicked = Some(item.clone());
                    }
                }
                ui.label(
                    RichText::new("↑ ↓ to choose, Tab to complete, Esc to close")
                        .color(t.text_faint)
                        .small(),
                );
            });
        });
    let rect = shown.response.rect;
    ui.ctx()
        .data_mut(|d| d.insert_temp(field.id.with("completions-rect"), rect));
    clicked
}

/// Whether the pointer is over the list of `field` (a click there takes the field's focus
/// before the list sees it).
fn pointer_over_list(ui: &Ui, field: &Response) -> bool {
    let rect: Option<egui::Rect> = ui
        .ctx()
        .data(|d| d.get_temp(field.id.with("completions-rect")));
    match (rect, ui.ctx().pointer_latest_pos()) {
        (Some(rect), Some(pos)) => rect.contains(pos),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cands() -> Candidates {
        Candidates {
            files: vec![
                "src/repo/detail/commit_box.rs".into(),
                "docs/Read me ü.md".into(),
                "src/main.rs".into(),
                "README.md".into(),
            ],
            kinds: vec!["feat".into(), "fix".into(), "fixup".into()],
            scopes: vec!["ui".into(), "git".into(), "graph".into()],
            types: true,
        }
    }

    fn items(text: &str, field: Field) -> Vec<String> {
        suggest(text, text.chars().count(), field, &cands()).items
    }

    #[test]
    fn types_at_the_start_of_the_summary() {
        assert_eq!(items("fi", Field::Summary), ["fix", "fixup"]);
        assert_eq!(items("d", Field::Summary), ["docs"]);
        // Not in the description, nor after the first word.
        assert!(items("fi", Field::Body).is_empty());
        assert!(items("x fi", Field::Summary).is_empty());
        // What is typed in full isn't offered again, in any case.
        assert_eq!(items("fix", Field::Summary), ["fixup"]);
        assert_eq!(items("Docs", Field::Summary), Vec::<String>::new());
        // Not with the picker on Gitmoji or off.
        let no_types = Candidates {
            types: false,
            ..cands()
        };
        assert!(suggest("fi", 2, Field::Summary, &no_types).items.is_empty());
        assert!(
            suggest("feat(g", 6, Field::Summary, &no_types)
                .items
                .is_empty()
        );
    }

    #[test]
    fn scopes_inside_the_parentheses() {
        assert_eq!(items("feat(g", Field::Summary), ["git", "graph"]);
        assert_eq!(items("feat(ui, g", Field::Summary), ["git", "graph"]);
        assert!(items("feat(ui): g", Field::Summary).is_empty());
        assert!(items("(g", Field::Summary).is_empty());
    }

    #[test]
    fn changed_files_anywhere() {
        assert_eq!(items("Fix commit_", Field::Summary), ["commit_box.rs"]);
        assert_eq!(
            items("see src/", Field::Body),
            ["src/repo/detail/commit_box.rs", "src/main.rs"]
        );
        // In any case.
        assert_eq!(items("in REA", Field::Body), ["Read me ü.md", "README.md"]);
        // Names with spaces and letters outside ASCII complete up to their first space.
        assert_eq!(items("docs/Re", Field::Body), ["docs/Read me ü.md"]);
        // One letter is too few for files.
        assert!(items("x m", Field::Body).is_empty());
        assert!(items("", Field::Body).is_empty());
    }

    #[test]
    fn only_at_the_end_of_a_word() {
        let text = "feat: commit_box";
        let s = suggest(text, 10, Field::Summary, &cands());
        assert!(s.items.is_empty());
        // The cursor past the end is the end.
        assert_eq!(
            suggest("fi", 99, Field::Summary, &cands()).items,
            ["fix", "fixup"]
        );
    }

    #[test]
    fn replaces_the_word_by_characters() {
        assert_eq!(
            replace("käse fi", 5, 7, "fixup"),
            ("käse fixup".to_owned(), 10)
        );
        assert_eq!(replace("ü", 1, 1, "x"), ("üx".to_owned(), 2));
        assert_eq!(replace("", 0, 0, "feat"), ("feat".to_owned(), 4));
        assert_eq!(replace("ab cd", 3, 99, "x"), ("ab x".to_owned(), 4));
    }

    #[test]
    fn a_huge_message_is_fine() {
        let text = format!("{}commit_", "word ".repeat(50_000));
        assert_eq!(items(&text, Field::Body), ["commit_box.rs"]);
    }
}
