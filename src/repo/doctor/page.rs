//! The doctor's page: its title bar, the sections listed on the left as in Settings, the one
//! chosen beside them; and the cards the sections are made of.

use egui::{Align2, RichText, Sense, Stroke, Ui, vec2};

use super::{DoctorAct, DoctorView, Section};
use crate::icons;
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// Width of the list of sections, and the room between it and the section.
const NAV_WIDTH: f32 = 210.0;
const GAP: f32 = 28.0;
/// Widest a section gets.
const CONTENT_WIDTH: f32 = 860.0;

impl RepoTab {
    /// The big area, while the doctor is open.
    pub(in crate::repo) fn doctor_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        self.doctor_poll(&ctx);
        self.hooks_poll();
        let busy = self.busy_op.is_some();
        // Those whose config can be read: not bare, not gone.
        let worktrees = self
            .worktrees
            .iter()
            .filter(|w| !w.bare && w.prunable.is_none())
            .count();
        let loading = self.loading_facts();
        let env = TabEnv {
            loading,
            worktrees,
            remote: crate::remote::is_remote(&self.path),
            top: crate::remote::git_path(&self.path),
            busy,
        };
        let Some(doctor) = self.doctor.as_deref_mut() else {
            return;
        };
        widgets::panel_header(
            ui,
            crate::i18n::text("repo-workflows-doctor-doctor-ui-3095e533"),
            |ui| {
                if icons::small_button(
                    ui,
                    "x",
                    crate::i18n::text("repo-workflows-doctor-doctor-ui-8c6c9cf3"),
                )
                .clicked()
                {
                    acts.push(Action::Doctor(DoctorAct::Close));
                }
                if doctor.waiting || busy {
                    ui.add(crate::spinner::Spinner::new());
                } else if icons::small_button(
                    ui,
                    "refresh-cw",
                    crate::i18n::text("repo-workflows-doctor-doctor-ui-d8cb92fa"),
                )
                .clicked()
                {
                    acts.push(Action::Doctor(DoctorAct::Reload));
                }
            },
        );
        confirm_modal(ui, doctor, acts);
        ui.horizontal_top(|ui| {
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.set_width(NAV_WIDTH);
                nav(ui, doctor.section, acts);
            });
            ui.add_space(GAP);
            egui::ScrollArea::vertical()
                .id_salt(("doctor", doctor.section as u8))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let width = CONTENT_WIDTH.min(ui.available_width() - 16.0).max(240.0);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.add_space(22.0);
                        title(ui, doctor.section.title());
                        section_ui(ui, doctor, &env, acts);
                        ui.add_space(40.0);
                    });
                });
        });
    }
}

/// What the sections need to know about the tab.
struct TabEnv {
    /// How its reads went.
    loading: super::health::Loading,
    /// Worktrees whose config can be read.
    worktrees: usize,
    /// The repository is in WSL or over SSH.
    remote: bool,
    /// The repository's top folder, as git names it.
    top: String,
    /// An operation of the tab is running.
    busy: bool,
}

fn section_ui(ui: &mut Ui, doctor: &mut DoctorView, env: &TabEnv, acts: &mut Vec<Action>) {
    let locked = env.busy || doctor.waiting;
    let (worktrees, remote, top) = (env.worktrees, env.remote, env.top.as_str());
    match doctor.section {
        Section::Ignore => super::ignore_section::ui(ui, &mut doctor.ignore, top, locked, acts),
        Section::Health => {
            super::health_section::ui(ui, &doctor.health, &env.loading, locked, acts)
        }
        Section::Hooks => super::hooks_section::ui(ui, &doctor.hooks, locked, acts),
        Section::Config => {
            let env = super::config_section::Env::new(worktrees, remote, locked);
            super::config_section::ui(ui, &mut doctor.config, env, acts)
        }
        Section::Flags => super::flags_section::ui(ui, &mut doctor.flags, locked, acts),
    }
}

/// The question asked before a change that can't be undone here.
fn confirm_modal(ui: &mut Ui, doctor: &DoctorView, acts: &mut Vec<Action>) {
    if doctor.closing {
        let file = doctor
            .ignore
            .editing
            .as_ref()
            .map(|e| e.file.label.clone())
            .unwrap_or_default();
        ask(
            ui,
            crate::i18n::text("repo-workflows-doctor-confirm-modal-3b13192b"),
            &crate::i18n::message(
                "repo-workflows-doctor-confirm-modal-c324130d",
                &[("file", (file).clone().into())],
            ),
            crate::i18n::text("repo-workflows-doctor-confirm-modal-eb1a70e3"),
            crate::i18n::text("repo-workflows-doctor-confirm-modal-e76fd2ad"),
            |yes| Action::Doctor(DoctorAct::AnswerClose(yes)),
            acts,
        );
        return;
    }
    let Some(confirm) = &doctor.confirm else {
        return;
    };
    let (title, body, button) = confirm.displayed();
    ask(
        ui,
        &title,
        &body,
        &button,
        crate::i18n::text("repo-workflows-doctor-confirm-modal-19766ed6"),
        |yes| Action::Doctor(DoctorAct::Answer(yes)),
        acts,
    );
}

/// A question in a modal: `answer(true)` for its button, `answer(false)` for the other one or
/// Esc.
pub(super) fn ask(
    ui: &mut Ui,
    title: &str,
    body: &str,
    button: &str,
    cancel: &str,
    answer: impl Fn(bool) -> Action,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::same(18));
    let resp = egui::Modal::new(egui::Id::new("doctor-confirm"))
        .frame(frame)
        .show(ui.ctx(), |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new(title).font(theme::semibold(15.0)));
            ui.add_space(8.0);
            ui.add(egui::Label::new(RichText::new(body).color(t.text_dim)).wrap());
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if widgets::primary_button(ui, button, true, 90.0).clicked() {
                    acts.push(answer(true));
                }
                if ui.button(cancel).clicked() {
                    acts.push(answer(false));
                }
            });
        });
    if resp.should_close() {
        acts.push(answer(false));
    }
}

fn title(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(text)
            .font(theme::semibold(20.0))
            .color(t.text),
    );
    ui.add_space(4.0);
}

/// The sections, the one shown marked.
fn nav(ui: &mut Ui, shown: Section, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(22.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    for section in Section::ALL {
        let selected = section == shown;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        crate::a11y::a11y_selected(&resp, section.title(), selected);
        let fill = if selected {
            t.hover
        } else if resp.hovered() {
            t.card
        } else {
            egui::Color32::TRANSPARENT
        };
        let painter = ui.painter();
        painter.rect_filled(rect, t.radius, fill);
        if selected {
            let bar = egui::Rect::from_min_size(
                rect.left_top() + vec2(0.0, 7.0),
                vec2(3.0, rect.height() - 14.0),
            );
            painter.rect_filled(bar, 1.5, t.accent);
        }
        let (text, icon) = if selected {
            (t.text, t.accent)
        } else {
            (t.text_dim, t.icon)
        };
        let icon_rect =
            egui::Rect::from_center_size(rect.left_center() + vec2(22.0, 0.0), vec2(16.0, 16.0));
        icons::paint(ui, icon_rect, section.icon(), 16.0, icon);
        let font = if selected {
            theme::medium(13.0)
        } else {
            egui::FontId::proportional(13.0)
        };
        ui.painter().text(
            rect.left_center() + vec2(40.0, 0.0),
            Align2::LEFT_CENTER,
            section.title(),
            font,
            text,
        );
        if resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && !selected
        {
            acts.push(Action::Doctor(DoctorAct::Show(section)));
        }
    }
}

/// A titled card: the title, a line about it, and its contents in a frame.
pub(super) fn card(ui: &mut Ui, title: &str, note: Option<&str>, add: impl FnOnce(&mut Ui)) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(16.0);
    ui.label(
        RichText::new(title)
            .font(theme::semibold(14.0))
            .color(t.text),
    );
    if let Some(note) = note {
        ui.add_space(2.0);
        ui.add(egui::Label::new(RichText::new(note).color(t.text_faint)).wrap());
    }
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.separator))
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// What a section shows while it is read, or why it couldn't be.
pub(super) fn reading<'a, T>(ui: &mut Ui, read: &'a super::Read<T>) -> Option<&'a T> {
    let t = Tokens::get(ui.ctx());
    match read {
        None => {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
            None
        }
        Some(Err(e)) => {
            ui.add_space(12.0);
            ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
            None
        }
        Some(Ok(found)) => Some(found),
    }
}

/// Text in a muted colour, wrapped.
pub(super) fn faint(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).color(t.text_faint)).wrap());
}

/// Text in the dimmed colour, wrapped.
pub(super) fn dim(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).color(t.text_dim)).wrap());
}

/// A path or a value, in the code font.
pub(super) fn code(text: &str) -> RichText {
    RichText::new(text).font(theme::mono(12.0))
}
