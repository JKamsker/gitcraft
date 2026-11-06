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
        RichText::new("REWRITE RECEIPT")
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
    egui::CollapsingHeader::new(format!("Follow each commit ({})", receipt.pairs.len()))
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
    let (mark, color) = match pair.kind {
        PairKind::Same => ("=", t.success),
        PairKind::Changed => ("≠", t.warning),
        PairKind::Added => ("+", t.accent),
        PairKind::Dropped => ("−", t.danger),
    };
    let side = |c: &Option<Commit>| match c {
        Some(c) => format!("{} {}", short(&c.hash), c.summary),
        None => "none".to_owned(),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(mark).font(theme::semibold(13.0)).color(color));
        ui.label(
            RichText::new(format!("{}  →  {}", side(&pair.old), side(&pair.new)))
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
        PairKind::Added => notes.push("only in the new version"),
        PairKind::Dropped => notes.push("dropped, or squashed into another"),
        PairKind::Same => notes.push("same change"),
        PairKind::Changed => notes.push("the change differs"),
    }
    if pair.old.is_some() && pair.new.is_some() {
        notes.push(if pair.same_tree() {
            "same files"
        } else {
            "files differ (the base moved, or the change did)"
        });
    }
    if pair.message_changed() {
        notes.push("message changed");
    }
    if pair.author_changed() {
        notes.push("author changed");
    }
    match (&pair.old, &pair.new) {
        (Some(o), Some(n)) if o.signed && n.signed => notes.push("signature replaced"),
        (Some(o), Some(_)) if o.signed => notes.push("signature dropped"),
        (Some(o), Some(n)) if !o.signed && n.signed => notes.push("now signed"),
        _ => {}
    }
    notes
}
