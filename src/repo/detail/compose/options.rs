//! The commit box's options: which prefix picker it offers, and the patterns that turn issue
//! references in messages into links.

use egui::{RichText, Ui};

use super::issue_links;
use super::store::{IssueRule, PrefixStyle, Store};
use crate::icons;
use crate::repo::RepoTab;
use crate::theme::Tokens;

/// The rule being added, and why it was refused.
#[derive(Clone, Debug, Default)]
pub(super) struct RuleDraft {
    rule: IssueRule,
    error: Option<String>,
}

impl RepoTab {
    pub(super) fn options_button(&mut self, ui: &mut Ui) {
        let b = icons::small_button(ui, "settings", crate::i18n::text("repo-core-0966"));
        let ctx = ui.ctx().clone();
        let draft = &mut self.compose.rule;
        let forge = self.hosting.as_ref().map(issue_links::forge_rule);
        egui::Popup::menu(&b)
            .width(340.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                // Read only while it shows, not on every frame.
                let mut store = Store::load(&ctx);
                let before = store.clone();
                let t = Tokens::get(ui.ctx());
                ui.label(
                    RichText::new(crate::i18n::text("repo-core-0967"))
                        .color(t.text_faint)
                        .small(),
                );
                ui.horizontal(|ui| {
                    for style in PrefixStyle::ALL {
                        ui.radio_value(&mut store.style, style, style.label());
                    }
                });
                ui.separator();
                let mut run_hook = !store.no_hook_prefill;
                ui.checkbox(&mut run_hook, crate::i18n::text("repo-core-0968"))
                    .on_hover_text(
                        crate::i18n::text("repo-core-0969"),
                    );
                store.no_hook_prefill = !run_hook;
                ui.separator();
                rules_ui(ui, &mut store.issue_rules, draft, forge.as_ref());
                if store != before {
                    store.save(&ctx);
                }
            });
    }
}

/// The issue link rules, each with ×, and a form to add one.
fn rules_ui(
    ui: &mut Ui,
    rules: &mut Vec<IssueRule>,
    draft: &mut RuleDraft,
    forge: Option<&IssueRule>,
) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(crate::i18n::text("repo-core-0970")).color(t.text_faint).small());
    let forge_note = match forge {
        Some(f) => crate::i18n::message("repo-core-0971", &[("replace", format!("{}", f.url.replace("$1", "12")).into())]),
        None => crate::i18n::text("repo-core-0972").to_owned(),
    };
    ui.label(RichText::new(forge_note).color(t.text_dim).small());
    let mut remove = None;
    for (i, rule) in rules.iter().enumerate() {
        ui.horizontal(|ui| {
            if icons::small_button(ui, "x", crate::i18n::text("repo-core-0973")).clicked() {
                remove = Some(i);
            }
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{}  →  {}", rule.pattern, rule.url))
                        .monospace()
                        .small(),
                )
                .truncate(),
            );
        });
    }
    if let Some(i) = remove {
        rules.remove(i);
    }
    let pattern = ui.add(
        egui::TextEdit::singleline(&mut draft.rule.pattern)
            .hint_text(crate::i18n::text("repo-core-0974"))
            .desired_width(f32::INFINITY),
    );
    let url = ui.add(
        egui::TextEdit::singleline(&mut draft.rule.url)
            .hint_text(crate::i18n::text("repo-core-0975"))
            .desired_width(f32::INFINITY),
    );
    if pattern.changed() || url.changed() {
        draft.error = None;
    }
    ui.horizontal(|ui| {
        if ui.button(crate::i18n::text("repo-core-0976")).clicked() {
            match add_rule(rules, &draft.rule) {
                Ok(()) => *draft = RuleDraft::default(),
                Err(e) => draft.error = Some(e),
            }
        }
        ui.label(
            RichText::new(crate::i18n::text("repo-core-0977"))
                .color(t.text_faint)
                .small(),
        );
    });
    if let Some(e) = &draft.error {
        ui.label(RichText::new(e).color(t.danger).small());
    }
}

/// The most rules kept.
const MAX_RULES: usize = 20;

/// Adds `rule` to `rules`, or says why it can't be.
pub fn add_rule(rules: &mut Vec<IssueRule>, rule: &IssueRule) -> Result<(), String> {
    let rule = IssueRule {
        pattern: rule.pattern.trim().to_owned(),
        url: rule.url.trim().to_owned(),
    };
    issue_links::check(&rule)?;
    if rules.contains(&rule) {
        return Err(crate::i18n::text("repo-core-0978").to_owned());
    }
    if rules.len() >= MAX_RULES {
        return Err(crate::i18n::message("repo-core-0979", &[("MAX-RULES", format!("{}", MAX_RULES).into())]));
    }
    rules.push(rule);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_checked_before_they_are_added() {
        let mut rules = Vec::new();
        let rule = |p: &str, u: &str| IssueRule {
            pattern: p.into(),
            url: u.into(),
        };
        assert!(add_rule(&mut rules, &rule(" JIRA-\\d+ ", " https://j.example/$0 ")).is_ok());
        assert_eq!(rules[0].pattern, "JIRA-\\d+");
        assert!(add_rule(&mut rules, &rule("JIRA-\\d+", "https://j.example/$0")).is_err());
        assert!(add_rule(&mut rules, &rule("[", "https://j.example/$0")).is_err());
        assert!(add_rule(&mut rules, &rule("x", "ftp://j.example/$0")).is_err());
        for i in 1..MAX_RULES {
            assert!(add_rule(&mut rules, &rule(&format!("X{i}"), "https://x/$0")).is_ok());
        }
        assert!(add_rule(&mut rules, &rule("Y", "https://x/$0")).is_err());
    }
}
