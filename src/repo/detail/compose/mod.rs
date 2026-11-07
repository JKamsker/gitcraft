//! What helps write a commit message in the commit box: the repository's `commit.template` and
//! its `prepare-commit-msg` hook to start from, saved templates, a type / scope picker
//! (Conventional Commits, Gitmoji), _Add co-author_, completion while typing, and the issue
//! references of a message as links. Each piece is a small component of its own; this holds
//! what they share and the prefill.

mod coauthor;
pub(in crate::repo) mod complete;
pub(in crate::repo) mod issue_links;
mod options;
mod picker;
pub(in crate::repo) mod prefix;
pub(in crate::repo) mod store;
mod templates;
#[cfg(test)]
mod tests;

use std::path::Path;

use egui::{RichText, Ui};

use crate::git::{self, Author, MessageSource};
use crate::repo::RepoTab;
use crate::theme::Tokens;
use complete::{Candidates, Field, Suggestions};

/// What the commit box's helpers keep for a tab.
pub(in crate::repo) struct Compose {
    /// The box is to be started from the template and the hook: the tab was opened, or a
    /// commit was just made.
    prefill_due: bool,
    prefilling: bool,
    /// What went wrong reading the template or running the hook (never in the way of
    /// committing).
    notice: Option<String>,
    /// The repository's `commit.template`, as last read.
    template: Option<String>,
    /// The message as the prefill put it in, and whether a template was in it: while the box
    /// still holds just that, amending, a merge's message or a template may take its place,
    /// and an unchanged template isn't committed (as git refuses it).
    prefilled: Option<(String, bool)>,
    /// What the box held before a template or the hook replaced it, for _Undo_.
    replaced: Option<String>,
    /// The completion candidates, and the status read and earlier prefixes they were made
    /// from.
    candidates: Option<(u64, bool, Candidates)>,
    /// The repository's authors, once asked for (`None` while they are read).
    authors: Option<Result<Vec<Author>, String>>,
    authors_asked: bool,
    /// The types and scopes of earlier commits, once read.
    used: Option<(Vec<String>, Vec<String>)>,
    used_asked: bool,
    /// The completion list showing, and in which field.
    completion: Option<(Field, Suggestions)>,
    /// A co-author being added (the trailer is written in the background).
    adding_coauthor: bool,
    coauthor_filter: String,
    picker: picker::PickerState,
    saving: templates::SaveState,
    rule: options::RuleDraft,
}

impl Default for Compose {
    fn default() -> Self {
        Self {
            prefill_due: true,
            prefilling: false,
            notice: None,
            template: None,
            prefilled: None,
            replaced: None,
            candidates: None,
            authors: None,
            authors_asked: false,
            used: None,
            used_asked: false,
            completion: None,
            adding_coauthor: false,
            coauthor_filter: String::new(),
            picker: picker::PickerState::default(),
            saving: templates::SaveState::default(),
            rule: options::RuleDraft::default(),
        }
    }
}

impl Compose {
    /// A commit was made: the next message starts from the template and hook again, and the
    /// earlier subjects and authors are read again when next needed.
    pub fn committed(&mut self) {
        self.prefill_due = true;
        self.prefilled = None;
        self.replaced = None;
        self.used = None;
        self.used_asked = false;
        self.authors = None;
        self.authors_asked = false;
    }
}

/// `text` split as the commit box holds it: the summary (first line) and the description.
pub fn split_message(text: &str) -> (String, String) {
    let (summary, body) = text.split_once('\n').unwrap_or((text, ""));
    let mut summary = summary.trim().to_owned();
    // A prefix waiting for its text (`feat:` from a template, whose space git's cleanup took
    // away) gets the space back, so that typing goes after it.
    if summary.ends_with(':') {
        summary.push(' ');
    }
    (summary, body.trim().to_owned())
}

/// The summary and description as one message, as `git commit -m … -m …` makes it.
pub fn join_message(summary: &str, body: &str) -> String {
    let (summary, body) = (summary.trim(), body.trim());
    if body.is_empty() {
        summary.to_owned()
    } else {
        format!("{summary}\n\n{body}")
    }
}

/// What the box starts from: the template, then what `prepare-commit-msg` makes of it.
/// Returns the message (`None`: nothing to put in), the template read, and what went wrong.
pub fn prefill(repo: &Path, run_hook: bool) -> Prefilled {
    let mut problems = Vec::new();
    let template = git::commit_template(repo).unwrap_or_else(|e| {
        problems.push(e);
        None
    });
    let seed = template.clone().unwrap_or_default();
    let source = if template.is_some() {
        MessageSource::Template
    } else {
        MessageSource::Empty
    };
    let hooked = if run_hook {
        git::prepare_commit_msg(repo, &seed, &source).unwrap_or_else(|e| {
            problems.push(e);
            None
        })
    } else {
        None
    };
    let message = hooked.or_else(|| template.clone());
    Prefilled {
        message: message.filter(|m| !m.trim().is_empty()),
        template,
        problems,
    }
}

#[derive(Debug, Default)]
pub struct Prefilled {
    pub message: Option<String>,
    pub template: Option<String>,
    pub problems: Vec<String>,
}

impl RepoTab {
    fn message_is_empty(&self) -> bool {
        self.commit_summary.trim().is_empty() && self.commit_body.trim().is_empty()
    }

    fn current_message(&self) -> String {
        join_message(&self.commit_summary, &self.commit_body)
    }

    /// Whether the box holds just what the prefill put there (nothing typed since).
    pub(in crate::repo) fn unedited_prefill(&self) -> bool {
        self.compose
            .prefilled
            .as_ref()
            .is_some_and(|(text, _)| *text == self.current_message())
    }

    /// Whether the box holds an unchanged template: committing it is refused, as git does.
    pub(in crate::repo) fn unedited_template(&self) -> bool {
        self.unedited_prefill()
            && self
                .compose
                .prefilled
                .as_ref()
                .is_some_and(|(_, template)| *template)
    }

    /// Puts `text` in the box, in place of what was there (kept for _Undo_ when it was
    /// something else than an unedited prefill).
    fn set_message(&mut self, text: &str) {
        let before = self.current_message();
        if !before.is_empty() && !self.unedited_prefill() {
            self.compose.replaced = Some(before);
        }
        let (summary, body) = split_message(text);
        self.commit_summary = summary;
        self.commit_body = body;
        self.compose.completion = None;
        self.compose.prefilled = None;
    }

    /// Puts a message git prepared (the commit being amended, a merge's) in the box when it is
    /// empty or holds an unedited prefill.
    pub(in crate::repo) fn take_message(&mut self, text: &str) {
        if self.message_is_empty() || self.unedited_prefill() {
            self.set_message(text);
        }
    }

    /// Starts the box from the template and the hook, in the background, when it is due and
    /// the box is empty. Never while amending (that starts from the commit's message) or
    /// continuing a rebase (which keeps its message), nor over a merge's prepared message.
    pub(in crate::repo) fn compose_prefill(&mut self, ctx: &egui::Context) {
        let c = &self.compose;
        let continuing = self.work.in_progress.is_some_and(|s| s.continues());
        if !c.prefill_due
            || c.prefilling
            || self.work_read == 0
            || self.amend
            || continuing
            || self.busy_op.is_some()
            || self.work.prepared_message.is_some()
            || !self.message_is_empty()
        {
            return;
        }
        self.compose.prefill_due = false;
        self.compose.prefilling = true;
        let repo = self.path.clone();
        let run_hook = !store::Store::load(ctx).no_hook_prefill;
        self.then(
            ctx,
            move || prefill(&repo, run_hook),
            |tab, found, _| {
                tab.compose.prefilling = false;
                tab.compose.notice =
                    (!found.problems.is_empty()).then(|| found.problems.join("\n"));
                // Unless something else came first: typing, amending, a merge's message.
                let continuing = tab.work.in_progress.is_some_and(|s| s.continues());
                if let Some(message) = found.message
                    && tab.message_is_empty()
                    && !tab.amend
                    && !continuing
                    && tab.work.prepared_message.is_none()
                {
                    tab.set_message(&message);
                    let template = found.template.is_some();
                    tab.compose.prefilled = Some((tab.current_message(), template));
                }
                tab.compose.template = found.template;
            },
        );
    }

    /// Runs `prepare-commit-msg` again on what the box holds now (from the templates menu).
    fn rerun_hook(&mut self, ctx: &egui::Context) {
        if self.compose.prefilling {
            return;
        }
        self.compose.prefilling = true;
        let repo = self.path.clone();
        let seed = join_message(&self.commit_summary, &self.commit_body);
        let source = if self.amend {
            MessageSource::Commit("HEAD".to_owned())
        } else if self.work.prepared_message.is_some() {
            MessageSource::Merge
        } else if seed.is_empty() {
            MessageSource::Empty
        } else if self.unedited_template() {
            MessageSource::Template
        } else {
            MessageSource::Message
        };
        let before = seed.clone();
        self.then(
            ctx,
            move || git::prepare_commit_msg(&repo, &seed, &source),
            move |tab, result, _| {
                tab.compose.prefilling = false;
                match result {
                    // Not over what was typed while it ran.
                    Ok(Some(_)) if tab.current_message() != before => {
                        tab.compose.notice = Some(
                            "The message changed while prepare-commit-msg ran: run it again"
                                .to_owned(),
                        );
                    }
                    Ok(Some(message)) => {
                        tab.compose.notice = None;
                        tab.set_message(&message);
                    }
                    Ok(None) => {
                        tab.compose.notice =
                            Some("This repository has no prepare-commit-msg hook".to_owned());
                    }
                    Err(e) => tab.compose.notice = Some(e),
                }
            },
        );
    }

    /// The types and scopes of earlier commits and the changed files, for completion; the
    /// earlier ones are read in the background the first time. Made again only when the status
    /// or what was read changed, not on every frame.
    fn completion_candidates(&mut self, ctx: &egui::Context) -> Candidates {
        if !self.compose.used_asked {
            self.compose.used_asked = true;
            let repo = self.path.clone();
            self.then(
                ctx,
                move || prefix::used_prefixes(&git::recent_subjects(&repo)),
                |tab, used, _| {
                    tab.compose.used = Some(used);
                    tab.compose.candidates = None;
                },
            );
        }
        let types = store::Store::style(ctx) == store::PrefixStyle::Conventional;
        if let Some((read, with_types, cands)) = self.compose.candidates.take()
            && read == self.work_read
            && with_types == types
        {
            return cands;
        }
        let (kinds, scopes) = self.compose.used.clone().unwrap_or_default();
        let mut files: Vec<String> = (self.work.staged.iter())
            .chain(&self.work.unstaged)
            .chain(&self.work.conflicted)
            .map(|f| f.path.clone())
            .collect();
        files.sort();
        files.dedup();
        Candidates {
            files,
            kinds,
            scopes,
            types,
        }
    }

    /// The summary and description fields, with completion. `enabled`: whether they can be
    /// typed in.
    pub(in crate::repo) fn message_fields(
        &mut self,
        ui: &mut Ui,
        enabled: bool,
    ) -> (egui::Response, egui::Response) {
        let cands = self.completion_candidates(ui.ctx());
        let mut open = self.compose.completion.take();
        let (summary, body) = ui
            .add_enabled_ui(enabled, |ui| {
                let summary = complete::field(
                    ui,
                    &mut self.commit_summary,
                    |text| {
                        egui::TextEdit::singleline(text)
                            .hint_text("Summary")
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(8, 6))
                    },
                    Field::Summary,
                    &cands,
                    &mut open,
                );
                let body = complete::field(
                    ui,
                    &mut self.commit_body,
                    |text| {
                        egui::TextEdit::multiline(text)
                            .hint_text("Description (optional)")
                            .desired_width(f32::INFINITY)
                            .desired_rows(3)
                            .margin(egui::Margin::symmetric(8, 6))
                    },
                    Field::Body,
                    &cands,
                    &mut open,
                );
                (summary, body)
            })
            .inner;
        self.compose.completion = open;
        self.compose.candidates = Some((self.work_read, cands.types, cands));
        (summary, body)
    }

    /// The row of helpers above the message: templates, the type / scope picker, co-authors
    /// and their options.
    pub(in crate::repo) fn compose_toolbar(&mut self, ui: &mut Ui, enabled: bool) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_enabled_ui(enabled, |ui| {
                self.templates_button(ui);
                self.picker_button(ui);
                self.coauthor_button(ui);
            });
            self.options_button(ui);
            if self.compose.prefilling || self.compose.adding_coauthor {
                ui.add(crate::spinner::Spinner::new());
            }
        });
    }

    /// What went wrong with the template or the hook, under the message, until dismissed.
    pub(in crate::repo) fn compose_notice(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        if self.compose.replaced.is_some() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("The message was replaced.")
                        .color(t.text_faint)
                        .small(),
                );
                if ui.small_button("Undo").clicked()
                    && let Some(before) = self.compose.replaced.take()
                {
                    let (summary, body) = split_message(&before);
                    self.commit_summary = summary;
                    self.commit_body = body;
                    self.compose.prefilled = None;
                }
                if crate::icons::small_button(ui, "x", "Dismiss").clicked() {
                    self.compose.replaced = None;
                }
            });
        }
        let Some(notice) = self.compose.notice.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            if crate::icons::small_button(ui, "x", "Dismiss").clicked() {
                self.compose.notice = None;
            }
            ui.add(egui::Label::new(RichText::new(notice).color(t.warning).small()).wrap());
        });
    }
}
