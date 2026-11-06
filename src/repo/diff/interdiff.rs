//! How the patch of a commit changed between two versions of a series (the diff of the two
//! diffs range-diff prints), drawn the way a diff is: removed and added lines on their colours,
//! the changed part of a line paired with its counterpart marked.

use egui::{Sense, Ui, vec2};

use super::paint::paint_line;
use super::parse::pair_lines;
use super::select::Side;
use super::{DiffData, DiffLine, Kind, LINE_H};
use crate::repo::highlight::{MAX_LINE_CHARS, cap_line};
use crate::theme::{self, Tokens};

/// The lines of a diff of patches (each starting with its outer mark: ` `, `-`, `+` or `@`,
/// then the patch's own line) made ready to draw.
pub(in crate::repo) fn interdiff_data(body: &[String]) -> DiffData {
    let mut lines: Vec<DiffLine> = body.iter().map(|raw| interdiff_line(raw)).collect();
    let (mut changed, split) = pair_lines(&lines);
    let mut uncut = std::collections::HashMap::new();
    for (i, line) in lines.iter_mut().enumerate() {
        if let Some(whole) = cap_line(&mut line.text, None) {
            uncut.insert(i, whole);
        }
    }
    for range in changed.iter_mut().flatten() {
        *range = (range.0.min(MAX_LINE_CHARS), range.1.min(MAX_LINE_CHARS));
    }
    let longest = lines
        .iter()
        .map(|l| l.text.chars().count())
        .max()
        .unwrap_or(0);
    DiffData {
        spans: vec![None; lines.len()],
        lines,
        changed,
        split,
        image: None,
        lfs: None,
        header: String::new(),
        partial: false,
        new_file: false,
        longest,
        uncut,
        map: Default::default(),
    }
}

/// One line: the outer mark says whether the patch lost it, gained it or kept it; `@` marks
/// where a part of the patch starts, as does range-diff's `## file ##` heading of a file.
fn interdiff_line(raw: &str) -> DiffLine {
    let mut chars = raw.chars();
    let mark = chars.next().unwrap_or(' ');
    let rest = chars.as_str();
    let kind = match mark {
        '+' => Kind::Add,
        '-' => Kind::Del,
        '@' => Kind::Hunk,
        _ if rest.trim_start().starts_with("## ") => Kind::Hunk,
        _ => Kind::Context,
    };
    let text = if kind == Kind::Hunk && mark == '@' {
        raw.to_owned()
    } else {
        rest.to_owned()
    };
    DiffLine {
        kind,
        old: None,
        new: None,
        text: text.replace('\t', "    "),
        raw: raw.to_owned(),
    }
}

/// Draws `data`'s lines, scrolling both ways, in what is left of `ui`.
pub(in crate::repo) fn interdiff_rows(
    ui: &mut Ui,
    data: &DiffData,
    id: impl std::hash::Hash + std::fmt::Debug,
) {
    let t = Tokens::get(ui.ctx());
    let font = theme::mono(12.0);
    let char_w = ui
        .painter()
        .layout_no_wrap("M".to_owned(), font.clone(), t.text)
        .size()
        .x;
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::both()
        .id_salt(("interdiff", id))
        .auto_shrink(false)
        .show_rows(ui, LINE_H, data.lines.len(), |ui, range| {
            let width = (24.0 + data.longest as f32 * char_w + 24.0).max(ui.available_width());
            for i in range {
                let (rect, _) = ui.allocate_exact_size(vec2(width, LINE_H), Sense::hover());
                paint_line(ui, rect, data, i, Side::Both, 0.0, &font, &t, false, None);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn outer_marks_decide_how_a_line_shows() {
        let data = interdiff_data(&body(&[
            "@@ Metadata",
            "  ## Commit message ##",
            "-    change ten",
            "+    change ten (again)",
            " @@ big.txt",
            "  9",
            "-+ten",
            "++TEN",
            "",
        ]));
        let kinds: Vec<Kind> = data.lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Hunk,
                Kind::Hunk,
                Kind::Del,
                Kind::Add,
                Kind::Context,
                Kind::Context,
                Kind::Del,
                Kind::Add,
                Kind::Context,
            ]
        );
        // The patch's own marks stay in the text; the outer one is the colour.
        assert_eq!(data.lines[6].text, "+ten");
        assert_eq!(data.lines[7].text, "+TEN");
        assert_eq!(data.lines[0].text, "@@ Metadata");
        // A removed line and the added one after it have their changed part marked.
        assert!(data.changed[2].is_some() && data.changed[3].is_some());
        assert_eq!(data.longest, "    change ten (again)".len());
    }

    #[test]
    fn tabs_and_very_long_lines_are_drawable() {
        let long = format!("+{}", "é".repeat(MAX_LINE_CHARS + 50));
        let data = interdiff_data(&body(&["+\tx", &long]));
        assert_eq!(data.lines[0].text, "    x");
        assert!(data.lines[1].text.ends_with("more characters"));
        assert!(data.uncut.contains_key(&1));
    }
}
