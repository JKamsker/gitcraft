//! Folded files in a file list: gathered at its end under one row, _3 lockfiles +1,204 −880_,
//! closed until clicked.

use std::sync::Arc;

use egui::{Align2, Id, Rect, Sense, Ui, pos2, vec2};

use super::{FoldReason, Folding, Rules, rules::thousands};
use crate::icons;
use crate::repo::Action;
use crate::repo::widgets::ROW_H;
use crate::theme::{self, Tokens};

/// A list's files: those shown as usual, and those folded at its end.
pub(in crate::repo) struct FoldGroup<'a, T> {
    pub(in crate::repo) shown: Vec<&'a T>,
    pub(in crate::repo) folded: Vec<&'a T>,
    reasons: Vec<FoldReason>,
}

/// Which files of a list fold, worked out when the list, the settings or what is known of
/// the attributes change, not every frame (a commit can change tens of thousands of files).
#[derive(Clone)]
struct Memo {
    key: Id,
    folded: Arc<Vec<(usize, FoldReason)>>,
}

/// Splits `files` (each at `path`) into those shown and those folded. Paths whose attributes
/// aren't known are asked about (the answer folds them a moment later, if it says to).
pub(in crate::repo) fn split<'a, T>(
    ui: &Ui,
    id: Id,
    files: &'a [T],
    path: impl Fn(&T) -> &str,
    (folding, rules): (&Folding, &Rules),
    acts: &mut Vec<Action>,
) -> FoldGroup<'a, T> {
    if !rules.on || files.is_empty() {
        return FoldGroup {
            shown: files.iter().collect(),
            folded: Vec::new(),
            reasons: Vec::new(),
        };
    }
    // The list itself, what is known and the settings.
    let key = id.with((list_fingerprint(files, &path), folding.generation, rules));
    let memo_id = id.with("fold-memo");
    let memo = ui
        .data(|d| d.get_temp::<Memo>(memo_id))
        .filter(|m| m.key == key);
    let folded = match memo {
        Some(memo) => memo.folded,
        None => {
            let unknown = folding.unknown(files.iter().map(&path));
            if !unknown.is_empty() {
                acts.push(Action::FoldAttributes(unknown));
            }
            let folded: Arc<Vec<(usize, FoldReason)>> = Arc::new(
                files
                    .iter()
                    .enumerate()
                    .filter_map(|(i, f)| folding.reason(rules, path(f)).map(|r| (i, r)))
                    .collect(),
            );
            let memo = Memo {
                key,
                folded: folded.clone(),
            };
            ui.data_mut(|d| d.insert_temp(memo_id, memo));
            folded
        }
    };
    let mut shown = Vec::with_capacity(files.len() - folded.len());
    let mut next = folded.iter().map(|&(i, _)| i).peekable();
    for (i, f) in files.iter().enumerate() {
        if next.peek() == Some(&i) {
            next.next();
        } else {
            shown.push(f);
        }
    }
    FoldGroup {
        shown,
        folded: folded.iter().map(|&(i, _)| &files[i]).collect(),
        reasons: folded.iter().map(|&(_, r)| r).collect(),
    }
}

/// Lists longer than this are told apart by some of their paths, not all, every frame.
const ALL_PATHS_UP_TO: usize = 4096;

/// What tells a list from another one shown in the same place: where it is, its length, and
/// its paths (every one in a list of up to [`ALL_PATHS_UP_TO`], else as many spread over it).
pub(super) fn list_fingerprint<T>(files: &[T], path: impl Fn(&T) -> &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (files.as_ptr() as usize, files.len()).hash(&mut h);
    let step = files.len().div_ceil(ALL_PATHS_UP_TO).max(1);
    for f in files.iter().step_by(step) {
        path(f).hash(&mut h);
    }
    if let Some(last) = files.last() {
        path(last).hash(&mut h);
    }
    h.finish()
}

impl<T> FoldGroup<'_, T> {
    /// The row the folded files are under (none when no file folds), opening or closing them
    /// when clicked; `open_id`: where it keeps whether it is open; `stats`: the lines they add
    /// and remove, when known; `selected`: it holds the file whose diff is open, and stands
    /// out while closed. True while open.
    pub(in crate::repo) fn header(
        &self,
        ui: &mut Ui,
        open_id: Id,
        stats: Option<(u32, u32)>,
        selected: bool,
    ) -> bool {
        if self.folded.is_empty() {
            return false;
        }
        let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
        let t = Tokens::get(ui.ctx());
        let (rect, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
        crate::a11y::expanded(
            &resp,
            || crate::i18n::message("repo-core-2543", &[("count", (self.folded.len()).into())]),
            open,
        );
        let painter = ui.painter_at(rect);
        if resp.hovered() {
            painter.rect_filled(rect, 0.0, t.hover);
        }
        let y = rect.center().y;
        let icon = if open {
            "chevron-down"
        } else {
            "chevron-right"
        };
        let tint = if selected && !open {
            t.accent
        } else {
            t.text_faint
        };
        icons::paint(
            ui,
            Rect::from_center_size(pos2(rect.left() + 18.0, y), vec2(14.0, 14.0)),
            icon,
            12.0,
            tint,
        );
        let mut right = rect.right() - 8.0;
        if let Some((added, removed)) = stats {
            let small = theme::mono(11.0);
            let r = painter.text(
                pos2(right, y),
                Align2::RIGHT_CENTER,
                format!("−{}", thousands(removed)),
                small.clone(),
                t.danger,
            );
            let r = painter.text(
                pos2(r.left() - 6.0, y),
                Align2::RIGHT_CENTER,
                format!("+{}", thousands(added)),
                small,
                t.success,
            );
            right = r.left() - 8.0;
        }
        painter.with_clip_rect(rect.with_max_x(right)).text(
            pos2(rect.left() + 32.0, y),
            Align2::LEFT_CENTER,
            group_title(&self.reasons),
            egui::FontId::proportional(12.5),
            t.text_dim,
        );
        let resp = resp.on_hover_text(if open {
            crate::i18n::text("repo-core-2544")
        } else {
            crate::i18n::text("repo-core-2545")
        });
        if resp.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(open_id, open));
        }
        open
    }
}

/// _3 lockfiles_, _1 generated file_, _4 folded files_ (of several kinds).
pub(super) fn group_title(reasons: &[FoldReason]) -> String {
    let n = reasons.len();
    let one = n == 1;
    let same = reasons.first().filter(|r| reasons.iter().all(|x| x == *r));
    let what = match same {
        Some(FoldReason::Lockfile) => {
            if one {
                crate::i18n::text("repo-core-2546")
            } else {
                crate::i18n::text("repo-core-2547")
            }
        }
        Some(FoldReason::Generated) => {
            if one {
                crate::i18n::text("repo-core-2548")
            } else {
                crate::i18n::text("repo-core-2549")
            }
        }
        Some(FoldReason::Minified) => {
            if one {
                crate::i18n::text("repo-core-2550")
            } else {
                crate::i18n::text("repo-core-2551")
            }
        }
        Some(FoldReason::NoDiff) => {
            if one {
                crate::i18n::text("repo-core-2552")
            } else {
                crate::i18n::text("repo-core-2553")
            }
        }
        Some(FoldReason::Pattern) | None => {
            if one {
                crate::i18n::text("repo-core-2554")
            } else {
                crate::i18n::text("repo-core-2555")
            }
        }
    };
    crate::i18n::message(
        "repo-core-1911",
        &[("n", (n).into()), ("what", what.to_string().into())],
    )
}
