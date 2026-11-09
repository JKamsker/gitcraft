//! Links to the lines of a commit's diff on the forge: which lines of which side of the file the
//! rows picked are, and the entries of the lines' menu that copy or open the link.

use egui::Ui;

use super::select::{Side, TextSel};
use super::{DiffLine, Kind};
use crate::forge::links::{Lines, Target};
use crate::repo::Action;
use crate::repo::forge_link::LinkMenu;

/// Where the two sides of a diff are, for links to their lines: the file at `new` (after the
/// change) and at `old` (before it).
pub(in crate::repo) struct LineLinks<'a> {
    pub path: String,
    pub new: String,
    pub old: String,
    /// The remotes the links can go to, asked for only once the menu is open.
    pub menu: &'a dyn Fn() -> Option<LinkMenu>,
}

/// Which side of the file rows `first..=last` of a diff are on, and their lines there: on a
/// side of the split view, that side's; unified, the new file's when any of the rows is in it,
/// else (only removed lines) the old file's. `true` for the old side; `None` when no row is a
/// line of the file (a hunk header).
pub(super) fn link_lines(
    lines: &[DiffLine],
    (first, last): (usize, usize),
    side: Side,
) -> Option<(bool, Lines)> {
    let rows = lines.get(first..=last.min(lines.len().saturating_sub(1)))?;
    let span = |old: bool| {
        let numbers = rows.iter().filter_map(|l| {
            let kept = matches!(l.kind, Kind::Context)
                || l.kind == if old { Kind::Del } else { Kind::Add };
            if !kept {
                return None;
            }
            if old { l.old } else { l.new }
        });
        let (lo, hi) = numbers.fold((u32::MAX, 0), |(lo, hi), n| (lo.min(n), hi.max(n)));
        Lines::new(lo, hi).map(|lines| (old, lines))
    };
    match side {
        Side::Old => span(true),
        Side::New => span(false),
        Side::Unified | Side::Both => span(false).or_else(|| span(true)),
    }
}

/// Where the last right-click on the lines was, for their menu: the row and side.
fn clicked_id() -> egui::Id {
    egui::Id::new("diff_link_row")
}

/// Notes the row right-clicked (`hit`; `None`: below the last line), which the lines' menu links
/// to unless it is in the text selected. Each diff's rows are their own: what was noted for
/// another one is dropped.
pub(super) fn note_right_click(ui: &Ui, diff: egui::Id, hit: Option<(usize, Side)>) {
    ui.data_mut(|d| match hit {
        Some(hit) => {
            d.insert_temp(clicked_id(), (diff, hit));
        }
        None => d.remove::<(egui::Id, (usize, Side))>(clicked_id()),
    });
}

impl LineLinks<'_> {
    /// Which diff these are the links of, for the row right-clicked in it.
    pub(super) fn id(&self) -> egui::Id {
        egui::Id::new(("diff_links", &self.path, &self.new, &self.old))
    }

    /// The menu's entries for the row right-clicked, or the rows selected when it is one of
    /// them. `header`: the patch's, which names a renamed file's old path.
    pub(super) fn items(
        &self,
        ui: &mut Ui,
        (lines, header): (&[DiffLine], &str),
        sel: Option<TextSel>,
        acts: &mut Vec<Action>,
    ) {
        let clicked = ui
            .data(|d| d.get_temp::<(egui::Id, (usize, Side))>(clicked_id()))
            .filter(|(diff, _)| *diff == self.id())
            .map(|(_, hit)| hit);
        let rows = rows_to_link(sel.map(|s| (s.rows(), s.side)), clicked);
        let Some((old, picked)) = rows.and_then(|(rows, side)| link_lines(lines, rows, side))
        else {
            return;
        };
        let Some(menu) = (self.menu)() else { return };
        ui.separator();
        let target = self.target(old, picked, header);
        let what = if old {
            crate::i18n::message(
                "repo-core-1860",
                &[("label", picked.label().to_string().into())],
            )
        } else {
            picked.label()
        };
        menu.items(ui, &what, &target, acts);
    }
}

impl LineLinks<'_> {
    /// The file lines `picked` are of: before the change (`old`) under the name the patch's
    /// `header` says it was renamed from, if it was.
    fn target(&self, old: bool, picked: Lines, header: &str) -> Target {
        let (rev, path) = if old {
            let path = super::images::renamed_from(header).unwrap_or(&self.path);
            (&self.old, path)
        } else {
            (&self.new, self.path.as_str())
        };
        Target::File {
            rev: rev.clone(),
            path: path.to_owned(),
            lines: Some(picked),
        }
    }
}

/// The rows a link goes to: the row right-clicked (`clicked`), or the rows selected (`sel`)
/// when it is one of them or nothing was right-clicked in this diff.
fn rows_to_link(
    sel: Option<((usize, usize), Side)>,
    clicked: Option<(usize, Side)>,
) -> Option<((usize, usize), Side)> {
    match (sel, clicked) {
        (Some(((a, b), side)), Some((i, _))) if (a..=b).contains(&i) => Some(((a, b), side)),
        (_, Some((i, side))) => Some(((i, i), side)),
        (sel, None) => sel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: Kind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            old,
            new,
            text: String::new(),
            raw: String::new(),
        }
    }

    /// `@@ -10,4 +10,4 @@`: a context line, two removed, two added, a context line.
    fn hunk() -> Vec<DiffLine> {
        vec![
            line(Kind::Hunk, None, None),
            line(Kind::Context, Some(10), Some(10)),
            line(Kind::Del, Some(11), None),
            line(Kind::Del, Some(12), None),
            line(Kind::Add, None, Some(11)),
            line(Kind::Add, None, Some(12)),
            line(Kind::Context, Some(13), Some(13)),
        ]
    }

    #[test]
    fn unified_rows_link_to_the_new_file_when_they_reach_it() {
        let lines = hunk();
        assert_eq!(
            link_lines(&lines, (1, 6), Side::Unified),
            Some((
                false,
                Lines {
                    first: 10,
                    last: 13
                }
            ))
        );
        assert_eq!(
            link_lines(&lines, (4, 4), Side::Unified),
            Some((
                false,
                Lines {
                    first: 11,
                    last: 11
                }
            ))
        );
        // Only removed lines: they are in the file before the change.
        assert_eq!(
            link_lines(&lines, (2, 3), Side::Unified),
            Some((
                true,
                Lines {
                    first: 11,
                    last: 12
                }
            ))
        );
    }

    #[test]
    fn a_side_of_the_split_view_links_to_its_own_file() {
        let lines = hunk();
        assert_eq!(
            link_lines(&lines, (1, 6), Side::Old),
            Some((
                true,
                Lines {
                    first: 10,
                    last: 13
                }
            ))
        );
        assert_eq!(
            link_lines(&lines, (2, 5), Side::New),
            Some((
                false,
                Lines {
                    first: 11,
                    last: 12
                }
            ))
        );
    }

    #[test]
    fn a_right_click_outside_the_selection_links_its_own_row() {
        let sel = Some(((3, 6), Side::Unified));
        assert_eq!(rows_to_link(sel, Some((4, Side::Unified))), sel);
        assert_eq!(
            rows_to_link(sel, Some((9, Side::New))),
            Some(((9, 9), Side::New))
        );
        assert_eq!(rows_to_link(sel, None), sel);
        assert_eq!(
            rows_to_link(None, Some((2, Side::Old))),
            Some(((2, 2), Side::Old))
        );
        assert_eq!(rows_to_link(None, None), None);
    }

    #[test]
    fn removed_lines_of_a_renamed_file_link_to_its_old_name() {
        let menu = || None;
        let links = LineLinks {
            path: "new name.rs".into(),
            new: "abc".into(),
            old: "abc^".into(),
            menu: &menu,
        };
        let header = "diff --git a/old.rs b/new name.rs\nsimilarity index 90%\n\
                      rename from old.rs\nrename to new name.rs\n";
        let picked = Lines { first: 2, last: 3 };
        let file = |rev: &str, path: &str| Target::File {
            rev: rev.into(),
            path: path.into(),
            lines: Some(picked),
        };
        assert_eq!(links.target(true, picked, header), file("abc^", "old.rs"));
        assert_eq!(
            links.target(false, picked, header),
            file("abc", "new name.rs")
        );
        assert_eq!(
            links.target(true, picked, "diff --git a/x b/x\n"),
            file("abc^", "new name.rs")
        );
    }

    #[test]
    fn a_hunk_header_or_nothing_has_no_lines() {
        let lines = hunk();
        assert_eq!(link_lines(&lines, (0, 0), Side::Both), None);
        assert_eq!(link_lines(&[], (0, 0), Side::Unified), None);
        assert_eq!(link_lines(&lines, (40, 50), Side::Unified), None);
        // Rows past the end are the last one's.
        assert_eq!(
            link_lines(&lines, (6, 99), Side::Unified),
            Some((
                false,
                Lines {
                    first: 13,
                    last: 13
                }
            ))
        );
    }
}
