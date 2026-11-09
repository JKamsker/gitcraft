//! The Activity panel: the tab's operations, newest first, each unfolding into its git
//! commands, how each ended and all it printed, to read or copy.

use egui::{RichText, Ui};

use super::{Entry, Outcome, exit_text, outcome_text, seconds, short_command_line};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How tall one command's output gets before it scrolls.
const OUTPUT_HEIGHT: f32 = 220.0;

impl RepoTab {
    /// The panel's contents: a header (count, Clear, close), then the operations.
    pub(in crate::repo) fn activity_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(crate::i18n::text(
                    "repo-workflows-activity-activity-panel-38da1505",
                ))
                .font(theme::semibold(12.5)),
            );
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-activity-count",
                    &[
                        ("count", (self.activity.len()).into()),
                        (
                            "running",
                            (if self.busy_op.is_some() {
                                crate::i18n::text("repo-workflows-activity-running-extra")
                            } else {
                                ""
                            })
                            .into(),
                        ),
                    ],
                ))
                .color(t.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::small_button(
                    ui,
                    "x",
                    crate::i18n::text("repo-workflows-activity-activity-panel-6e279c58"),
                )
                .clicked()
                {
                    acts.push(Action::ToggleActivity);
                }
                if ui
                    .add_enabled(
                        self.activity.len() > 0,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-activity-activity-panel-83b12c22",
                        )),
                    )
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-activity-activity-panel-3756b124",
                    ))
                    .clicked()
                {
                    self.activity.clear();
                }
            });
        });
        ui.separator();
        let reveal = self.activity.reveal.take();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.activity.len() == 0 && self.busy_op.is_none() {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-activity-activity-panel-0a4cf721",
                        ))
                        .color(t.text_dim),
                    );
                }
                self.running_entry(ui, &t);
                for entry in self.activity.entries().rev() {
                    entry_ui(ui, entry, reveal == Some(entry.id), &t);
                }
            });
    }

    /// The operation running, with the commands it ran so far.
    fn running_entry(&self, ui: &mut Ui, t: &Tokens) {
        let (Some(op), Some(running)) = (&self.busy_op, &self.running) else {
            return;
        };
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(
                RichText::new(crate::repo::messages::display_operation(op))
                    .font(theme::medium(12.5)),
            );
            if let Some(p) = running.control.progress() {
                ui.label(RichText::new(p.label()).color(t.text_dim));
            }
        });
        for (args, code) in running.control.step_summaries() {
            ui.label(
                RichText::new(format!(
                    "$ {}  · {}",
                    short_command_line(&args),
                    exit_text(code)
                ))
                .font(theme::mono(11.5))
                .color(t.text_dim),
            );
        }
        ui.add_space(4.0);
    }
}

/// One operation: a line saying what, how it ended and how long it took, unfolding into its
/// commands.
fn entry_ui(ui: &mut Ui, entry: &Entry, reveal: bool, t: &Tokens) {
    let (icon, color) = match entry.outcome {
        Outcome::Done => ("check", t.success),
        Outcome::Failed(_) => ("circle-alert", t.danger),
        Outcome::Cancelled => ("x", t.text_dim),
    };
    let id = ui.id().with(("activity-entry", entry.id));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    if reveal {
        state.set_open(true);
    }
    let header = state.show_header(ui, |ui| {
        icons::inline(ui, icon, 13.0, color);
        ui.label(
            RichText::new(crate::repo::messages::display_operation(&entry.label))
                .font(theme::medium(12.5)),
        );
        let when = crate::i18n::message(
            "repo-workflows-activity-entry-summary",
            &[
                ("outcome", (outcome_text(&entry.outcome)).into()),
                ("duration", (seconds(entry.took)).into()),
                ("age", (ago(entry.at.elapsed())).into()),
                (
                    "background",
                    (if entry.background { "yes" } else { "no" }).into(),
                ),
                ("repeats", (entry.repeats).into()),
            ],
        );
        ui.label(RichText::new(when).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(
                ui,
                "copy",
                crate::i18n::text("repo-workflows-activity-entry-ui-332e9c9d"),
            )
            .clicked()
            {
                ui.ctx().copy_text(entry.as_text());
            }
        });
    });
    let (_, header_resp, _) = header.body(|ui| {
        if let Outcome::Failed(why) = &entry.outcome {
            ui.label(RichText::new(entry.display_error(why)).color(t.danger));
        }
        if entry.steps.is_empty() {
            ui.label(
                RichText::new(crate::i18n::text(
                    "repo-workflows-activity-entry-ui-599c9e61",
                ))
                .color(t.text_dim),
            );
        }
        for (i, (step, line)) in entry.steps.iter().zip(&entry.lines).enumerate() {
            ui.label(
                RichText::new(format!(
                    "$ {line}  · {} · {}",
                    exit_text(step.code),
                    seconds(step.took)
                ))
                .font(theme::mono(11.5)),
            );
            if !step.output.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt(("activity-output", entry.id, i))
                    .max_height(OUTPUT_HEIGHT)
                    .show(ui, |ui| {
                        // Read-only, but selectable and copyable.
                        let mut text = step.output.as_str();
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .font(theme::mono(11.5))
                                .desired_width(f32::INFINITY)
                                .desired_rows(1),
                        );
                    });
            }
        }
    });
    if reveal {
        header_resp.response.scroll_to_me(Some(egui::Align::TOP));
    }
}

/// "12 s", "3 min", "2 h".
fn ago(d: std::time::Duration) -> String {
    match d.as_secs() {
        s @ 0..60 => {
            crate::i18n::message("repo-workflows-activity-ago-d2916a22", &[("s", s.into())])
        }
        s @ 60..3600 => crate::i18n::message(
            "repo-workflows-activity-ago-e823f861",
            &[("s-60-1", (s / 60).into())],
        ),
        s => crate::i18n::message(
            "repo-workflows-activity-ago-4d631fe4",
            &[("s-3600-1", (s / 3600).into())],
        ),
    }
}

#[cfg(test)]
mod operation_presentation_tests {
    use super::*;
    use crate::repo::activity::{ActivityLog, Outcome};
    use crate::repo::notice::operation::{Failure, Status};
    use crate::repo::push_preflight::core_guard_paint_tests::paint;
    use crate::repo::{Notice, operations::check_stack_captured};
    use std::time::Duration;

    #[test]
    fn same_recording_stack_check_repaints_retained_status_finished_and_actual_activity()
    -> Result<(), String> {
        let hash = "a".repeat(40);
        let other = "b".repeat(40);
        let id = "I0123456789abcdef0123456789abcdef01234567";
        let valid = format!("Subject\n\nChange-Id: {id}");
        let cases = [
            (
                format!(
                    "{hash}\0parent\0{}\0",
                    "opaque commit message\n".repeat(800000)
                ),
                "commit-msg-Hook",
            ),
            (format!("{hash}\0first second\0{valid}\0"), "Merge-Commit"),
            (
                format!("{hash}\0parent\0{valid}\0{other}\0parent\0{valid}\0"),
                "dieselbe Change-Id",
            ),
            (String::new(), "keine Commits"),
        ];
        for (log, german) in cases {
            crate::i18n::set_language("de");
            let control = crate::git::OpControl::new(|| {});
            let mut commands = Vec::new();
            let result = check_stack_captured("base", "head", Some(&control), |args| {
                commands.push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
                Ok(if args[0] == "log" {
                    log.clone()
                } else {
                    String::new()
                })
            });
            let raw_error = result.err().ok_or("expected failed recording check")?;
            assert_eq!(commands.len(), 2);
            assert_eq!(commands[0], ["merge-base", "--is-ancestor", "base", "head"]);
            assert_eq!(
                commands[1],
                [
                    "log",
                    "--no-show-signature",
                    "--no-notes",
                    "--encoding=UTF-8",
                    "--format=%H%x00%P%x00%B%x00",
                    "base..head",
                    "--"
                ]
            );
            let receipt = control
                .stack_failure()
                .ok_or("same operation's finite witness")?;
            assert!(format!("{receipt:?}").len() < 1000);
            let status = Notice::operation(Status::Failed {
                label: "Commit".into(),
                error: Failure::capture(&raw_error, Some(receipt.clone())),
            });
            let finished = Notice::operation_finished(
                "repo $(opaque)",
                "Commit",
                &Err(raw_error.clone()),
                Some(receipt.clone()),
            );
            let mut parked = vec![finished];
            let mut queue = Vec::new();
            queue.append(&mut parked);
            let finished = queue.remove(0);
            let status_raw = status.raw().clone();
            let finished_raw = finished.raw().clone();
            assert_eq!(status_raw, (format!("Commit failed: {raw_error}"), true));
            assert_eq!(
                finished_raw,
                (format!("repo $(opaque) · Commit failed: {raw_error}"), true)
            );
            let failure = Failure::capture(&raw_error, Some(receipt));
            let mut activity = ActivityLog::default();
            let id = activity.push_presented(
                "Commit",
                Duration::from_secs(7),
                Vec::new(),
                Outcome::Failed(raw_error.clone()),
                false,
                Some(failure),
            );
            activity.status_link = Some((status_raw.0.clone(), id));
            let link = activity.status_link.clone();
            let ctx = egui::Context::default();
            // Retire the operation-local carrier: retained values must own their source.
            control.stack_checked(None);
            assert!(control.stack_failure().is_none());
            for locale in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(locale);
                let entry = activity.last().ok_or("expected recorded activity")?;
                let shown = paint(&ctx, |ui| entry_ui(ui, entry, true, &Tokens::get(ui.ctx())));
                let error = entry.display_error(&raw_error);
                assert!(shown.contains(&error), "{shown}");
                if locale == "de" {
                    assert!(error.contains(german), "{error}");
                    assert!(
                        status
                            .projected()
                            .ok_or("expected retained presentation")?
                            .contains(german)
                    );
                    assert!(
                        finished
                            .projected()
                            .ok_or("expected retained presentation")?
                            .contains(german)
                    );
                } else {
                    assert_eq!(error, raw_error);
                    assert_eq!(
                        status.projected().ok_or("expected retained presentation")?,
                        status_raw.0
                    );
                    assert_eq!(
                        finished
                            .projected()
                            .ok_or("expected retained presentation")?,
                        finished_raw.0
                    );
                }
                assert_eq!(entry.outcome, Outcome::Failed(raw_error.clone()));
                let mut plain = ActivityLog::default();
                plain.push(
                    "Commit",
                    entry.took,
                    Vec::new(),
                    entry.outcome.clone(),
                    false,
                );
                assert_eq!(
                    entry.as_text(),
                    plain.last().ok_or("expected recorded activity")?.as_text()
                );
                assert_eq!(status.raw(), &status_raw);
                assert_eq!(finished.raw(), &finished_raw);
                assert_eq!(activity.status_link, link);
            }
            assert_eq!(Notice::plain(status_raw).projected(), None);
            assert_eq!(Notice::plain(finished_raw).projected(), None);
        }
        Ok(())
    }

    #[test]
    fn provider_lookalikes_and_later_checks_never_acquire_or_reuse_a_stack_witness()
    -> Result<(), String> {
        let control = crate::git::OpControl::new(|| {});
        let log = format!("{}\0parent\0Missing footer\0", "a".repeat(40));
        let raw = check_stack_captured("b", "h", Some(&control), |args| {
            Ok(if args[0] == "log" {
                log.clone()
            } else {
                String::new()
            })
        })
        .err()
        .ok_or("expected failed recording check")?;
        assert!(control.stack_failure().is_some());
        let changed = format!("{raw}\nprovider added opaque tail");
        let finished =
            Notice::operation_finished("repo", "Commit", &Err(changed), control.stack_failure());
        crate::i18n::set_language("de");
        assert!(
            finished
                .projected()
                .ok_or("expected retained presentation")?
                .ends_with(&raw)
        );
        assert!(
            !finished
                .projected()
                .ok_or("expected retained presentation")?
                .contains("commit-msg-Hook")
        );
        let forged = check_stack_captured("b", "h", Some(&control), |args| {
            if args[0] == "log" {
                Err(raw.clone())
            } else {
                Ok(String::new())
            }
        });
        assert_eq!(forged, Err(raw.clone()));
        assert!(control.stack_failure().is_none());
        for error in [
            raw,
            "Two commits have the same Change-Id (opaque); each change needs its own".into(),
            "provider: missing Change-Id in the message footer\nCommit failed: $(opaque)".into(),
        ] {
            let status = Notice::operation(Status::Failed {
                label: "Commit".into(),
                error: Failure::capture(&error, None),
            });
            let mut log = ActivityLog::default();
            log.push_presented(
                "Commit",
                Duration::ZERO,
                Vec::new(),
                Outcome::Failed(error.clone()),
                false,
                Some(Failure::capture(&error, None)),
            );
            for locale in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(locale);
                assert!(
                    status
                        .projected()
                        .ok_or("expected retained presentation")?
                        .ends_with(&error)
                );
                assert_eq!(
                    log.last()
                        .ok_or("expected recorded activity")?
                        .display_error(&error),
                    error
                );
            }
        }
        let mut commands = Vec::new();
        let cancelled = check_stack_captured("b", "h", Some(&control), |args| {
            commands.push(args[0].to_owned());
            Err(crate::git::CANCELLED.into())
        });
        assert_eq!(cancelled, Err(crate::git::CANCELLED.into()));
        assert_eq!(commands, ["merge-base"]);
        assert!(control.stack_failure().is_none());
        let failed = check_stack_captured("b", "h", Some(&control), |_| {
            Err("provider details stay private".into())
        });
        let error = failed.err().ok_or("expected failed recording check")?;
        let source = control
            .stack_failure()
            .ok_or("expected finite stack failure")?;
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            let shown = source
                .display(&error)
                .ok_or("expected matching stack failure")?;
            if locale == "en-US" {
                assert_eq!(shown, error);
            } else {
                assert_ne!(shown, error);
            }
        }
        Ok(())
    }
}
