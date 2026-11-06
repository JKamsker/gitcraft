//! _This branch is based on…_: picks the branch a branch is a layer on, the one history
//! suggests chosen first.

use egui::{Key, RichText};

use super::StackAct;
use crate::git::stack;
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// The dialog, for one branch.
pub(crate) struct BasedOn {
    pub branch: String,
    /// The parent picked (empty: none yet).
    pub choice: String,
    /// What history suggests: `None` while it is being looked for, then the branch (if any),
    /// or why history couldn't be read.
    pub suggestion: Option<Result<Option<String>, String>>,
    /// Which dialog this is: a suggestion found for one closed since isn't taken.
    id: u64,
}

impl RepoTab {
    /// Opens the dialog for `branch`, its recorded parent picked, and looks for a suggestion.
    pub(super) fn open_based_on(&mut self, branch: String, ctx: &egui::Context) {
        static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // A recorded parent that is gone isn't offered: the suggestion takes its place.
        let choice = self
            .stacks
            .recorded
            .get(&branch)
            .map(|r| r.parent.clone())
            .filter(|p| self.refs.local.iter().any(|b| b.name == *p))
            .unwrap_or_default();
        self.stacks.dialog = Some(BasedOn {
            branch: branch.clone(),
            choice,
            suggestion: None,
            id,
        });
        let path = self.path.clone();
        let recorded = self.stacks.recorded.clone();
        self.then(
            ctx,
            move || stack::suggest_parent(&path, &branch, &recorded),
            move |tab, found, _| {
                if let Some(dialog) = tab.stacks.dialog.as_mut().filter(|d| d.id == id) {
                    if dialog.choice.is_empty()
                        && let Ok(Some(name)) = &found
                    {
                        dialog.choice = name.clone();
                    }
                    dialog.suggestion = Some(found);
                }
            },
        );
    }

    /// The branches `branch` could be based on: every other local branch but those based on
    /// it.
    fn parent_options(&self, branch: &str) -> Vec<String> {
        self.refs
            .local
            .iter()
            .map(|b| b.name.clone())
            .filter(|name| !stack::would_loop(&self.stacks.recorded, branch, name))
            .collect()
    }

    /// Draws the dialog, when it is open.
    pub(in crate::repo) fn stack_dialogs(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(dialog) = &self.stacks.dialog else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let options = self.parent_options(&dialog.branch);
        let current = self.stacks.recorded.get(&dialog.branch);
        let idle = self.busy_op.is_none();
        let response = egui::Modal::new(egui::Id::new("gitcraft-stack-based-on"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(
                    RichText::new(format!("{} is based on…", dialog.branch))
                        .font(theme::semibold(15.0)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "The branch it is a layer on in a stack: STACKS then shows the commits \
                         it adds, and warns when that branch moves on.",
                    )
                    .color(t.text_dim),
                );
                ui.add_space(10.0);
                let mut picked = None;
                egui::ComboBox::from_id_salt("stack-parent")
                    .width(ui.available_width())
                    .selected_text(if dialog.choice.is_empty() {
                        "Pick a branch"
                    } else {
                        dialog.choice.as_str()
                    })
                    .show_ui(ui, |ui| {
                        for name in &options {
                            if ui.selectable_label(*name == dialog.choice, name).clicked() {
                                picked = Some(name.clone());
                            }
                        }
                    });
                if let Some(name) = picked {
                    acts.push(Action::Stack(StackAct::PickParent(name)));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| match &dialog.suggestion {
                    None => {
                        ui.add(crate::spinner::Spinner::new());
                        ui.label(RichText::new("Looking at history…").color(t.text_faint));
                    }
                    Some(Ok(Some(name))) => {
                        ui.label(
                            RichText::new(format!("Suggested from history: {name}"))
                                .color(t.text_faint),
                        );
                        if *name != dialog.choice && ui.small_button("Use").clicked() {
                            acts.push(Action::Stack(StackAct::PickParent(name.clone())));
                        }
                    }
                    Some(Ok(None)) => {
                        ui.label(
                            RichText::new("No other branch shares history with it.")
                                .color(t.text_faint),
                        );
                    }
                    Some(Err(e)) => {
                        ui.label(
                            RichText::new(format!("Couldn't look at history: {e}"))
                                .color(t.warning),
                        );
                    }
                });
                if let Some(r) = current {
                    let gone = !self.refs.local.iter().any(|b| b.name == r.parent);
                    let (text, color) = if gone {
                        (
                            format!("Now based on {}, which is gone", r.parent),
                            t.warning,
                        )
                    } else {
                        (format!("Now based on {}", r.parent), t.text_faint)
                    };
                    ui.label(RichText::new(text).color(color));
                }
                ui.add_space(14.0);
                let mut done = false;
                // Enter picks in the list while it is open, and sets the parent only after.
                let enter =
                    !egui::Popup::is_any_open(ui.ctx()) && ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    let can = idle && !dialog.choice.is_empty();
                    if widgets::primary_button(ui, "Set parent", can, 90.0).clicked()
                        || (can && enter)
                    {
                        done = true;
                    }
                    if ui
                        .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
                        .clicked()
                    {
                        acts.push(Action::Stack(StackAct::CloseDialog));
                    }
                });
                if done {
                    acts.push(Action::Stack(StackAct::SetParent {
                        branch: dialog.branch.clone(),
                        parent: dialog.choice.clone(),
                    }));
                }
            });
        if response.should_close() {
            acts.push(Action::Stack(StackAct::CloseDialog));
        }
    }
}
