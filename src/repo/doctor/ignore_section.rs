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
    faint(ui, crate::i18n::text("repo-workflows-doctor-ui-678f7b82"));
    explain_card(ui, state, top, acts);
    ignored_card(ui, state, acts);
    rules_card(ui, state, locked, acts);
}

/// The path to explain, and what was found.
fn explain_card(ui: &mut Ui, state: &mut IgnoreState, top: &str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-explain-card-7486f5b7"),
        None,
        |ui| {
            let typed = ignore::clean_path(&state.typed, top);
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut state.typed)
                        .hint_text("path/in/the/repository")
                        .font(crate::theme::mono(12.0))
                        .desired_width(ui.available_width() - 90.0),
                );
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let go = ui.add_enabled(
                    typed.is_ok(),
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-doctor-explain-card-3b61e4e9",
                    )),
                );
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
        },
    );
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
                    .add_enabled(
                        !unsaved,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-doctor-explained-98353ed7",
                        )),
                    )
                    .on_disabled_hover_text(crate::i18n::text(
                        "repo-workflows-doctor-explained-55b528ba",
                    ))
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
            crate::i18n::text("repo-workflows-doctor-verdict-b78474c9").to_owned(),
        ));
    } else if e.missing {
        out.push((
            t.text_faint,
            crate::i18n::text("repo-workflows-doctor-verdict-c3b9d549").to_owned(),
        ));
    }
    let (color, text) = match &e.rule {
        _ if e.ignored() => (
            t.danger,
            crate::i18n::text("repo-workflows-doctor-verdict-b796f495").to_owned(),
        ),
        Some(rule) if rule.negated() && e.tracked => (
            t.success,
            crate::i18n::text("repo-workflows-doctor-verdict-f0d742e1").to_owned(),
        ),
        Some(rule) if rule.negated() => (
            t.success,
            crate::i18n::text("repo-workflows-doctor-verdict-a292bdad").to_owned(),
        ),
        Some(_) => (
            t.warning,
            crate::i18n::text("repo-workflows-doctor-verdict-050f8d52").to_owned(),
        ),
        None if e.tracked => (
            t.success,
            crate::i18n::text("repo-workflows-doctor-verdict-a1c6264a").to_owned(),
        ),
        None => (
            t.success,
            crate::i18n::text("repo-workflows-doctor-verdict-2459673a").to_owned(),
        ),
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
            crate::i18n::text("repo-workflows-doctor-verdict-765462d4").to_owned(),
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
        "none" => crate::i18n::text("repo-workflows-doctor-eol-word-ec3e00be"),
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
            crate::i18n::message(
                "repo-workflows-doctor-attributes-68cb5f93",
                &[
                    ("eol-word-eol-index-1", (eol_word(&eol.index)).into()),
                    ("eol-word-eol-worktree-2", (eol_word(&eol.worktree)).into()),
                    (
                        "if-eol-attr-is-empty-else-eol-at-3",
                        (if eol.attr.is_empty() {
                            "—"
                        } else {
                            &eol.attr
                        })
                        .into(),
                    ),
                ],
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
            crate::i18n::text("repo-workflows-doctor-attributes-cb2060cc"),
        );
    }
    for line in ignore::meaning(&e.attrs) {
        dim(ui, line);
    }
    for d in &e.missing_drivers {
        let text = match (d.attr.as_str(), d.name.as_str()) {
            ("filter", "lfs") => {
                crate::i18n::text("repo-workflows-doctor-attributes-82806016").to_owned()
            }
            ("filter", name) if d.required => crate::i18n::message(
                "repo-workflows-doctor-attributes-ec97a4d2",
                &[("name", (name).into())],
            ),
            ("filter", name) => crate::i18n::message(
                "repo-workflows-doctor-attributes-4af8906f",
                &[("name", (name).into())],
            ),
            (attr, name) => crate::i18n::message(
                "repo-workflows-doctor-attributes-c6b25148",
                &[
                    ("attr", (attr).into()),
                    ("name", (name).into()),
                    (
                        "if-attr-merge-merges-it-else-dif-1",
                        (if attr == "merge" {
                            crate::i18n::text("repo-workflows-doctor-attributes-f31e8d7f")
                        } else {
                            crate::i18n::text("repo-workflows-doctor-attributes-2176c800")
                        })
                        .into(),
                    ),
                ],
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
            crate::i18n::text("repo-workflows-doctor-eol-notes-868d3e72"),
        ));
    }
    if eol.attr.is_empty() && !binary && !eol.index.is_empty() {
        out.push((
            false,
            crate::i18n::text("repo-workflows-doctor-eol-notes-384973dd"),
        ));
    }
    out
}

/// The ignored files git doesn't track; a click explains one.
fn ignored_card(ui: &mut Ui, state: &IgnoreState, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let title = match &state.ignored {
        Some(Ok((_, total))) => crate::i18n::message(
            "repo-workflows-doctor-ignored-card-37abda7f",
            &[("total", (*(total)).into())],
        ),
        _ => crate::i18n::text("repo-workflows-doctor-ignored-card-0384b20e").to_owned(),
    };
    let note = crate::i18n::text("repo-workflows-doctor-ignored-card-fb0ae18f");
    card(ui, &title, Some(note), |ui| {
        let Some((list, total)) = reading(ui, &state.ignored) else {
            return;
        };
        if list.is_empty() {
            faint(
                ui,
                crate::i18n::text("repo-workflows-doctor-ignored-card-1303e9b5"),
            );
            return;
        }
        for path in list {
            let resp = ui
                .add(egui::Label::new(code(path).color(t.text_dim)).sense(egui::Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-doctor-ignored-card-cdd2ef27",
                ));
            if resp.clicked() {
                acts.push(act(IgnoreAct::Explain(path.clone())));
            }
        }
        if *total > list.len().min(IGNORED_SHOWN) {
            faint(
                ui,
                crate::i18n::message(
                    "repo-workflows-doctor-ignored-card-683768ab",
                    &[("total-list-len-1", (total - list.len()).into())],
                ),
            );
        }
    });
}

/// The files of rules, and the one being edited.
fn rules_card(ui: &mut Ui, state: &mut IgnoreState, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let unsaved = state.unsaved();
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-rules-card-91b1b197"),
        Some(crate::i18n::text(
            "repo-workflows-doctor-rules-card-379ee96f",
        )),
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
                            .add_enabled(
                                !unsaved,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-doctor-rules-card-464c4ffd",
                                )),
                            )
                            .on_disabled_hover_text(crate::i18n::text(
                                "repo-workflows-doctor-rules-card-55b528ba",
                            ))
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
        crate::i18n::message(
            "repo-workflows-doctor-editor-b5bfc677",
            &[("e-file-label-1", (e.file.label).clone().into())],
        )
    } else {
        crate::i18n::message(
            "repo-workflows-doctor-editor-52448408",
            &[("e-file-label-1", (e.file.label).clone().into())],
        )
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
                .add_enabled(
                    changed,
                    egui::Button::new(crate::i18n::text("repo-workflows-doctor-editor-324b134f")),
                )
                .on_disabled_hover_text(crate::i18n::text("repo-workflows-doctor-editor-a875327e"))
                .clicked()
            {
                acts.push(act(IgnoreAct::Preview));
            }
            let save_text = if e.confirm_save {
                crate::i18n::text("repo-workflows-doctor-editor-5902680c")
            } else {
                crate::i18n::text("repo-workflows-doctor-editor-1509f561")
            };
            let save = ui
                .add_enabled(
                    changed && !e.saving && !locked,
                    egui::Button::new(save_text),
                )
                .on_disabled_hover_text(if e.saving {
                    crate::i18n::text("repo-workflows-doctor-editor-23e39291")
                } else if locked {
                    crate::i18n::text("repo-workflows-doctor-editor-b5c82673")
                } else {
                    crate::i18n::text("repo-workflows-doctor-editor-0ba2a573")
                });
            if save.clicked() {
                acts.push(act(IgnoreAct::Save));
            }
            let close = if unsaved {
                crate::i18n::text("repo-workflows-doctor-editor-f9bfa3dc")
            } else {
                crate::i18n::text("repo-workflows-doctor-editor-7d9eb7ac")
            };
            if ui.button(close).clicked() {
                acts.push(act(IgnoreAct::CloseEditor));
            }
            if e.saving {
                ui.add(crate::spinner::Spinner::new());
            }
        });
        if e.confirm_save {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-doctor-editor-1ee81c8e",
                    &[("e-file-label-1", (e.file.label).clone().into())],
                ))
                .color(t.warning),
            );
        }
        match &e.saved {
            Some(Ok(())) => {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-doctor-editor-fca046cc"))
                        .color(t.success),
                );
            }
            Some(Err(err)) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(err).color(t.danger));
                    if ui
                        .button(crate::i18n::text("repo-workflows-doctor-editor-6c1adba2"))
                        .clicked()
                    {
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
        crate::i18n::text("repo-workflows-doctor-preview-cd2605b4"),
    );
    let list = |ui: &mut Ui, title: &str, paths: &[String], color| {
        if paths.is_empty() {
            return;
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("{title} ({})", crate::i18n::number(paths.len())))
                .font(crate::theme::semibold(12.5))
                .color(color),
        );
        for path in paths.iter().take(PREVIEW_SHOWN) {
            ui.label(code(path).color(t.text_dim));
        }
        if paths.len() > PREVIEW_SHOWN {
            faint(
                ui,
                crate::i18n::message(
                    "repo-workflows-doctor-preview-683768ab",
                    &[(
                        "paths-len-preview-shown-1",
                        (paths.len() - PREVIEW_SHOWN).into(),
                    )],
                ),
            );
        }
    };
    if p.newly_ignored.is_empty() && p.no_longer_ignored.is_empty() {
        faint(
            ui,
            crate::i18n::text("repo-workflows-doctor-preview-bbb0c0cc"),
        );
    }
    list(
        ui,
        crate::i18n::text("repo-workflows-doctor-preview-63666b35"),
        &p.newly_ignored,
        t.danger,
    );
    list(
        ui,
        crate::i18n::text("repo-workflows-doctor-preview-0f241c6b"),
        &p.no_longer_ignored,
        t.success,
    );
    list(
        ui,
        crate::i18n::text("repo-workflows-doctor-preview-8c8b9b2a"),
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
