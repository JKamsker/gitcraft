//! The Signature section: the verdict, who signed, with which key, and git's output.

use egui::{Color32, RichText, Ui};

use super::ExtrasCmd;
use super::section::{self, Summary};
use crate::git::{Signature, SignatureVerdict};
use crate::theme::{self, Tokens};

pub(super) fn section(
    ui: &mut Ui,
    signature: Option<&Result<Signature, String>>,
    cmds: &mut Vec<ExtrasCmd>,
) {
    let t = Tokens::get(ui.ctx());
    let summary = match signature {
        None => Summary::Loading,
        Some(Err(_)) => Summary::Text(crate::i18n::text("repo-core-1134").to_owned(), t.danger),
        Some(Ok(sig)) => Summary::Text(headline(sig), verdict_color(&t, &sig.verdict)),
    };
    let mut again = false;
    // Anything but unsigned can turn out otherwise: a key imported, allowed signers set up.
    let checkable = match signature {
        Some(Ok(sig)) => sig.verdict != SignatureVerdict::Unsigned,
        Some(Err(_)) => true,
        None => false,
    };
    let refresh = checkable.then_some((crate::i18n::text("repo-core-1135"), &mut again));
    section::section(
        ui,
        crate::i18n::text("repo-core-1136"),
        summary,
        false,
        refresh,
        |ui| match signature {
            None => section::faint(ui, crate::i18n::text("repo-core-1137")),
            Some(Err(e)) => section::error(ui, e),
            Some(Ok(sig)) => body(ui, sig),
        },
    );
    if again {
        cmds.push(ExtrasCmd::CheckSignature);
    }
}

/// "Valid · SSH · Zoë": the verdict, the kind of key, the signer.
fn headline(sig: &Signature) -> String {
    let mut parts = vec![sig.verdict.label().to_owned()];
    if let Some(format) = sig.format {
        parts.push(format.label().to_owned());
    }
    if !sig.signer.is_empty() {
        parts.push(sig.signer.clone());
    }
    parts.join(" · ")
}

fn verdict_color(t: &Tokens, verdict: &SignatureVerdict) -> Color32 {
    match verdict {
        SignatureVerdict::Valid { trusted: true } => t.success,
        SignatureVerdict::Valid { trusted: false }
        | SignatureVerdict::Expired { .. }
        | SignatureVerdict::UnknownKey
        | SignatureVerdict::NotChecked => t.warning,
        SignatureVerdict::Revoked | SignatureVerdict::Invalid => t.danger,
        SignatureVerdict::Unsigned => t.text_dim,
    }
}

fn body(ui: &mut Ui, sig: &Signature) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(sig.verdict.explanation()).color(t.text_dim)).wrap());
    if sig.verdict == SignatureVerdict::Unsigned {
        return;
    }
    egui::Grid::new("commit_signature")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            let row = |ui: &mut Ui, name: &str, value: &str, mono: bool| {
                if value.is_empty() {
                    return;
                }
                ui.label(RichText::new(name).color(t.text_faint));
                let text = RichText::new(value);
                let text = if mono {
                    text.font(theme::mono(11.5))
                } else {
                    text
                };
                ui.add(egui::Label::new(text).wrap().selectable(true));
                ui.end_row();
            };
            if let Some(format) = sig.format {
                row(ui, crate::i18n::text("repo-core-1139"), format.label(), false);
            }
            row(ui, crate::i18n::text("repo-core-1140"), &sig.signer, false);
            row(ui, crate::i18n::text("repo-core-1141"), &sig.key, true);
            if sig.primary_key != sig.key {
                row(ui, crate::i18n::text("repo-core-1142"), &sig.primary_key, true);
            }
        });
    if sig.raw.trim().is_empty() {
        if sig.verdict == SignatureVerdict::NotChecked {
            section::faint(ui, crate::i18n::text("repo-core-1143"));
        }
    } else {
        section::output_box(ui, crate::i18n::text("repo-core-1144"), sig.raw.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::SignatureFormat;

    #[test]
    fn the_headline_names_the_verdict_kind_and_signer() {
        let mut sig = Signature {
            verdict: SignatureVerdict::Valid { trusted: true },
            format: Some(SignatureFormat::Ssh),
            signer: "Zoë <z@example.org>".to_owned(),
            key: "SHA256:k".to_owned(),
            primary_key: String::new(),
            raw: String::new(),
        };
        assert_eq!(headline(&sig), "Valid · SSH · Zoë <z@example.org>");
        sig.signer.clear();
        sig.verdict = SignatureVerdict::UnknownKey;
        assert_eq!(headline(&sig), "Unknown key · SSH");
        sig.format = None;
        sig.verdict = SignatureVerdict::Unsigned;
        assert_eq!(headline(&sig), "Unsigned");
    }
}
