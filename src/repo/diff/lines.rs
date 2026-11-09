//! Which lines of which version of the file a selection in a diff stands for, to read their
//! history (`git log -L`).

use std::collections::BTreeSet;

use super::select::{Side, TextSel};
use super::{DiffLine, Kind};
use crate::git::{Area, LineRange, LinesAt};
use crate::repo::DiffKey;

/// The lines picked in a diff, and where their numbers count from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedLines {
    pub path: String,
    pub at: LinesAt,
    pub range: LineRange,
}

/// The numbers of the lines selected, on the new side and on the old one: the text dragged
/// over (`sel`), or else the changed lines clicked (`chosen`).
fn numbers(
    lines: &[DiffLine],
    sel: Option<&TextSel>,
    chosen: &BTreeSet<usize>,
) -> (Option<LineRange>, Option<LineRange>) {
    let picked: Vec<(usize, Side)> = match sel {
        Some(s) => {
            let ((from, _), (to, _)) = (s.anchor.min(s.cursor), s.anchor.max(s.cursor));
            (from..=to.min(lines.len().saturating_sub(1)))
                .filter(|&i| s.chars_of(i, s.side).is_some())
                .map(|i| (i, s.side))
                .collect()
        }
        None => chosen.iter().map(|&i| (i, Side::Unified)).collect(),
    };
    let mut new: Option<LineRange> = None;
    let mut old: Option<LineRange> = None;
    let widen = |r: &mut Option<LineRange>, n: u32| {
        *r = Some(r.map_or((n, n), |(a, b)| (a.min(n), b.max(n))));
    };
    for (i, side) in picked {
        let Some(line) = lines.get(i).filter(|l| side.shows(l.kind)) else {
            continue;
        };
        if side != Side::Old
            && line.kind != Kind::Del
            && let Some(n) = line.new
        {
            widen(&mut new, n);
        }
        if side != Side::New
            && line.kind != Kind::Add
            && let Some(n) = line.old
        {
            widen(&mut old, n);
        }
    }
    (new, old)
}

/// The version of the file on each side of the diff of `key`: (new side, old side), with the
/// path on the old side (`old_path`, when the file was renamed).
fn sides(key: &DiffKey, old_path: Option<&str>) -> Option<((LinesAt, String), (LinesAt, String))> {
    let path = key.path().to_owned();
    let old = old_path.map_or_else(|| path.clone(), str::to_owned);
    let (new_at, old_at) = match key {
        DiffKey::Remerge { .. } => return None,
        DiffKey::Commit { hash, .. } => {
            (LinesAt::Rev(hash.clone()), LinesAt::Rev(format!("{hash}^")))
        }
        DiffKey::Range { from, to, .. } => (
            to.clone().map_or(LinesAt::WorkTree, LinesAt::Rev),
            LinesAt::Rev(from.clone()),
        ),
        DiffKey::Work(file) => match file.area {
            Area::Staged => (LinesAt::Index, LinesAt::Rev("HEAD".to_owned())),
            Area::Unstaged => (LinesAt::WorkTree, LinesAt::Index),
            Area::Untracked | Area::Conflicted => return None,
        },
    };
    Some(((new_at, path), (old_at, old)))
}

/// The lines whose history the selection in the diff of `key` asks for: those on the new side,
/// or the old side's when only removed lines are selected. `Err`: why there are none.
pub fn picked_lines(
    key: &DiffKey,
    old_path: Option<&str>,
    lines: &[DiffLine],
    sel: Option<&TextSel>,
    chosen: &BTreeSet<usize>,
) -> Result<PickedLines, &'static str> {
    let Some(((new_at, new_path), (old_at, old_path))) = sides(key, old_path) else {
        return Err(crate::i18n::text("repo-core-1856"));
    };
    match numbers(lines, sel, chosen) {
        (Some(range), _) => Ok(PickedLines {
            path: new_path,
            at: new_at,
            range,
        }),
        (None, Some(range)) => Ok(PickedLines {
            path: old_path,
            at: old_at,
            range,
        }),
        (None, None) => Err(crate::i18n::text("repo-core-1857")),
    }
}

/// The lines picked in the whole file shown for the diff of `key` (`before`: the file as it
/// was before the change, which deleted it).
pub fn picked_in_file(
    key: &DiffKey,
    old_path: Option<&str>,
    before: bool,
    range: LineRange,
) -> Option<PickedLines> {
    let ((new_at, new_path), (old_at, old_path)) = sides(key, old_path)?;
    let (at, path) = if before {
        (old_at, old_path)
    } else {
        (new_at, new_path)
    };
    Some(PickedLines { path, at, range })
}

/// The old name of a file the patch `header` renames (`rename from …`).
pub fn old_name(header: &str) -> Option<String> {
    header
        .lines()
        .find_map(|l| l.strip_prefix("rename from "))
        .map(crate::git::unquote_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::StatusFile;
    use crate::repo::diff::parse;

    fn diff() -> Vec<DiffLine> {
        parse("@@ -10,4 +10,4 @@\n ten\n-eleven\n+ELEVEN\n twelve\n-thirteen\n+THIRTEEN\n")
    }

    fn commit() -> DiffKey {
        DiffKey::Commit {
            hash: "abc".to_owned(),
            path: "new.txt".to_owned(),
        }
    }

    fn sel(side: Side, from: usize, to: usize) -> TextSel {
        TextSel {
            side,
            anchor: (from, 0),
            cursor: (to, 3),
        }
    }

    #[test]
    fn a_selection_in_a_commit_is_read_at_the_commit() {
        let lines = diff();
        // From "ten" to "twelve", unified: new lines 10–12.
        let picked = picked_lines(
            &commit(),
            None,
            &lines,
            Some(&sel(Side::Unified, 1, 4)),
            &BTreeSet::new(),
        );
        assert_eq!(
            picked,
            Ok(PickedLines {
                path: "new.txt".to_owned(),
                at: LinesAt::Rev("abc".to_owned()),
                range: (10, 12),
            })
        );
    }

    #[test]
    fn only_removed_lines_are_read_before_the_commit_under_the_old_name() {
        let lines = diff();
        // The removed "eleven" only.
        let picked = picked_lines(
            &commit(),
            Some("old.txt"),
            &lines,
            Some(&sel(Side::Unified, 2, 2)),
            &BTreeSet::new(),
        );
        assert_eq!(
            picked,
            Ok(PickedLines {
                path: "old.txt".to_owned(),
                at: LinesAt::Rev("abc^".to_owned()),
                range: (11, 11),
            })
        );
        // Side by side, the old half.
        let old = picked_lines(
            &commit(),
            None,
            &lines,
            Some(&sel(Side::Old, 1, 5)),
            &BTreeSet::new(),
        );
        assert_eq!(
            old.map(|p| (p.at, p.range)),
            Ok((LinesAt::Rev("abc^".to_owned()), (10, 13)))
        );
    }

    #[test]
    fn clicked_lines_count_when_nothing_is_dragged_over() {
        let lines = diff();
        let chosen: BTreeSet<usize> = [3, 6].into();
        let picked = picked_lines(&commit(), None, &lines, None, &chosen);
        assert_eq!(picked.map(|p| p.range), Ok((11, 13)));
        assert!(picked_lines(&commit(), None, &lines, None, &BTreeSet::new()).is_err());
    }

    #[test]
    fn uncommitted_lines_count_from_disk_or_the_index() {
        let lines = diff();
        let work = |area| {
            DiffKey::Work(StatusFile {
                path: "f".to_owned(),
                old_path: None,
                code: 'M',
                area,
            })
        };
        let s = sel(Side::Unified, 1, 1);
        let at = |key: &DiffKey| {
            picked_lines(key, None, &lines, Some(&s), &BTreeSet::new()).map(|p| p.at)
        };
        assert_eq!(at(&work(Area::Unstaged)), Ok(LinesAt::WorkTree));
        assert_eq!(at(&work(Area::Staged)), Ok(LinesAt::Index));
        assert!(at(&work(Area::Untracked)).is_err());
        let range = DiffKey::Range {
            from: "a".to_owned(),
            to: None,
            path: "f".to_owned(),
        };
        assert_eq!(at(&range), Ok(LinesAt::WorkTree));
    }

    #[test]
    fn the_whole_file_of_a_diff() {
        let p = picked_in_file(&commit(), Some("old.txt"), true, (1, 2));
        assert_eq!(
            p.map(|p| (p.path, p.at)),
            Some(("old.txt".to_owned(), LinesAt::Rev("abc^".to_owned())))
        );
        assert_eq!(
            old_name(
                "diff --git a/x b/y\nsimilarity index 90%\nrename from \"\\303\\274.txt\"\nrename to y\n"
            ),
            Some("ü.txt".to_owned())
        );
    }
}
