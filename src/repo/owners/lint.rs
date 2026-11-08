//! The check of the CODEOWNERS file: lines its forge reads otherwise than they look, patterns
//! that match no file, and files without an owner. Opened from the command palette.

use std::sync::Arc;

use egui::{Context, RichText, Ui};

use crate::codeowners::{CodeOwners, Lint};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How many files without an owner are listed; the rest are counted.
const LISTED: usize = 300;

/// The check's window.
pub(crate) struct LintView {
    file: Arc<CodeOwners>,
    /// Which check this is (see `Owners::lint_requests`).
    request: u64,
    /// Why the file couldn't be read again lately (the check is of the one read before).
    stale: Option<String>,
    /// `None` while the repository's files are read and matched.
    pub(super) result: Option<Result<Lint, String>>,
}

/// What the check's window asks for.
pub(crate) enum OwnersAct {
    Check,
    Close,
}

impl RepoTab {
    pub(in crate::repo) fn owners_act(&mut self, act: OwnersAct, ctx: &Context) {
        match act {
            OwnersAct::Close => self.owners.lint = None,
            OwnersAct::Check => {
                let Some(file) = self.owners.file.clone() else {
                    self.status = Some(crate::repo::Notice::plain((
                        "This repository has no CODEOWNERS file".to_owned(),
                        true,
                    )));
                    return;
                };
                self.owners.lint_requests += 1;
                let request = self.owners.lint_requests;
                self.owners.lint = Some(LintView {
                    file: file.clone(),
                    request,
                    stale: self.owners.error.clone(),
                    result: None,
                });
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || {
                        let out = crate::git::run(
                            Some(&repo),
                            &["ls-tree", "-r", "-z", "--name-only", "HEAD"],
                        )?;
                        let files: Vec<String> = out
                            .split('\0')
                            .filter(|p| !p.is_empty())
                            .map(str::to_owned)
                            .collect();
                        std::panic::catch_unwind(|| file.lint(&files)).map_err(|_| {
                            crate::i18n::text("repo-workflows-owners-owners-act-b11dc59d")
                                .to_owned()
                        })
                    },
                    move |tab, result, _| {
                        if let Some(view) = &mut tab.owners.lint
                            && view.request == request
                        {
                            view.result = Some(result);
                        }
                    },
                );
            }
        }
    }

    /// The check's window, while it is open.
    pub(in crate::repo) fn owners_lint_window(&mut self, ctx: &Context, acts: &mut Vec<Action>) {
        let Some(view) = &self.owners.lint else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18));
        let modal = egui::Modal::new(egui::Id::new("gitcraft-codeowners-check")).frame(frame);
        let response = modal.show(ctx, |ui| {
            ui.set_width(560.0);
            contents(ui, view, acts);
        });
        if response.should_close() {
            acts.push(Action::Owners(OwnersAct::Close));
        }
    }
}

fn contents(ui: &mut Ui, view: &LintView, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let file = &view.file;
    ui.label(
        RichText::new(crate::i18n::text("repo-workflows-owners-contents-e1e62224"))
            .font(theme::semibold(15.0)),
    );
    let rules = match file.rule_count() {
        1 => crate::i18n::text("repo-workflows-owners-contents-343bb384").to_owned(),
        n => crate::i18n::message(
            "repo-workflows-owners-contents-4d726f4a",
            &[("n", (n).clone().into())],
        ),
    };
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-owners-contents-96978c8c",
            &[
                ("file-path-1", (file.path).clone().into()),
                ("file-flavor-name-2", (file.flavor.name()).clone().into()),
                ("rules", (rules).clone().into()),
            ],
        ))
        .color(t.text_dim),
    );
    if let Some(e) = &view.stale {
        ui.colored_label(
            t.warning,
            crate::i18n::message(
                "repo-workflows-owners-contents-c72a165d",
                &[("e", (e).clone().into())],
            ),
        );
    }
    let sections = file.section_labels();
    if !sections.is_empty() {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-owners-contents-4adeeb91",
                &[("sections-join-1", (sections.join(" · ")).clone().into())],
            ))
            .color(t.text_faint)
            .small(),
        );
    }
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            heading(
                ui,
                &crate::i18n::message(
                    "repo-workflows-owners-contents-64feb239",
                    &[("file-problems-len-1", (file.problems.len()).clone().into())],
                ),
            );
            if file.problems.is_empty() {
                faint(
                    ui,
                    crate::i18n::text("repo-workflows-owners-contents-bd73a6b1"),
                );
            }
            for p in &file.problems {
                line(
                    ui,
                    &crate::i18n::message(
                        "repo-workflows-owners-contents-8f8353f4",
                        &[
                            ("p-line-1", (p.line).clone().into()),
                            ("p-text-2", (p.text).clone().into()),
                        ],
                    ),
                    t.warning,
                );
            }
            match &view.result {
                None => {
                    ui.add_space(10.0);
                    ui.add(crate::spinner::Spinner::new());
                }
                Some(Err(e)) => {
                    ui.add_space(10.0);
                    ui.colored_label(
                        t.danger,
                        crate::i18n::message(
                            "repo-workflows-owners-contents-3203b0df",
                            &[("e", (e).clone().into())],
                        ),
                    );
                }
                Some(Ok(lint)) => results(ui, lint),
            }
        });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(crate::i18n::text("repo-workflows-owners-contents-bd5f6b1d"))
                    .min_size(egui::vec2(100.0, 30.0)),
            )
            .on_hover_text(crate::i18n::text("repo-workflows-owners-contents-4b99390e"))
            .clicked()
        {
            acts.push(Action::OpenInEditor(file.path.clone().into()));
        }
        if ui
            .add(
                egui::Button::new(crate::i18n::text("repo-workflows-owners-contents-7d9eb7ac"))
                    .min_size(egui::vec2(80.0, 30.0)),
            )
            .clicked()
        {
            acts.push(Action::Owners(OwnersAct::Close));
        }
    });
}

fn results(ui: &mut Ui, lint: &Lint) {
    let t = Tokens::get(ui.ctx());
    heading(
        ui,
        &crate::i18n::message(
            "repo-workflows-owners-results-85d177f5",
            &[(
                "lint-unmatched-len-1",
                (lint.unmatched.len()).clone().into(),
            )],
        ),
    );
    if lint.unmatched.is_empty() {
        faint(
            ui,
            crate::i18n::text("repo-workflows-owners-results-9c0433a9"),
        );
    }
    for (n, pattern) in &lint.unmatched {
        line(
            ui,
            &crate::i18n::message(
                "repo-workflows-owners-results-a6551c12",
                &[
                    ("n", (n).clone().into()),
                    ("pattern", (pattern).clone().into()),
                ],
            ),
            t.warning,
        );
    }
    heading(
        ui,
        &crate::i18n::message(
            "repo-workflows-owners-results-a92df9d7",
            &[
                ("lint-unowned-len-1", (lint.unowned.len()).clone().into()),
                ("lint-files-2", (lint.files).clone().into()),
            ],
        ),
    );
    if lint.unowned.is_empty() {
        faint(
            ui,
            crate::i18n::text("repo-workflows-owners-results-26e406d1"),
        );
    }
    for path in lint.unowned.iter().take(LISTED) {
        line(ui, path, t.text_dim);
    }
    if lint.unowned.len() > LISTED {
        faint(
            ui,
            &crate::i18n::message(
                "repo-workflows-owners-results-683768ab",
                &[(
                    "lint-unowned-len-listed-1",
                    (lint.unowned.len() - LISTED).clone().into(),
                )],
            ),
        );
    }
}

fn heading(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    ui.label(
        RichText::new(text)
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.add_space(2.0);
}

fn line(ui: &mut Ui, text: &str, color: egui::Color32) {
    ui.add(egui::Label::new(RichText::new(text).font(theme::mono(11.5)).color(color)).wrap());
}

fn faint(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(text).color(t.text_faint));
}
