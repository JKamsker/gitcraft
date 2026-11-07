//! Ignore rules and attributes: why a file is ignored (the rule and where it is), what its
//! attributes make of it (line endings, text or binary, LFS, merge and diff drivers), the
//! ignored files, and editing a file of rules with a preview of what a change would do.

use egui::{RichText, Ui};

use super::DoctorAct;
use super::ignore::{Editing, IGNORED_SHOWN, IgnoreAct, IgnoreState, lf};
use super::page::{card, code, dim, faint, reading};
use crate::git::doctor::ignore::{self, Eol, Explained};
use crate::git::doctor::rules::{Preview, RulesFile};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

/// Most paths a preview lists of each kind.
const PREVIEW_SHOWN: usize = 200;

fn act(a: IgnoreAct) -> Action {
    Action::Doctor(DoctorAct::Ignore(a))
}

pub(super) fn ui(
    ui: &mut Ui,
    state: &mut IgnoreState,
    top: &str,
    locked: bool,
    acts: &mut Vec<Action>,
) {
    faint(
        ui,
        "Why a file seems to vanish, why line endings keep changing, why a merge driver doesn't \
         run: the rule that ignores a file and the attributes git gives it, as the working \
         tree has them. A file's menu (Ignore rules and attributes) brings it here.",
    );
    explain_card(ui, state, top, acts);
    ignored_card(ui, state, acts);
    rules_card(ui, state, locked, acts);
}

/// The path to explain, and what was found.
fn explain_card(ui: &mut Ui, state: &mut IgnoreState, top: &str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    card(ui, "Explain a file", None, |ui| {
        let typed = ignore::clean_path(&state.typed, top);
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut state.typed)
                    .hint_text("path/in/the/repository")
                    .font(crate::theme::mono(12.0))
                    .desired_width(ui.available_width() - 90.0),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let go = ui.add_enabled(typed.is_ok(), egui::Button::new("Explain"));
            if (go.clicked() || enter)
                && let Ok(path) = &typed
            {
                acts.push(act(IgnoreAct::Explain(path.clone())));
            }
        });
        if let Err(why) = &typed
            && !state.typed.is_empty()
        {
            ui.label(RichText::new(why).color(t.danger));
        }
        let Some((_, read)) = &state.explained else {
            return;
        };
        let Some(found) = reading(ui, read) else {
            return;
        };
        ui.add_space(8.0);
        let files = state.files.as_ref().and_then(|f| f.as_ref().ok());
        let editing = state.editing.as_ref().map(|e| e.file.label.as_str());
        let unsaved = state.unsaved();
        explained(
            ui,
            found,
            files.map_or(&[][..], Vec::as_slice),
            editing,
            unsaved,
            acts,
        );
    });
}

fn explained(
    ui: &mut Ui,
    e: &Explained,
    files: &[RulesFile],
    editing: Option<&str>,
    unsaved: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    ui.label(code(&e.path).color(t.text));
    ui.add_space(4.0);
    for (color, line) in verdict(e, &t) {
        ui.add(egui::Label::new(RichText::new(line).color(color)).wrap());
    }
    if let Some(rule) = &e.rule {
        ui.horizontal_wrapped(|ui| {
            ui.label(code(&rule.pattern).color(t.text));
            ui.label(code(&format!("{}:{}", rule.source, rule.line)).color(t.text_faint));
            let at = files.iter().position(|f| same_file(&f.label, &rule.source));
            let open = at.is_some_and(|i| editing == Some(files[i].label.as_str()));
            if let Some(i) = at.filter(|_| !open)
                && ui
                    .add_enabled(!unsaved, egui::Button::new("Edit this file"))
                    .on_disabled_hover_text("Save or discard the changes being edited first")
                    .clicked()
            {
                acts.push(act(IgnoreAct::Edit(i)));
            }
        });
    }
    ui.add_space(8.0);
    if !e.folder {
        attributes(ui, e);
    }
}

/// What the rule (or none) means for the path: lines of text and their colour.
fn verdict(e: &Explained, t: &Tokens) -> Vec<(egui::Color32, String)> {
    let mut out = Vec::new();
    if e.folder {
        out.push((
            t.text_dim,
            "A folder: the rule shown decides for everything in it (git doesn't look inside an \
             ignored folder)."
                .to_owned(),
        ));
    } else if e.missing {
        out.push((
            t.text_faint,
            "There is no such file here: the rules are read as for a file of that name (a rule \
             for folders, ending in /, can't match it)."
                .to_owned(),
        ));
    }
    let (color, text) = match &e.rule {
        _ if e.ignored() => (t.danger, "Ignored by this rule.".to_owned()),
        Some(rule) if rule.negated() && e.tracked => (
            t.success,
            "Tracked, and the last rule that matches it is this ! rule.".to_owned(),
        ),
        Some(rule) if rule.negated() => (
            t.success,
            "Not ignored: this ! rule is the last one that matches it.".to_owned(),
        ),
        Some(_) => (
            t.warning,
            "Tracked: ignore rules don't apply to files git already tracks, so this rule has no \
             effect on it. To have it ignored, stop tracking it (git rm --cached; the file stays \
             on disk)."
                .to_owned(),
        ),
        None if e.tracked => (
            t.success,
            "Tracked, and no ignore rule matches it.".to_owned(),
        ),
        None => (t.success, "No ignore rule matches it.".to_owned()),
    };
    out.push((color, text));
    // A rule for its folder wins over any ! rule for the file itself.
    if e.ignored()
        && let Some(rule) = &e.rule
        && rule.pattern.ends_with('/')
        && !e.folder
    {
        out.push((
            t.text_faint,
            "Its folder is ignored: git doesn't look inside, so a ! rule for a file in it has no \
             effect. Un-ignore the folder (!folder/), or ignore folder/* instead."
                .to_owned(),
        ));
    }
    out
}

/// Whether the list's `label` and the `source` check-ignore printed are the same file.
fn same_file(label: &str, source: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/");
    norm(label) == norm(source)
}

/// A line-ending word of `ls-files --eol`, in words.
fn eol_word(v: &str) -> &str {
    match v {
        "" => "—",
        "none" => "no line breaks",
        "-text" => "binary",
        other => other,
    }
}

/// What the attributes and line endings make of the file.
fn attributes(ui: &mut Ui, e: &Explained) {
    let t = Tokens::get(ui.ctx());
    if let Some(eol) = &e.eol {
        dim(
            ui,
            format!(
                "Line endings · in the repository: {} · in the working tree: {} · attributes: {}",
                eol_word(&eol.index),
                eol_word(&eol.worktree),
                if eol.attr.is_empty() {
                    "—"
                } else {
                    &eol.attr
                }
            ),
        );
        for (warn, note) in eol_notes(eol) {
            let color = if warn { t.warning } else { t.text_faint };
            ui.add(egui::Label::new(RichText::new(note).color(color)).wrap());
        }
    }
    if e.attrs.is_empty() {
        faint(
            ui,
            "No attributes: git guesses text or binary for diffs and merges, and core.autocrlf \
             alone decides line endings.",
        );
    }
    for line in ignore::meaning(&e.attrs) {
        dim(ui, line);
    }
    for d in &e.missing_drivers {
        let text = match (d.attr.as_str(), d.name.as_str()) {
            ("filter", "lfs") => "filter=lfs, but Git LFS isn't set up here (git lfs install): \
                                  checkouts leave pointer files, and files you add go into git \
                                  itself, not LFS."
                .to_owned(),
            ("filter", name) if d.required => format!(
                "filter={name} is required but has no clean or smudge command here: git refuses \
                 to add or check out this file."
            ),
            ("filter", name) => format!(
                "filter={name} names a filter this configuration doesn't define: git stores and \
                 checks the file out as it is."
            ),
            (attr, name) => format!(
                "{attr}={name} names a driver this configuration doesn't define \
                 ({attr}.{name}.*): git {} as usual.",
                if attr == "merge" {
                    "merges it"
                } else {
                    "diffs it"
                }
            ),
        };
        let color = if d.required { t.danger } else { t.warning };
        ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
    }
    if !e.attrs.is_empty() {
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
}

/// What to say about the line endings: (a warning, the note).
pub(super) fn eol_notes(eol: &Eol) -> Vec<(bool, &'static str)> {
    let mut out = Vec::new();
    let binary = eol.attr.contains("-text") || eol.index == "-text";
    if !binary && matches!(eol.index.as_str(), "crlf" | "mixed") {
        out.push((
            true,
            "Stored in the repository with CRLF or mixed line endings: whoever has core.autocrlf \
             set will see the whole file change.",
        ));
    }
    if eol.attr.is_empty() && !binary && !eol.index.is_empty() {
        out.push((
            false,
            "No text or eol attribute: each machine's core.autocrlf decides its line endings, \
             which is how line-ending churn starts; `* text=auto` in .gitattributes makes it the \
             same everywhere.",
        ));
    }
    out
}

/// The ignored files git doesn't track; a click explains one.
fn ignored_card(ui: &mut Ui, state: &IgnoreState, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let title = match &state.ignored {
        Some(Ok((_, total))) => format!("Ignored files ({total})"),
        _ => "Ignored files".to_owned(),
    };
    let note = "Untracked files git ignores; a folder stands for all it holds. Click one to \
                explain it.";
    card(ui, &title, Some(note), |ui| {
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
                acts.push(act(IgnoreAct::Explain(path.clone())));
            }
        }
        if *total > list.len().min(IGNORED_SHOWN) {
            faint(ui, format!("and {} more", total - list.len()));
        }
    });
}

/// The files of rules, and the one being edited.
fn rules_card(ui: &mut Ui, state: &mut IgnoreState, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let unsaved = state.unsaved();
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
                    if !open
                        && ui
                            .add_enabled(!unsaved, egui::Button::new("Edit"))
                            .on_disabled_hover_text(
                                "Save or discard the changes being edited first",
                            )
                            .clicked()
                    {
                        acts.push(act(IgnoreAct::Edit(i)));
                    }
                });
            }
        },
    );
    if let Some(editing) = &mut state.editing {
        editor(ui, editing, locked, acts);
    }
}

fn editor(ui: &mut Ui, e: &mut Editing, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let unsaved = e.unsaved();
    let title = if unsaved {
        format!("Editing {} · not saved", e.file.label)
    } else {
        format!("Editing {}", e.file.label)
    };
    card(ui, &title, Some(e.file.kind), |ui| {
        if std::mem::take(&mut e.scroll_to) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        let Some(original) = reading(ui, &e.original).map(|o| lf(o)) else {
            return;
        };
        let edit = ui.add(
            egui::TextEdit::multiline(&mut e.text)
                .font(crate::theme::mono(12.0))
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        );
        if edit.changed() {
            e.preview = None;
            e.saved = None;
            e.confirm_save = false;
        }
        let changed = e.text != original;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(changed, egui::Button::new("Preview"))
                .on_disabled_hover_text("Change the rules first")
                .clicked()
            {
                acts.push(act(IgnoreAct::Preview));
            }
            let save_text = if e.confirm_save {
                "Save outside the repository"
            } else {
                "Save"
            };
            let save = ui
                .add_enabled(
                    changed && !e.saving && !locked,
                    egui::Button::new(save_text),
                )
                .on_disabled_hover_text(if e.saving {
                    "Saving…"
                } else if locked {
                    "Wait for the running operation"
                } else {
                    "Nothing changed"
                });
            if save.clicked() {
                acts.push(act(IgnoreAct::Save));
            }
            let close = if unsaved { "Discard changes" } else { "Close" };
            if ui.button(close).clicked() {
                acts.push(act(IgnoreAct::CloseEditor));
            }
            if e.saving {
                ui.add(crate::spinner::Spinner::new());
            }
        });
        if e.confirm_save {
            ui.label(
                RichText::new(format!(
                    "{} isn't a file of this repository: it applies to every repository that \
                     reads it. Click again to save it.",
                    e.file.label
                ))
                .color(t.warning),
            );
        }
        match &e.saved {
            Some(Ok(())) => {
                ui.label(RichText::new("Saved.").color(t.success));
            }
            Some(Err(err)) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(err).color(t.danger));
                    if ui.button("Read it again").clicked() {
                        acts.push(act(IgnoreAct::Reread));
                    }
                });
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
        "By this file's rules alone: rules in other files are left out, so a ! rule here that \
         takes back what another file ignores doesn't show, and another file may still ignore \
         a path listed here.",
    );
    let list = |ui: &mut Ui, title: &str, paths: &[String], color| {
        if paths.is_empty() {
            return;
        }
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
    if p.newly_ignored.is_empty() && p.no_longer_ignored.is_empty() {
        faint(
            ui,
            "This change wouldn't start or stop ignoring any untracked file.",
        );
    }
    list(ui, "Newly ignored", &p.newly_ignored, t.danger);
    list(ui, "No longer ignored", &p.no_longer_ignored, t.success);
    list(
        ui,
        "Tracked files this file's rules match (they stay tracked)",
        &p.tracked_matching,
        t.warning,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eol(index: &str, worktree: &str, attr: &str) -> Eol {
        Eol {
            index: index.into(),
            worktree: worktree.into(),
            attr: attr.into(),
        }
    }

    #[test]
    fn line_ending_notes_warn_only_where_churn_comes_from() {
        // autocrlf=true on Windows, no attribute: a neutral note, no warning.
        let notes = eol_notes(&eol("lf", "crlf", ""));
        assert_eq!(notes.len(), 1);
        assert!(!notes[0].0);
        // eol=crlf as asked: nothing to say.
        assert!(eol_notes(&eol("lf", "crlf", "text eol=crlf")).is_empty());
        // CRLF stored in the repository: a warning.
        assert!(eol_notes(&eol("crlf", "crlf", "")).iter().any(|(w, _)| *w));
        // Binary: nothing.
        assert!(eol_notes(&eol("-text", "-text", "-text")).is_empty());
        assert!(eol_notes(&eol("", "lf", "")).is_empty());
    }
}
