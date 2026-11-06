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

/// Where a right-click on the lines was, for their menu: the row and side.
fn clicked_id(path: &str) -> egui::Id {
    egui::Id::new(("diff_link_row", path))
}

/// Notes the row right-clicked (`hit`), which the lines' menu links to when no text is selected.
pub(super) fn note_right_click(ui: &Ui, path: &str, hit: Option<(usize, Side)>) {
    if let Some(hit) = hit {
        ui.data_mut(|d| d.insert_temp(clicked_id(path), hit));
    }
}

impl LineLinks<'_> {
    /// The menu's entries for the selected rows (`sel`), else the one right-clicked.
    pub(super) fn items(
        &self,
        ui: &mut Ui,
        lines: &[DiffLine],
        sel: Option<TextSel>,
        acts: &mut Vec<Action>,
    ) {
        let rows = match sel {
            Some(sel) => Some((sel.rows(), sel.side)),
            None => ui
                .data(|d| d.get_temp::<(usize, Side)>(clicked_id(&self.path)))
                .map(|(i, side)| ((i, i), side)),
        };
        let Some((old, picked)) = rows.and_then(|(rows, side)| link_lines(lines, rows, side))
        else {
            return;
        };
        let Some(menu) = (self.menu)() else { return };
        ui.separator();
        let rev = if old { &self.old } else { &self.new };
        let target = Target::File {
            rev: rev.clone(),
            path: self.path.clone(),
            lines: Some(picked),
        };
        menu.items(ui, &picked.label(), &target, acts);
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
