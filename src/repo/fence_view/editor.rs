//! The window that edits the fence and writes it into the agents' configs.

use egui::RichText;

use super::super::{Action, RepoTab};
use crate::fence;
use crate::theme::{self, Tokens};

/// The list being edited, one entry per line.
pub(crate) struct FenceEditor {
    text: String,
    /// What saving said (an error when `true`).
    said: Option<(String, bool)>,
    saving: bool,
}

impl FenceEditor {
    fn new(list: &[String]) -> Self {
        Self {
            text: list.join("\n"),
            said: None,
            saving: false,
        }
    }

    fn entries(&self) -> Vec<String> {
        self.text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// Adds the suggested entries not in the list yet.
    fn suggest(&mut self) {
        let have = self.entries();
        for entry in fence::SUGGESTED {
            if !have.iter().any(|h| h == entry) {
                if !self.text.is_empty() && !self.text.ends_with('\n') {
                    self.text.push('\n');
                }
                self.text.push_str(entry);
            }
        }
    }
}

impl RepoTab {
    pub(crate) fn open_fence(&mut self) {
        if self.fence.editor.is_none() {
            self.fence.editor = Some(FenceEditor::new(&self.fence.list));
        }
    }

    /// Saves the list, writes it into the agents' configs of every worktree, and checks the
    /// worktrees again.
    fn save_fence(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.fence.editor else {
            return;
        };
        editor.saving = true;
        editor.said = None;
        let entries = editor.entries();
        let old = fence::parse_list(&self.fence.list);
        let roots: Vec<std::path::PathBuf> = self
            .worktrees
            .iter()
            .filter(|w| !w.bare && w.prunable.is_none())
            .map(|w| w.path.clone())
            .collect();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || save(&repo, &entries, &old, &roots),
            |tab, said, ctx| {
                if let Some(editor) = &mut tab.fence.editor {
                    editor.saving = false;
                    editor.said = Some(said);
                }
                tab.check_fence(ctx);
            },
        );
    }

    /// The window, while it is open.
    pub(crate) fn fence_window(&mut self, ctx: &egui::Context, _acts: &mut Vec<Action>) {
        let Some(editor) = &mut self.fence.editor else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut save = false;
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("gitcraft-fence"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(480.0);
                (save, close) = body(ui, editor);
            });
        if close || (response.should_close() && !editor.saving) {
            self.fence.editor = None;
        } else if save {
            self.save_fence(ctx);
        }
    }
}

fn body(ui: &mut egui::Ui, editor: &mut FenceEditor) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("Agent fence").font(theme::semibold(15.0)));
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "Paths agents must stay out of, one per line, as in .gitignore (secrets/, .env, \
             *.pem, /Cargo.lock). Saved in this repository's git config, shared by its \
             worktrees, and written into each worktree's agent configs: Claude Code's deny \
             rules ({}), {}, and a section of {} (Codex). Rules can leak, so every linked \
             worktree is checked too: one whose changes, untracked or ignored files touch the \
             fence turns red, and merging its branch needs a confirmation.",
            fence::CLAUDE,
            fence::CURSOR,
            fence::AGENTS
        ))
        .size(12.0)
        .color(t.text_dim),
    );
    ui.add_space(8.0);
    ui.add_enabled(
        !editor.saving,
        egui::TextEdit::multiline(&mut editor.text)
            .font(theme::mono(12.0))
            .desired_rows(8)
            .desired_width(f32::INFINITY)
            .hint_text(".env\nsecrets/\n.github/workflows/"),
    );
    ui.add_space(8.0);
    let mut save = false;
    let mut close = false;
    ui.horizontal(|ui| {
        if crate::repo::primary_button(ui, "Save and write to agents", !editor.saving, 190.0)
            .clicked()
        {
            save = true;
        }
        if ui
            .add_enabled(!editor.saving, egui::Button::new("Add suggestions"))
            .on_hover_text(fence::SUGGESTED.join("  "))
            .clicked()
        {
            editor.suggest();
        }
        if editor.saving {
            ui.add(crate::spinner::Spinner::new());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = ui
                .add_enabled(!editor.saving, egui::Button::new("Close"))
                .clicked();
        });
    });
    if let Some((said, error)) = &editor.said {
        ui.add_space(8.0);
        let color = if *error { t.danger } else { t.text_dim };
        ui.label(RichText::new(said).size(12.0).color(color));
    }
    (save, close)
}

/// Saves `entries` as the list of `repo` and writes them into the agents' configs of each of
/// `roots` (the worktrees), `old` being what was written before. What to tell, and whether
/// it is an error.
fn save(
    repo: &std::path::Path,
    entries: &[String],
    old: &[fence::Pattern],
    roots: &[std::path::PathBuf],
) -> (String, bool) {
    if let Err(e) = fence::write(repo, entries) {
        return (e, true);
    }
    let new = fence::parse_list(entries);
    let mut written = 0;
    let mut errors = Vec::new();
    for root in roots {
        match fence::write_all(root, old, &new) {
            Ok(files) => written += files.len(),
            Err(e) => errors.push(format!("{}: {e}", crate::remote::display(root))),
        }
    }
    if errors.is_empty() {
        let n = new.len();
        (
            format!(
                "Saved {n} entr{}; {written} agent config file{} written in {} worktree{}.",
                if n == 1 { "y" } else { "ies" },
                if written == 1 { "" } else { "s" },
                roots.len(),
                if roots.len() == 1 { "" } else { "s" }
            ),
            false,
        )
    } else {
        (
            format!("Saved, but not everywhere:\n{}", errors.join("\n")),
            true,
        )
    }
}

#[cfg(test)]
pub(super) fn save_for_test(
    repo: &std::path::Path,
    entries: &[String],
    roots: &[std::path::PathBuf],
) -> (String, bool) {
    save(repo, entries, &[], roots)
}
