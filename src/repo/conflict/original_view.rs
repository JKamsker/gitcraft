//! A conflict card's Original section: the base, and what each side inserted and deleted
//! relative to it.

use egui::{Color32, RichText, Stroke, Ui};

use super::original::{Change, Lines, Original};
use crate::icons;
use crate::repo::RepoTab;
use crate::theme::{self, Tokens};

/// At most this many lines of a side are listed; the rest are counted.
const MAX_SHOWN: usize = 400;

impl RepoTab {
    /// Asks the index for the bases of the open conflict editor's conflicts, in the background,
    /// once each time they are wanted (as it opens, or after Mergiraf left conflicts without
    /// one): cheap to call every frame.
    pub(in crate::repo) fn request_conflict_bases(&mut self, ctx: &egui::Context) {
        let Some(ed) = self
            .conflict
            .as_mut()
            .filter(|ed| ed.wants_bases() && !ed.original.asked)
        else {
            return;
        };
        ed.original.asked = true;
        let (repo, path, generation) = (self.path.clone(), ed.path.clone(), ed.generation);
        let read = path.clone();
        self.then(
            ctx,
            move || super::original::load(&repo, &read),
            move |tab, bases, _| {
                if let Some(ed) = tab
                    .conflict
                    .as_mut()
                    .filter(|ed| ed.path == path && ed.generation == generation)
                {
                    ed.original.set_index(bases, &ed.segments);
                }
            },
        );
    }
}

/// The Original section of conflict `k`; true when its header was clicked (to open or close
/// it). `diffs`: the sides' changes against the base, once worked out; `names`: the sides'.
pub(super) fn section(
    ui: &mut Ui,
    t: &Tokens,
    (k, open): (usize, bool),
    original: Original<'_>,
    diffs: Option<&(Lines, Lines)>,
    names: (&str, &str),
) -> bool {
    let tip = if open {
        crate::i18n::text("repo-core-0619")
    } else {
        crate::i18n::text("repo-core-0620")
    };
    let mut clicked = false;
    ui.horizontal_wrapped(|ui| {
        let icon = if open {
            "chevron-down"
        } else {
            "chevron-right"
        };
        icons::inline(ui, icon, 12.0, t.text_dim);
        let button = egui::Button::new(
            RichText::new(crate::i18n::text("repo-core-0621"))
                .font(theme::medium(11.5))
                .color(t.text_dim),
        )
        .frame(false);
        clicked = ui.add(button).on_hover_text(tip).clicked();
        let note = match (original, diffs) {
            (Original::Loading, _) => crate::i18n::text("repo-core-0622").to_owned(),
            (Original::Added, _) => crate::i18n::text("repo-core-0623").to_owned(),
            (Original::Unknown(_), _) => crate::i18n::text("repo-core-0624").to_owned(),
            (Original::Text(_), Some((ours, theirs))) => format!(
                "{}: {} · {}: {}",
                names.0,
                super::original::summary(ours),
                names.1,
                super::original::summary(theirs)
            ),
            (Original::Text(_), None) => crate::i18n::text("repo-core-0625").to_owned(),
        };
        ui.label(RichText::new(note).color(t.text_faint).small());
    });
    if open {
        ui.add_space(4.0);
        body(ui, t, k, original, diffs, names);
    }
    clicked
}

/// The open section: each side's lines against the base, or why there is no base.
fn body(
    ui: &mut Ui,
    t: &Tokens,
    k: usize,
    original: Original<'_>,
    diffs: Option<&(Lines, Lines)>,
    names: (&str, &str),
) {
    match (original, diffs) {
        (Original::Loading, _) | (Original::Text(_), None) => {
            ui.add(crate::spinner::Spinner::new());
        }
        (Original::Added, _) => {
            note(
                ui,
                t,
                crate::i18n::text("repo-core-0626"),
            );
        }
        (Original::Unknown(why), _) => note(ui, t, &crate::i18n::message("repo-core-0627", &[("why", format!("{}", why).into())])),
        (Original::Text(base), Some((ours, theirs))) => {
            if base.is_empty() {
                note(
                    ui,
                    t,
                    crate::i18n::text("repo-core-0628"),
                );
            }
            ui.columns(2, |cols| {
                for (col, (name, lines, side)) in cols
                    .iter_mut()
                    .zip([(names.0, ours, true), (names.1, theirs, false)])
                {
                    side_changes(col, t, (k, side), name, lines);
                }
            });
        }
    }
}

fn note(ui: &mut Ui, t: &Tokens, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(t.text_dim)).wrap());
}

/// One side's lines against the base: inserted ones on the diff's green, deleted ones on its
/// red and struck through.
fn side_changes(ui: &mut Ui, t: &Tokens, (k, ours): (usize, bool), name: &str, lines: &Lines) {
    egui::Frame::new()
        .fill(t.field)
        .stroke(Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(crate::i18n::message("repo-core-0629", &[("name", format!("{}", name).into()), ("lines", format!("{}", super::original::summary(lines)).into())]))
                .color(t.text_faint)
                .small(),
            );
            egui::ScrollArea::horizontal()
                .id_salt(("conflict-original", k, ours))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (change, text) in lines.iter().take(MAX_SHOWN) {
                        line(ui, t, *change, text);
                    }
                    if lines.len() > MAX_SHOWN {
                        ui.label(
                            RichText::new(crate::i18n::message("repo-core-0631", &[("MAX-SHOWN", format!("{}", lines.len() - MAX_SHOWN).into())]))
                                .color(t.text_faint)
                                .small(),
                        );
                    }
                });
        });
}

fn line(ui: &mut Ui, t: &Tokens, change: Change, text: &str) {
    let (mark, fill) = match change {
        Change::Same => (' ', Color32::TRANSPARENT),
        Change::Added => ('+', t.diff_add_bg),
        Change::Removed => ('-', t.diff_del_bg),
    };
    let mut rich = RichText::new(format!("{mark} {text}"))
        .font(theme::mono(12.0))
        .background_color(fill);
    if change == Change::Removed {
        rich = rich.strikethrough();
    }
    if change == Change::Same {
        rich = rich.color(t.text_dim);
    }
    ui.add(egui::Label::new(rich).extend());
}
