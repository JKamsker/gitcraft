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
                    self.status = Some(("This repository has no CODEOWNERS file".to_owned(), true));
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
                        std::panic::catch_unwind(|| file.lint(&files))
                            .map_err(|_| "checking it failed".to_owned())
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
    ui.label(RichText::new("Check CODEOWNERS").font(theme::semibold(15.0)));
    let rules = match file.rule_count() {
        1 => "1 rule".to_owned(),
        n => format!("{n} rules"),
    };
    ui.label(
        RichText::new(format!(
            "{} at HEAD, read as {} reads it · {rules}",
            file.path,
            file.flavor.name(),
        ))
        .color(t.text_dim),
    );
    if let Some(e) = &view.stale {
        ui.colored_label(
            t.warning,
            format!("Couldn't read it again lately ({e}): this is the one read before."),
        );
    }
    let sections = file.section_labels();
    if !sections.is_empty() {
        ui.label(
            RichText::new(format!("Sections: {}", sections.join(" · ")))
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
                &format!(
                    "LINES THE FORGE READS DIFFERENTLY ({})",
                    file.problems.len()
                ),
            );
            if file.problems.is_empty() {
                faint(ui, "None: the forge reads every line as written.");
            }
            for p in &file.problems {
                line(ui, &format!("line {}: {}", p.line, p.text), t.warning);
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
                        format!("Couldn't list the repository's files: {e}"),
                    );
                }
                Some(Ok(lint)) => results(ui, lint),
            }
        });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new("Open the file").min_size(egui::vec2(100.0, 30.0)))
            .on_hover_text("In your editor")
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
        &format!("PATTERNS THAT MATCH NO FILE ({})", lint.unmatched.len()),
    );
    if lint.unmatched.is_empty() {
        faint(ui, "None: every pattern matches a file.");
    }
    for (n, pattern) in &lint.unmatched {
        line(ui, &format!("line {n}: {pattern}"), t.warning);
    }
    heading(
        ui,
        &format!(
            "FILES WITHOUT AN OWNER ({} of {})",
            lint.unowned.len(),
            lint.files
        ),
    );
    if lint.unowned.is_empty() {
        faint(ui, "None: every file has an owner.");
    }
    for path in lint.unowned.iter().take(LISTED) {
        line(ui, path, t.text_dim);
    }
    if lint.unowned.len() > LISTED {
        faint(ui, &format!("and {} more", lint.unowned.len() - LISTED));
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
