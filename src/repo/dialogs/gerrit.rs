//! Review submission selects a fetched target and previews the exact object being pushed.

use super::{Outcome, buttons, title};
use crate::repo::{Action, Refresh};
use crate::theme::Tokens;

#[allow(clippy::too_many_arguments)]
pub(super) fn review(
    ui: &mut egui::Ui,
    remote: &str,
    head: &str,
    targets: &[(String, String)],
    target: &mut String,
    topic: &mut String,
    skip_hooks: bool,
    web_warning: Option<&str>,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-0031"),
        &crate::i18n::message(
            "repo-core-1328",
            &[
                ("min", format!("{}", &head[..head.len().min(8)]).into()),
                ("remote", format!("{}", remote).into()),
            ],
        ),
        &t,
    );
    if let Some(warning) = web_warning {
        ui.label(egui::RichText::new(warning).color(t.warning));
    }
    ui.label(crate::i18n::text("repo-core-1329"));
    egui::ComboBox::from_id_salt("gerrit-target")
        .width(ui.available_width())
        .selected_text(target.as_str())
        .show_ui(ui, |ui| {
            for (name, _) in targets {
                ui.selectable_value(target, name.clone(), name);
            }
        });
    ui.add_space(6.0);
    ui.label(crate::i18n::text("repo-core-1331"));
    ui.add(
        egui::TextEdit::singleline(topic)
            .desired_width(f32::INFINITY)
            .hint_text(crate::i18n::text("repo-core-1332")),
    );
    let base = targets
        .iter()
        .find(|(name, _)| name == target)
        .map(|(_, sha)| sha.as_str())
        .unwrap_or_default();
    let mut plan = crate::gerrit::review_ops(remote, target, topic, head, base);
    ui.add_space(6.0);
    if targets.is_empty() {
        ui.label(egui::RichText::new(crate::i18n::text("repo-core-1333")).color(t.warning));
    } else {
        ui.label(format!("{remote}: {head} → refs/for/{target}"));
        ui.label(egui::RichText::new(crate::i18n::text("repo-core-1335")).color(t.text_dim));
    }
    if skip_hooks {
        ui.label(crate::i18n::text("repo-core-1336"));
        if let Ok(steps) = &mut plan
            && let Some(push) = steps.last_mut()
        {
            push.insert(1, "--no-verify".to_owned());
        }
    }
    if !crate::gerrit::valid_topic(topic) {
        ui.label(egui::RichText::new(crate::i18n::text("repo-core-1337")).color(t.warning));
    }
    ui.add_space(10.0);
    // A deliberate click is needed; Enter in the topic field must not send a review.
    buttons(
        ui,
        crate::i18n::text("repo-core-0031"),
        plan.is_ok(),
        false,
        || Action::Ops {
            label: crate::i18n::text("repo-core-0031").to_owned(),
            ops: plan.unwrap_or_default(),
            refresh: Refresh::All,
        },
    )
}
