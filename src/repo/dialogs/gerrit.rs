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
                ("min", head[..head.len().min(8)].to_string().into()),
                ("remote", remote.to_string().into()),
            ],
        ),
        &t,
    );
    if let Some(warning) = web_warning {
        ui.label(egui::RichText::new(crate::gerrit::display_error(warning)).color(t.warning));
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
            label: "Push for review".to_owned(),
            ops: plan.unwrap_or_default(),
            refresh: Refresh::All,
        },
    )
}

#[cfg(test)]
mod warning_presentation_tests {
    use super::*;
    use crate::repo::push_preflight::core_guard_paint_tests::paint;

    #[test]
    fn actual_review_warning_paint_repaints_without_changing_raw_server_or_plan()
    -> Result<(), String> {
        let raw = "Invalid gitcraft.gerritUrl: use an HTTP(S) URL without credentials, query or fragment. Using the remote's web address; Gerrit review submission remains enabled.";
        let server = crate::gerrit::Server {
            remote: "origin".into(),
            web: "https://review.invalid".into(),
            project: "opaque $(project)".into(),
            web_warning: Some(raw.into()),
        };
        let original = server.clone();
        let hash = "a".repeat(40);
        let targets = vec![("main".to_owned(), hash.clone())];
        let mut target = "main".to_owned();
        let mut topic = "opaque,submit%ready=yes".to_owned();
        let plan = crate::gerrit::review_ops(&server.remote, &target, &topic, &hash, &hash)?;
        let ctx = egui::Context::default();
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            let shown = paint(&ctx, |ui| {
                assert!(matches!(
                    review(
                        ui,
                        &server.remote,
                        &hash,
                        &targets,
                        &mut target,
                        &mut topic,
                        false,
                        server.web_warning.as_deref()
                    ),
                    Outcome::Open
                ));
            });
            let expected = server
                .display_web_warning()
                .ok_or("expected retained warning")?;
            assert!(shown.contains(&expected), "{shown}");
            if locale == "de" {
                assert_ne!(expected, raw);
            } else {
                assert_eq!(expected, raw);
            }
            assert_eq!(server, original);
            assert_eq!(
                crate::gerrit::review_ops(&server.remote, &target, &topic, &hash, &hash)?,
                plan
            );
        }
        Ok(())
    }
}
