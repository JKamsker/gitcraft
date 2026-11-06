//! Ignore rules and attributes: why a file is ignored (the rule and where it is), what its
//! attributes make of it (line endings, text or binary, LFS, merge and diff drivers), the
//! ignored files, and editing a file of rules with a preview of what a change would do.

use egui::{RichText, Ui};

use super::DoctorAct;
use super::ignore::{Editing, IGNORED_SHOWN, IgnoreAct, IgnoreState};
use super::page::{card, code, dim, faint, reading};
use crate::git::doctor::ignore::{self, Explained};
use crate::git::doctor::rules::{Preview, RulesFile};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

/// Most paths a preview lists of each kind.
const PREVIEW_SHOWN: usize = 200;

fn act(a: IgnoreAct) -> Action {
    Action::Doctor(DoctorAct::Ignore(a))
}

pub(super) fn ui(ui: &mut Ui, state: &mut IgnoreState, acts: &mut Vec<Action>) {
    faint(
        ui,
        "Why a file seems to vanish, why line endings keep changing, why a merge driver doesn't \
         run: the rule that ignores a file and the attributes git gives it. A file's menu \
         (Ignore rules and attributes) brings it here.",
    );
    explain_card(ui, state, acts);
    ignored_card(ui, state, acts);
    rules_card(ui, state, acts);
}

/// The path to explain, and what was found.
fn explain_card(ui: &mut Ui, state: &mut IgnoreState, acts: &mut Vec<Action>) {
    card(ui, "Explain a file", None, |ui| {
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut state.typed)
                    .hint_text("path/in/the/repository")
                    .font(crate::theme::mono(12.0))
                    .desired_width(ui.available_width() - 90.0),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let typed = ignore::clean_path(&state.typed);
            let go = ui
                .add_enabled(typed.is_ok(), egui::Button::new("Explain"))
                .on_disabled_hover_text(typed.as_ref().err().map_or("", String::as_str));
            if (go.clicked() || enter)
                && let Ok(path) = typed
            {
                acts.push(act(IgnoreAct::Explain(path)));
            }
        });
        let Some((_, read)) = &state.explained else {
            return;
        };
        let Some(found) = reading(ui, read) else {
            return;
        };
        ui.add_space(8.0);
        let files = state.files.as_ref().and_then(|f| f.as_ref().ok());
        explained(ui, found, files.map_or(&[][..], Vec::as_slice), acts);
    });
}

fn explained(ui: &mut Ui, e: &Explained, files: &[RulesFile], acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.label(code(&e.path).color(t.text));
    ui.add_space(4.0);
    let (color, verdict) = match &e.rule {
        _ if e.ignored() => (t.danger, "Ignored by this rule."),
        Some(rule) if rule.negated() => (
            t.success,
            "Not ignored: this ! rule takes it back from an earlier one.",
        ),
        Some(_) => (
            t.warning,
            "Tracked: ignore rules don't apply to a file git already tracks. This rule would \
             ignore it otherwise; to stop tracking it, remove it from the index (Ignore in its \
             menu does).",
        ),
        None if e.tracked => (t.success, "Tracked, and no ignore rule matches it."),
        None => (t.success, "No ignore rule matches it."),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(verdict).color(color));
    });
    if let Some(rule) = &e.rule {
        ui.horizontal_wrapped(|ui| {
            ui.label(code(&rule.pattern).color(t.text));
            ui.label(code(&format!("{}:{}", rule.source, rule.line)).color(t.text_faint));
            if let Some(i) = files.iter().position(|f| same_file(&f.label, &rule.source))
                && ui.button("Edit this file").clicked()
            {
                acts.push(act(IgnoreAct::Edit(i)));
            }
        });
    }
    ui.add_space(8.0);
    attributes(ui, e);
}

/// Whether the list's `label` and the `source` check-ignore printed are the same file.
fn same_file(label: &str, source: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/");
    norm(label) == norm(source)
}

/// What the attributes and line endings make of the file.
fn attributes(ui: &mut Ui, e: &Explained) {
    let t = Tokens::get(ui.ctx());
    if let Some(eol) = &e.eol {
        let part = |what: &str, v: &str| format!("{what}: {}", if v.is_empty() { "—" } else { v });
        dim(
            ui,
            format!(
                "Line endings · {} · {} · {}",
                part("in the repository", &eol.index),
                part("in the working tree", &eol.worktree),
                part("attributes", &eol.attr)
            ),
        );
        let text = |v: &str| matches!(v, "lf" | "crlf" | "mixed");
        if text(&eol.index) && text(&eol.worktree) && eol.index != eol.worktree {
            let why = if eol.attr.is_empty() {
                "The working tree's line endings differ from the repository's. With no text or \
                 eol attribute, each machine's core.autocrlf decides how git converts them, \
                 which is how line-ending churn starts: an eol or text attribute in \
                 .gitattributes makes it the same everywhere."
            } else {
                "The working tree's line endings differ from the repository's: the attributes \
                 convert them the next time git stores or checks out the file."
            };
            ui.label(RichText::new(why).color(t.warning));
        }
    }
    if e.attrs.is_empty() {
        faint(
            ui,
            "No attributes: git decides by itself whether it is text.",
        );
        return;
    }
    for line in ignore::meaning(&e.attrs) {
        dim(ui, line);
    }
    for (attr, driver) in &e.missing_drivers {
        ui.label(
            RichText::new(format!(
                "{attr}={driver} names a driver nobody defined ({attr}.{driver}.* in the \
                 configuration): git quietly does without it."
            ))
            .color(t.warning),
        );
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for a in &e.attrs {
            let text = match a.value.as_str() {
                "set" => a.name.clone(),
                "unset" => format!("-{}", a.name),
                v => format!("{}={v}", a.name),
            };
            widgets::chip(ui, &text, t.text_dim);
        }
    });
}

/// The ignored files git doesn't track; a click explains one.
fn ignored_card(ui: &mut Ui, state: &IgnoreState, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let title = match &state.ignored {
        Some(Ok((_, total))) => format!("Ignored files ({total})"),
        _ => "Ignored files".to_owned(),
    };
    card(
        ui,
        &title,
        Some("Untracked files git ignores; a folder stands for all it holds."),
        |ui| {
            let Some((list, total)) = reading(ui, &state.ignored) else {
                return;
            };
            if list.is_empty() {
                faint(ui, "Nothing is ignored.");
                return;
            }
            for path in list {
                let resp = ui
                    .add(egui::Label::new(code(path).color(t.text_dim)).sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Explain it");
                if resp.clicked() {
                    acts.push(act(IgnoreAct::Explain(
                        path.trim_end_matches('/').to_owned(),
                    )));
                }
            }
            if *total > IGNORED_SHOWN.min(list.len()) {
                faint(ui, format!("and {} more", total - list.len()));
            }
        },
    );
}

/// The files of rules, and the one being edited.
fn rules_card(ui: &mut Ui, state: &mut IgnoreState, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    card(
        ui,
        "Files of ignore rules",
        Some("Edit one, preview what the change would ignore, then save it."),
        |ui| {
            let Some(files) = reading(ui, &state.files) else {
                return;
            };
            let editing = state.editing.as_ref().map(|e| e.file.label.clone());
            for (i, file) in files.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(code(&file.label).color(t.text));
                    ui.label(RichText::new(file.kind).color(t.text_faint));
                    let open = editing.as_deref() == Some(file.label.as_str());
                    if !open && ui.button("Edit").clicked() {
                        acts.push(act(IgnoreAct::Edit(i)));
                    }
                });
            }
        },
    );
    if let Some(editing) = &mut state.editing {
        editor(ui, editing, acts);
    }
}

fn editor(ui: &mut Ui, e: &mut Editing, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let title = format!("Editing {}", e.file.label);
    card(ui, &title, Some(e.file.kind), |ui| {
        if std::mem::take(&mut e.scroll_to) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        let Some(original) = reading(ui, &e.original).cloned() else {
            return;
        };
        let changed = e.text != original;
        let edit = ui.add(
            egui::TextEdit::multiline(&mut e.text)
                .font(crate::theme::mono(12.0))
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        );
        if edit.changed() {
            e.preview = None;
            e.saved = None;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(changed, egui::Button::new("Preview"))
                .on_disabled_hover_text("Change the rules first")
                .clicked()
            {
                acts.push(act(IgnoreAct::Preview));
            }
            let save = ui
                .add_enabled(changed && !e.saving, egui::Button::new("Save"))
                .on_disabled_hover_text(if e.saving {
                    "Saving…"
                } else {
                    "Nothing changed"
                });
            if save.clicked() {
                acts.push(act(IgnoreAct::Save));
            }
            if ui.button("Close").clicked() {
                acts.push(act(IgnoreAct::CloseEditor));
            }
            if e.saving {
                ui.add(crate::spinner::Spinner::new());
            }
        });
        match &e.saved {
            Some(Ok(())) => {
                ui.label(RichText::new("Saved.").color(t.success));
            }
            Some(Err(err)) => {
                ui.label(RichText::new(err).color(t.danger));
            }
            None => {}
        }
        if let Some(read) = &e.preview {
            ui.add_space(6.0);
            if let Some(p) = reading(ui, read) {
                preview(ui, p);
            }
        }
    });
}

fn preview(ui: &mut Ui, p: &Preview) {
    let t = Tokens::get(ui.ctx());
    faint(
        ui,
        "By this file's rules alone: another file's rules may still ignore, or take back, a \
         path listed here.",
    );
    let list = |ui: &mut Ui, title: &str, paths: &[String], color| {
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("{title} ({})", paths.len()))
                .font(crate::theme::semibold(12.5))
                .color(color),
        );
        for path in paths.iter().take(PREVIEW_SHOWN) {
            ui.label(code(path).color(t.text_dim));
        }
        if paths.len() > PREVIEW_SHOWN {
            faint(ui, format!("and {} more", paths.len() - PREVIEW_SHOWN));
        }
    };
    if p == &Preview::default() {
        faint(ui, "The change ignores and un-ignores no file here.");
        return;
    }
    list(ui, "Newly ignored", &p.newly_ignored, t.danger);
    list(ui, "No longer ignored", &p.no_longer_ignored, t.success);
    if !p.tracked_matching.is_empty() {
        list(
            ui,
            "Tracked files the rules match: they stay tracked",
            &p.tracked_matching,
            t.warning,
        );
    }
}
