//! The toolbar's quality-gate chip and its popup: details, refresh, dashboard and binding.

use egui::{Color32, RichText, Ui};

use super::binding::Binding;
use super::{Answer, Badge, RepoTab};
use crate::analysis::Target;
use crate::analysis::gate::{ServerGate, Stale, Unavailable, Verdict};
use crate::theme::Tokens;

fn t(id: &'static str) -> &'static str {
    crate::i18n::text(id)
}

fn word(badge: &Badge) -> &'static str {
    t(match badge {
        Badge::Unbound => "analysis-gate-unbound",
        Badge::NoConnection | Badge::NoTarget | Badge::Failed(_) => "analysis-gate-unavailable",
        Badge::Checking => "analysis-gate-checking",
        Badge::Verdict(Verdict::Passed) => "analysis-gate-passed",
        Badge::Verdict(Verdict::Failed) => "analysis-gate-failed",
        Badge::Verdict(Verdict::Pending) => "analysis-gate-pending",
        Badge::Verdict(Verdict::Unavailable(_)) => "analysis-gate-unavailable",
        Badge::Verdict(Verdict::Stale(..)) => "analysis-gate-stale",
    })
}

fn color(tokens: &Tokens, badge: &Badge) -> Color32 {
    match badge {
        Badge::Verdict(Verdict::Passed) => tokens.success,
        Badge::Verdict(Verdict::Failed) => tokens.danger,
        Badge::Verdict(Verdict::Pending | Verdict::Stale(..)) => tokens.warning,
        _ => tokens.text_faint,
    }
}

/// The sentence explaining a badge that is not a plain pass or fail.
fn detail(badge: &Badge) -> Option<String> {
    let id = match badge {
        Badge::Unbound => "analysis-gate-detail-unbound",
        Badge::NoConnection => "analysis-gate-detail-no-connection",
        Badge::NoTarget => "analysis-gate-detail-no-target",
        Badge::Checking => return None,
        Badge::Failed(failure) => return Some(failure.text()),
        Badge::Verdict(Verdict::Passed | Verdict::Failed) => return None,
        Badge::Verdict(Verdict::Pending) => "analysis-gate-detail-pending",
        Badge::Verdict(Verdict::Unavailable(Unavailable::NotAnalyzed)) => {
            "analysis-gate-detail-not-analyzed"
        }
        Badge::Verdict(Verdict::Unavailable(Unavailable::NoGate)) => "analysis-gate-detail-no-gate",
        Badge::Verdict(Verdict::Unavailable(Unavailable::Unrecognized)) => {
            "analysis-gate-detail-unrecognized"
        }
        Badge::Verdict(Verdict::Stale(stale, gate)) => return Some(stale_text(stale, *gate)),
    };
    Some(t(id).to_owned())
}

fn stale_text(stale: &Stale, gate: ServerGate) -> String {
    let reason = match stale {
        Stale::RevisionUnknown => t("analysis-stale-revision-unknown").to_owned(),
        Stale::HeadUnknown => t("analysis-stale-head-unknown").to_owned(),
        Stale::TaskFailed => t("analysis-stale-task-failed").to_owned(),
        Stale::Mismatch { analyzed, open } => crate::i18n::message(
            "analysis-stale-mismatch",
            &[
                ("analyzed", super::super::short(analyzed).to_owned().into()),
                ("open", super::super::short(open).to_owned().into()),
            ],
        ),
    };
    let reported = t(match gate {
        ServerGate::Ok => "analysis-gate-passed",
        ServerGate::Error | ServerGate::Warn => "analysis-gate-failed",
        ServerGate::None | ServerGate::Unknown => "analysis-gate-unavailable",
    });
    crate::i18n::message(
        "analysis-stale-reported",
        &[("reason", reason.into()), ("reported", reported.into())],
    )
}

/// `2016-12-12T17:12:45+0100` → the localized date, the time and the server's offset.
fn when(raw: &str) -> String {
    match (raw.get(..10), raw.get(11..16), raw.get(19..)) {
        (Some(day), Some(time), Some(offset)) => {
            format!("{} {time} {offset}", crate::i18n::date_iso(day))
        }
        _ => raw.to_owned(),
    }
}

impl RepoTab {
    /// The chip after the branch in the toolbar; hidden without any Sonar configuration.
    pub(in crate::repo) fn quality_gate_chip(&mut self, ui: &mut Ui) {
        self.poll_quality_gate(ui.ctx(), false);
        let Some(badge) = self.quality_gate_badge(ui.ctx()) else {
            return;
        };
        let tokens = Tokens::get(ui.ctx());
        let label = RichText::new(crate::i18n::message(
            "analysis-gate-chip",
            &[("state", word(&badge).into())],
        ))
        .small()
        .color(color(&tokens, &badge));
        let button = ui
            .add(egui::Button::new(label).frame(false))
            .on_hover_text(detail(&badge).unwrap_or_else(|| word(&badge).to_owned()));
        let mut refresh = false;
        egui::Popup::menu(&button)
            .width(420.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                refresh = self.gate_details(ui, &badge);
                ui.separator();
                self.binding_editor(ui);
            });
        if refresh {
            self.poll_quality_gate(ui.ctx(), true);
        }
    }

    /// Returns whether a refresh was asked for.
    fn gate_details(&self, ui: &mut Ui, badge: &Badge) -> bool {
        ui.strong(word(badge));
        if let Some(text) = detail(badge) {
            ui.label(text);
        }
        let answer = self
            .gate_context(ui.ctx())
            .ok()
            .and_then(|(_, q, c)| self.quality_gate.current(&q, &c).cloned());
        if let Some(answer) = &answer {
            self.answer_details(ui, answer);
        }
        let mut refresh = false;
        ui.horizontal(|ui| {
            if let Some(Answer { result: Ok(o), .. }) = &answer
                && ui.button(t("analysis-gate-open-dashboard")).clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(&o.dashboard));
            }
            let busy = self.quality_gate.loading.is_some();
            if matches!(
                badge,
                Badge::Checking | Badge::Failed(_) | Badge::Verdict(_)
            ) && ui
                .add_enabled(!busy, egui::Button::new(t("analysis-gate-refresh")))
                .clicked()
            {
                refresh = true;
            }
        });
        refresh
    }

    fn answer_details(&self, ui: &mut Ui, answer: &Answer) {
        let target = match &answer.query.target {
            Target::Branch(name) => crate::i18n::message(
                "analysis-gate-target-branch",
                &[("name", name.clone().into())],
            ),
            Target::PullRequest(key) => {
                crate::i18n::message("analysis-gate-target-pr", &[("key", key.clone().into())])
            }
        };
        let faint = Tokens::get(ui.ctx()).text_faint;
        let line = |ui: &mut Ui, text: String| {
            ui.label(RichText::new(text).small().color(faint));
        };
        line(
            ui,
            crate::i18n::message(
                "analysis-gate-project",
                &[
                    ("project", answer.query.project.clone().into()),
                    ("target", target.into()),
                ],
            ),
        );
        if let Ok(observation) = &answer.result
            && let Some(analysis) = &observation.analysis
        {
            let date = analysis.date.as_deref().map(when).unwrap_or_default();
            let revision = analysis
                .revision
                .as_deref()
                .map(|r| super::super::short(r).to_owned())
                .unwrap_or_else(|| t("analysis-gate-revision-unknown").to_owned());
            line(
                ui,
                crate::i18n::message(
                    "analysis-gate-analysis",
                    &[("date", date.into()), ("revision", revision.into())],
                ),
            );
            for metric in observation.failing.iter().take(10) {
                line(ui, format!("• {metric}"));
            }
        }
        line(
            ui,
            crate::i18n::message(
                "analysis-gate-open-commit",
                &[(
                    "commit",
                    super::super::short(&self.refs.head).to_owned().into(),
                )],
            ),
        );
        line(
            ui,
            crate::i18n::message(
                "analysis-gate-refreshed",
                &[("minutes", (answer.at.elapsed().as_secs() / 60).into())],
            ),
        );
    }

    /// Edits the repository's binding; saved only when valid.
    fn binding_editor(&mut self, ui: &mut Ui) {
        let account = crate::connections::current().choices.sonar;
        if !account.configured() {
            ui.label(t("analysis-gate-detail-no-connection"));
        }
        let id = egui::Id::new(("sonar-binding-editor", &self.path));
        let saved = Binding::load(ui.ctx(), &self.path);
        let mut draft = ui
            .ctx()
            .data(|d| d.get_temp::<Binding>(id))
            .or_else(|| saved.clone())
            .unwrap_or_default();
        ui.label(t("analysis-binding-title"));
        ui.horizontal(|ui| {
            ui.label(t("analysis-binding-project"));
            ui.text_edit_singleline(&mut draft.project);
        });
        ui.horizontal(|ui| {
            ui.label(t("analysis-binding-branches"));
            ui.add(egui::TextEdit::singleline(&mut draft.branches).hint_text("master=main"));
        });
        ui.checkbox(
            &mut draft.pull_requests,
            t("analysis-binding-pull-requests"),
        );
        let valid = draft.validate();
        if let Err(message) = valid {
            ui.label(RichText::new(t(message)).small());
        }
        let mut keep = true;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    valid.is_ok() && account.configured(),
                    egui::Button::new(t("analysis-binding-save")),
                )
                .clicked()
            {
                draft.connection = account.identity();
                draft.project = draft.project.trim().to_owned();
                Binding::store(ui.ctx(), &self.path, Some(&draft));
                self.quality_gate.next = None;
                keep = false;
            }
            if saved.is_some() && ui.button(t("analysis-binding-remove")).clicked() {
                Binding::store(ui.ctx(), &self.path, None);
                self.quality_gate = super::State::default();
                keep = false;
            }
        });
        ui.ctx().data_mut(|d| {
            if keep {
                d.insert_temp(id, draft);
            } else {
                d.remove::<Binding>(id);
            }
        });
    }
}
