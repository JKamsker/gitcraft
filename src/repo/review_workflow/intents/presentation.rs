//! Presentation responsibilities for intents.
use super::*;

pub(in crate::repo::review_workflow) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    remotes: &[git::Remote],
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-intent-cards-b73148",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-advisory-notes-travel-through-refs-gitcraft-in-05b884",
    ));
    let previous_remote = s.remote.clone();
    egui::ComboBox::from_id_salt("intent-remote")
        .selected_text(if s.remote.is_empty() {
            crate::i18n::text("repo-workflows-review-label-choose-sharing-remote-a61f48")
        } else {
            &s.remote
        })
        .show_ui(ui, |ui| {
            for r in remotes {
                ui.selectable_value(&mut s.remote, r.name.clone(), &r.name);
            }
        });
    if previous_remote != s.remote {
        s.selection_changed();
    }
    ui.horizontal(|ui| {
        for (label, act) in [
            (
                crate::i18n::text("repo-workflows-review-label-read-cached-intents-816c85"),
                Act::Read,
            ),
            (
                crate::i18n::text("repo-workflows-review-label-fetch-teammate-intents-313dc0"),
                Act::Sync(s.remote.clone()),
            ),
            (
                crate::i18n::text("repo-workflows-review-label-publish-my-intents-cd3137"),
                Act::Publish(s.remote.clone()),
            ),
        ] {
            if ui.add_enabled(!s.busy, egui::Button::new(label)).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Intent(act)));
            }
        }
    });
    if let Some(e) = &s.error {
        ui.label(crate::repo::messages::display_status(e));
    }
    if network::refused().is_some() {
        ui.label(crate::i18n::text(
            "repo-workflows-review-label-offline-cached-observations-only-e0eb0f",
        ));
    } else if s.synced == 0 || now().saturating_sub(s.synced) > 120 {
        ui.label(crate::i18n::text(
            "repo-workflows-review-label-stale-shared-refs-have-not-been-synchronized-r-f7babb",
        ));
    }
    if !s.quarantine.is_empty() {
        egui::CollapsingHeader::new(crate::i18n::message(
            "repo-workflows-review-message-count-one-one-invalid-oversize-intent-record-q-0432a3",
            &[("count", (s.quarantine.len()).into())],
        ))
        .id_salt("review-intent-quarantine")
        .show(ui, |ui| {
            for warning in &s.quarantine {
                ui.label(crate::repo::messages::display_intent_warning(warning));
            }
        });
    }
    for seen in &s.rows {
        ui.group(|ui| {
            ui.label(format!(
                "{} · {} {} · {}",
                seen.card.author, seen.card.path, seen.card.symbol, seen.card.message
            ));
            let left = seen.card.expires.saturating_sub(now());
            ui.weak(if left == 0 {
                crate::i18n::text("repo-workflows-review-label-expired-ended-6f52b4").into()
            } else {
                crate::i18n::message("repo-workflows-review-message-minutes-one-expires-in-one-minute-other-expire-287948", &[("minutes", (left.div_ceil(60)).into())])
            });
            if seen.local && left > 0 && ui.button(crate::i18n::text("repo-workflows-review-label-end-this-intent-c9706e")).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Intent(Act::Expire(
                    seen.reference.clone(),
                ))));
            }
        });
    }
    ui.add(
        egui::TextEdit::singleline(&mut s.path).hint_text(crate::i18n::text(
            "repo-workflows-review-label-repository-relative-path-or-folder-f7f7ab",
        )),
    );
    ui.add(
        egui::TextEdit::singleline(&mut s.symbol).hint_text(crate::i18n::text(
            "repo-workflows-review-label-symbol-optional-e114d2",
        )),
    );
    ui.add(
        egui::TextEdit::singleline(&mut s.message).hint_text(crate::i18n::text(
            "repo-workflows-review-label-changing-token-lifetime-187d31",
        )),
    );
    if s.minutes == 0 {
        s.minutes = 60;
    }
    ui.add(
        egui::Slider::new(&mut s.minutes, 1..=1440).text(crate::i18n::text(
            "repo-workflows-review-label-minutes-4f846a",
        )),
    );
    if ui
        .add_enabled(
            !s.busy,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-review-label-post-local-intent-publish-when-ready-ee6bdb",
            )),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Intent(Act::Post {
            path: s.path.clone(),
            symbol: s.symbol.clone(),
            message: s.message.clone(),
            minutes: s.minutes,
        })));
    }
}
