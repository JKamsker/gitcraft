//! The commit's signature, containment, notes and backlinks folded into one compact _More
//! details_ row (#213): what they found in a few words on it, the four sections inside when
//! opened. A signature that needs attention stays visible above it while folded, and opens it.

use egui::{RichText, Sense, Ui};

use super::section::{self, Summary};
use super::{Extras, ExtrasCmd, backlinks_view, contained_view, notes_view, signature_view};
use crate::git::{Signature, SignatureVerdict};
use crate::theme::Tokens;

pub(super) fn show(
    extras: &mut Extras,
    ui: &mut Ui,
    remotes: &[String],
    idle: bool,
    cmds: &mut Vec<ExtrasCmd>,
) {
    let t = Tokens::get(ui.ctx());
    let title = crate::i18n::text("commit-details-more");
    if let Some(Ok(sig)) = &extras.signature
        && needs_attention(&sig.verdict)
    {
        signature_warning(ui, sig, &t);
    }
    let summary = match glance(extras) {
        Some(text) => Summary::Text(text, t.text_dim),
        None => Summary::Loading,
    };
    section::section(ui, title, summary, false, None, |ui| {
        signature_view::section(ui, extras.signature.as_ref(), cmds);
        contained_view::section(ui, extras.contained.as_ref(), cmds);
        notes_view::section(ui, &mut extras.notes, remotes, idle, cmds);
        backlinks_view::section(ui, extras.backlinks.as_ref(), cmds);
    });
}

/// A verdict worth seeing without opening anything: not good and trusted, and not simply
/// unsigned.
pub(super) fn needs_attention(verdict: &SignatureVerdict) -> bool {
    !matches!(
        verdict,
        SignatureVerdict::Valid { trusted: true } | SignatureVerdict::Unsigned
    )
}

/// "Signature: Unknown key · GPG", in the verdict's colour; clicked, opens the signature.
fn signature_warning(ui: &mut Ui, sig: &Signature, t: &Tokens) {
    let color = signature_view::verdict_color(t, &sig.verdict);
    let text = crate::i18n::message(
        "commit-details-signature-warning",
        &[("headline", signature_view::headline(sig).into())],
    );
    ui.add_space(4.0);
    let row = ui.horizontal(|ui| {
        crate::icons::inline(ui, "circle-alert", 12.0, color);
        ui.add(egui::Label::new(RichText::new(&text).color(color).small()).truncate());
    });
    let hit = ui.interact(
        row.response.rect,
        ui.id().with("commit-signature-warning"),
        Sense::click(),
    );
    crate::a11y::a11y(&hit, egui::WidgetType::Button, &text);
    let hit = hit.on_hover_text(sig.verdict.explanation());
    if hit.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if hit.clicked() {
        let more = egui::Id::new((
            "commit-extras-open",
            crate::i18n::text("commit-details-more"),
        ));
        let signature = egui::Id::new(("commit-extras-open", crate::i18n::text("repo-core-1136")));
        ui.data_mut(|d| {
            d.insert_persisted(more, true);
            d.insert_persisted(signature, true);
        });
    }
}

/// What the four found, in a few words: the signature's verdict, where the commit is, that it
/// has a note, its backlinks. `None` until each has been read.
pub(super) fn glance(extras: &Extras) -> Option<String> {
    let mut parts = Vec::new();
    match extras.signature.as_ref()? {
        Ok(sig) if sig.verdict != SignatureVerdict::Unsigned => {
            parts.push(sig.verdict.label().to_owned());
        }
        _ => {}
    }
    if let Ok(c) = extras.contained.as_ref()? {
        parts.push(contained_view::headline(c));
    }
    if let Ok(Some(_)) = extras.notes.note.as_ref()? {
        parts.push(crate::i18n::text("repo-core-1104").to_owned());
    }
    if let Ok((links, more)) = extras.backlinks.as_ref()?
        && !links.is_empty()
    {
        parts.push(backlinks_view::headline(links, *more));
    }
    Some(parts.join(" · "))
}

#[cfg(test)]
#[path = "more_tests.rs"]
mod tests;
