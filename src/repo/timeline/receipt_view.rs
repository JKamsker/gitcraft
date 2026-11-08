//! The receipt of a rewrite in the timeline's detail: the one-line summary, then each old
//! commit followed to the one that replaced it.

use egui::{RichText, Ui};

use super::receipt::{Commit, Pair, PairKind, Receipt};
use crate::repo::short;
use crate::theme::{self, Tokens};

/// Draws `receipt`.
pub(super) fn receipt_section(ui: &mut Ui, receipt: &Receipt) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-timeline-receipt-section-a5fcdee7",
        ))
        .font(theme::semibold(10.5))
        .color(t.text_faint),
    );
    ui.add_space(2.0);
    ui.add(egui::Label::new(RichText::new(receipt.sentence()).color(t.text)).wrap());
    ui.label(
        RichText::new(format!(
            "{}: {}  →  {}",
            receipt.branch,
            short(&receipt.old_tip),
            short(&receipt.new_tip)
        ))
        .font(theme::mono(12.0))
        .color(t.text_dim),
    );
    if receipt.pairs.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(crate::i18n::message(
        "repo-workflows-timeline-receipt-section-94c37af0",
        &[("receipt-pairs-len-1", (receipt.pairs.len()).clone().into())],
    ))
    .id_salt(("receipt", &receipt.new_tip))
    .show(ui, |ui| {
        for pair in &receipt.pairs {
            pair_row(ui, pair);
            ui.add_space(4.0);
        }
    });
}

/// One old commit and its replacement, with what differs between them.
fn pair_row(ui: &mut Ui, pair: &Pair) {
    let t = Tokens::get(ui.ctx());
    let (mark, color, meaning) = match pair.kind {
        PairKind::Same => (
            "=",
            t.success,
            crate::i18n::text("repo-workflows-timeline-pair-row-dbbc6123"),
        ),
        PairKind::Changed => (
            "≠",
            t.warning,
            crate::i18n::text("repo-workflows-timeline-pair-row-d28f50d6"),
        ),
        PairKind::Added => (
            "+",
            t.accent,
            crate::i18n::text("repo-workflows-timeline-pair-row-1162b77f"),
        ),
        PairKind::Dropped => (
            "−",
            t.danger,
            crate::i18n::text("repo-workflows-timeline-pair-row-086cbeb5"),
        ),
    };
    let side = |c: &Option<Commit>, missing: &str| match c {
        Some(c) => format!("{} {}", short(&c.hash), c.summary),
        None => missing.to_owned(),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(mark).font(theme::semibold(13.0)).color(color))
            .on_hover_text(meaning);
        ui.label(
            RichText::new(format!(
                "{}  →  {}",
                side(&pair.old, crate::i18n::text("repo-workflows-paint-new")),
                side(&pair.new, crate::i18n::text("repo-workflows-paint-dropped"))
            ))
            .font(theme::mono(12.0))
            .color(t.text_dim),
        );
    });
    let notes = notes(pair);
    if !notes.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(18.0);
            ui.label(
                RichText::new(notes.join(" · "))
                    .size(11.5)
                    .color(t.text_faint),
            );
        });
    }
}

/// What differs within a pair, in words.
pub(super) fn notes(pair: &Pair) -> Vec<&'static str> {
    let mut notes = Vec::new();
    match pair.kind {
        PairKind::Added => notes.push(crate::i18n::text("repo-workflows-timeline-notes-6e54e0e9")),
        PairKind::Dropped => {
            notes.push(crate::i18n::text("repo-workflows-timeline-notes-e448894a"))
        }
        PairKind::Same => notes.push(crate::i18n::text("repo-workflows-timeline-notes-32f54038")),
        PairKind::Changed => {
            notes.push(crate::i18n::text("repo-workflows-timeline-notes-b5c609b3"))
        }
    }
    // A same change on a moved base has other files: only worth saying for a changed one.
    if pair.paired() && pair.kind == PairKind::Changed {
        notes.push(if pair.same_tree() {
            crate::i18n::text("repo-workflows-timeline-notes-553d30ef")
        } else {
            crate::i18n::text("repo-workflows-timeline-notes-b2f0d6f0")
        });
    }
    if pair.message_changed() {
        notes.push(crate::i18n::text("repo-workflows-timeline-notes-431569df"));
    }
    if pair.author_changed() {
        notes.push(crate::i18n::text("repo-workflows-timeline-notes-fcc760d7"));
    }
    match (&pair.old, &pair.new) {
        (Some(o), Some(n)) if o.signed && n.signed => {
            notes.push(crate::i18n::text("repo-workflows-timeline-notes-716dcf0f"))
        }
        (Some(o), Some(_)) if o.signed => {
            notes.push(crate::i18n::text("repo-workflows-timeline-notes-c267b6be"))
        }
        (Some(o), Some(n)) if !o.signed && n.signed => {
            notes.push(crate::i18n::text("repo-workflows-timeline-notes-521926c1"))
        }
        _ => {}
    }
    notes
}
