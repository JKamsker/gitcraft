//! The agent's work under review, in the big area: a bar with each file (and whether it is to
//! be applied), then for the file picked each conflict with its two sides and what the agent
//! wrote in its place, or the file as a line diff. Apply from here once it reads right.

use egui::{Color32, RichText, Stroke, Ui};

use super::review::{FileReview, Line, Resolved, Took};
use super::{Phase, ResolverAct, ReviewState, Run};
use crate::icons;
use crate::repo::{Action, RepoTab, widgets};
use crate::resolver::report::StepStatus;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(in crate::repo) fn resolver_review_view(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(run) = &self.resolver.run else {
            return;
        };
        let Some(review) = &run.review else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        top_bar(ui, &t, run, review, acts);
        let Some(files) = &review.files else {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.add(crate::spinner::Spinner::new().size(18.0));
            });
            return;
        };
        let Some((path, file)) = files.get(review.selected) else {
            return;
        };
        file_bar(ui, &t, run, review, path, file, acts);
        egui::ScrollArea::vertical()
            .id_salt(("resolver-review", path))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Long lines scroll sideways in their own blocks; the view keeps its width.
                let width = ui.available_width() - 32.0;
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        body(ui, &t, file, review.as_diff);
                    });
            });
    }
}

fn top_bar(ui: &mut Ui, t: &Tokens, run: &Run, review: &ReviewState, acts: &mut Vec<Action>) {
    egui::Frame::new()
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .stroke(Stroke::new(1.0, t.separator))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::inline(ui, "bot", 16.0, t.accent);
                ui.label(RichText::new("What the agent did").font(theme::semibold(14.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::small_button(ui, "x", "Back (the agent's card)").clicked() {
                        acts.push(Action::Resolver(ResolverAct::CloseReview));
                    }
                    // Read while the merge is put aside; applied once it is back.
                    if let Some(parked) = &run.parked {
                        super::panel::bring_back_button(ui, parked, acts);
                        return;
                    }
                    let n = run.picked.len();
                    let label = match n {
                        0 => "Nothing to apply".to_owned(),
                        1 => "Apply 1 file".to_owned(),
                        n => format!("Apply {n} files"),
                    };
                    if widgets::primary_button(ui, &label, n > 0 && run.phase == Phase::Ready, 0.0)
                        .on_hover_text(
                            "Writes the files ticked into your checkout and marks them resolved",
                        )
                        .clicked()
                    {
                        acts.push(Action::Resolver(ResolverAct::Apply));
                    }
                });
            });
            // The files, on as many rows as they need: ✓ to apply, ○ left out, ! can't be.
            ui.horizontal_wrapped(|ui| {
                for (i, (path, result)) in run.results.iter().enumerate() {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    let mark = match (result.applies(), run.picked.contains(path)) {
                        (true, true) => "✓",
                        (true, false) => "○",
                        (false, _) => "!",
                    };
                    let r = ui
                        .selectable_label(review.selected == i, format!("{mark} {name}"))
                        .on_hover_text(path);
                    if r.clicked() {
                        acts.push(Action::Resolver(ResolverAct::Review(i)));
                    }
                }
            });
        });
}

/// The file's path, whether to apply it, the agent's note on it and how to show it.
fn file_bar(
    ui: &mut Ui,
    t: &Tokens,
    run: &Run,
    review: &ReviewState,
    path: &str,
    file: &FileReview,
    acts: &mut Vec<Action>,
) {
    let result = run.results.iter().find(|(p, _)| p == path).map(|(_, r)| r);
    let note = run
        .progress
        .steps
        .iter()
        .find(|s| s.id == path || s.file.as_deref() == Some(path))
        .filter(|s| s.status != StepStatus::Pending);
    egui::Frame::new()
        .fill(t.card)
        .inner_margin(egui::Margin::symmetric(16, 8))
        .stroke(Stroke::new(1.0, t.separator))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let mut on = run.picked.contains(path);
                let usable = result.is_some_and(|r| r.applies()) && run.phase == Phase::Ready;
                if ui
                    .add_enabled(usable, egui::Checkbox::new(&mut on, ""))
                    .on_hover_text("Apply this file")
                    .on_disabled_hover_text("Nothing of it can be applied")
                    .changed()
                {
                    acts.push(Action::Resolver(ResolverAct::Pick(path.to_owned(), on)));
                }
                ui.label(RichText::new(path).font(theme::mono(13.0)));
                if let FileReview::Conflicts { resolved, .. } = file {
                    let n = resolved.len();
                    let what = if n == 1 {
                        "1 conflict".to_owned()
                    } else {
                        format!("{n} conflicts")
                    };
                    widgets::chip(ui, &what, t.text_dim);
                }
            });
            // How to show it: by conflict, or as a line diff of the file before and after.
            if matches!(file, FileReview::Conflicts { .. }) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::selectable(!review.as_diff, "By conflict"))
                        .on_hover_text("Each conflict's two sides, and what the agent wrote")
                        .clicked()
                    {
                        acts.push(Action::Resolver(ResolverAct::ReviewAsDiff(false)));
                    }
                    if ui
                        .add(egui::Button::selectable(review.as_diff, "As a diff"))
                        .on_hover_text("The file with its conflicts, and as the agent left it")
                        .clicked()
                    {
                        acts.push(Action::Resolver(ResolverAct::ReviewAsDiff(true)));
                    }
                });
            }
            if let Some(note) = note.and_then(|s| s.note.as_ref()) {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Agent:").color(t.accent).small().strong());
                    ui.label(RichText::new(note).color(t.text).small());
                });
            }
        });
}

fn body(ui: &mut Ui, t: &Tokens, file: &FileReview, as_diff: bool) {
    match file {
        FileReview::Conflicts {
            resolved,
            ours_label,
            theirs_label,
            changed_outside,
            diff,
        } => {
            if as_diff {
                diff_lines(ui, t, diff);
                return;
            }
            if *changed_outside {
                banner(
                    ui,
                    t,
                    t.warning,
                    "The agent also changed lines outside the conflicts: see them As a diff.",
                );
            }
            let n = resolved.len();
            for (i, c) in resolved.iter().enumerate() {
                conflict_card(ui, t, c, (i, n), (ours_label, theirs_label));
                ui.add_space(14.0);
            }
        }
        FileReview::Diff(lines) if lines.is_empty() => {
            banner(ui, t, t.text_dim, "The agent left this file as it was.");
        }
        FileReview::Diff(lines) => {
            banner(
                ui,
                t,
                t.text_dim,
                "Its changes can't be lined up with the conflicts one by one: here is the file \
                 before (with the conflicts) and after.",
            );
            diff_lines(ui, t, lines);
        }
        FileReview::Deleted { before } => {
            banner(ui, t, t.warning, "The agent deleted this file. It was:");
            let lines: Vec<Line> = before
                .lines()
                .map(|l| Line::Removed(l.to_owned()))
                .collect();
            diff_lines(ui, t, &lines);
        }
        FileReview::Created { after } => {
            banner(ui, t, t.text_dim, "The agent made this file:");
            let lines: Vec<Line> = after.lines().map(|l| Line::Added(l.to_owned())).collect();
            diff_lines(ui, t, &lines);
        }
        FileReview::Binary => banner(
            ui,
            t,
            t.text_dim,
            "Not a text file: once applied, gitcraft's diff shows what it is.",
        ),
        FileReview::Unreadable(e) => banner(ui, t, t.danger, e),
    }
}

fn banner(ui: &mut Ui, t: &Tokens, color: Color32, text: &str) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).color(t.text)).wrap());
        });
    ui.add_space(10.0);
}

fn took_color(t: &Tokens, took: Took) -> Color32 {
    match took {
        Took::Ours | Took::Theirs => t.accent,
        Took::Both | Took::BothSwapped => t.success,
        Took::Rewrote => t.warning,
        Took::Removed => t.danger,
    }
}

/// A conflict: its two sides next to each other, and what the agent wrote under them.
fn conflict_card(
    ui: &mut Ui,
    t: &Tokens,
    c: &Resolved,
    (i, n): (usize, usize),
    (ours, theirs): (&str, &str),
) {
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Conflict {} of {n}", i + 1)).font(theme::semibold(13.0)),
                );
                ui.label(
                    RichText::new(format!("line {}", c.line))
                        .color(t.text_faint)
                        .small(),
                );
                widgets::chip(ui, &c.took.words(ours, theirs), took_color(t, c.took));
            });
            ui.add_space(8.0);
            let gap = 10.0;
            let half = ((ui.available_width() - gap) / 2.0).max(120.0);
            ui.horizontal_top(|ui| {
                side(ui, t, ours, &c.ours, t.accent, half, c.took == Took::Ours);
                ui.add_space(gap);
                side(
                    ui,
                    t,
                    theirs,
                    &c.theirs,
                    t.warning,
                    half,
                    c.took == Took::Theirs,
                );
            });
            if let Some(base) = &c.base {
                egui::CollapsingHeader::new(RichText::new("Base: what both started from").small())
                    .id_salt(("resolver-base", i))
                    .show(ui, |ui| code(ui, t, base, t.field, ui.available_width()));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                icons::inline(ui, "arrow-down", 13.0, t.success);
                ui.label(
                    RichText::new("The agent wrote")
                        .color(t.success)
                        .small()
                        .strong(),
                );
            });
            let text = if c.result.is_empty() {
                "(nothing)"
            } else {
                c.result.as_str()
            };
            code(ui, t, text, t.diff_add_bg, ui.available_width());
        });
}

/// One side of a conflict, under its name; `chosen` when the agent took it as it is.
fn side(ui: &mut Ui, t: &Tokens, name: &str, text: &str, color: Color32, width: f32, chosen: bool) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, color);
            ui.label(RichText::new(name).color(t.text_dim).small().strong());
            if chosen {
                ui.label(RichText::new("taken").color(color).small());
            }
        });
        let text = if text.is_empty() { "(nothing)" } else { text };
        code(ui, t, text, color.gamma_multiply(0.10), width);
    });
}

/// `text` in a monospaced block of its own, lines kept whole (scrolled sideways when long).
fn code(ui: &mut Ui, t: &Tokens, text: &str, fill: Color32, width: f32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(width - 16.0);
            egui::ScrollArea::horizontal()
                .id_salt(ui.next_auto_id())
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text.trim_end_matches('\n'))
                                .font(theme::mono(12.0))
                                .color(t.text),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}

/// A line diff: removed lines red, added green, unchanged runs folded.
fn diff_lines(ui: &mut Ui, t: &Tokens, lines: &[Line]) {
    egui::Frame::new()
        .fill(t.field)
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::symmetric(0, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::horizontal()
                .id_salt("resolver-diff")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for line in lines {
                        let (sign, text, fill, color) = match line {
                            Line::Same(s) => (" ", s.as_str(), Color32::TRANSPARENT, t.text_dim),
                            Line::Removed(s) => ("−", s.as_str(), t.diff_del_bg, t.text),
                            Line::Added(s) => ("+", s.as_str(), t.diff_add_bg, t.text),
                            Line::Skipped(n) => {
                                ui.add_space(3.0);
                                ui.label(
                                    RichText::new(format!("   ⋯ {n} unchanged lines"))
                                        .color(t.text_faint)
                                        .small(),
                                );
                                ui.add_space(3.0);
                                continue;
                            }
                        };
                        egui::Frame::new()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(10, 1))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{sign} {text}"))
                                            .font(theme::mono(12.0))
                                            .color(color),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend),
                                );
                            });
                    }
                });
        });
}
