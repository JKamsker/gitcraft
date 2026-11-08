//! The window that edits the fence and writes it into the agents' configs.

use egui::RichText;

use super::super::{Action, RepoTab};
use crate::fence;
use crate::theme::{self, Tokens};

/// The list being edited, one entry per line.
pub(crate) struct FenceEditor {
    text: String,
    targets: fence::Targets,
    /// What saving said (an error when `true`).
    said: Option<(String, bool)>,
    /// Saving, or reading the saved list (nothing can be edited before it is in: saving an
    /// empty box would wipe it).
    saving: bool,
}

impl FenceEditor {
    fn loading() -> Self {
        Self {
            text: String::new(),
            targets: fence::Targets::default(),
            said: None,
            saving: true,
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
    /// Opens the window, with the list as it is saved now (read first).
    pub(crate) fn open_fence(&mut self, ctx: &egui::Context) {
        if self.fence.editor.is_some() {
            return;
        }
        self.fence.editor = Some(FenceEditor::loading());
        let repo = self.path.clone();
        self.then(
            ctx,
            move || fence::read(&repo).map(|list| (list, fence::read_targets(&repo))),
            |tab, read, _| {
                let Some(editor) = &mut tab.fence.editor else {
                    return;
                };
                editor.saving = false;
                match read {
                    Ok((list, targets)) => {
                        editor.text = list.join("\n");
                        editor.targets = targets;
                    }
                    // Nothing to edit from: closed, with why.
                    Err(e) => {
                        tab.fence.editor = None;
                        tab.status = Some((e, true));
                    }
                }
            },
        );
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
        let targets = editor.targets;
        let roots: Vec<std::path::PathBuf> = self
            .worktrees
            .iter()
            .filter(|w| !w.bare && w.prunable.is_none())
            .map(|w| w.path.clone())
            .collect();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || save(&repo, &entries, targets, &roots),
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
                (save, close) = body(ui, editor, &self.fence.write_errors);
            });
        if close || (response.should_close() && !editor.saving) {
            self.fence.editor = None;
        } else if save {
            self.save_fence(ctx);
        }
    }
}

fn body(ui: &mut egui::Ui, editor: &mut FenceEditor, write_errors: &[String]) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(crate::i18n::text("repo-workflows-fence_view-body-803f082f"))
            .font(theme::semibold(15.0)),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-fence_view-body-d1953461",
            &[("fence-claude-1", (fence::CLAUDE).clone().into())],
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
    ui.add_space(6.0);
    ui.add_enabled_ui(!editor.saving, |ui| {
        ui.checkbox(
            &mut editor.targets.cursor,
            crate::i18n::message(
                "repo-workflows-fence_view-body-c408ae8c",
                &[("fence-cursor-1", (fence::CURSOR).clone().into())],
            ),
        )
        .on_hover_text(crate::i18n::text("repo-workflows-fence_view-body-d10df00e"));
        ui.checkbox(
            &mut editor.targets.agents_md,
            crate::i18n::message(
                "repo-workflows-fence_view-body-98912c89",
                &[("fence-agents-1", (fence::AGENTS).clone().into())],
            ),
        )
        .on_hover_text(crate::i18n::text("repo-workflows-fence_view-body-b31ba75f"));
    });
    ui.add_space(8.0);
    let mut save = false;
    let mut close = false;
    ui.horizontal(|ui| {
        if crate::repo::primary_button(
            ui,
            crate::i18n::text("repo-workflows-fence_view-body-a8aa1101"),
            !editor.saving,
            190.0,
        )
        .clicked()
        {
            save = true;
        }
        if ui
            .add_enabled(
                !editor.saving,
                egui::Button::new(crate::i18n::text("repo-workflows-fence_view-body-b42a9a6f")),
            )
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
                .add_enabled(
                    !editor.saving,
                    egui::Button::new(crate::i18n::text("repo-workflows-fence_view-body-7d9eb7ac")),
                )
                .clicked();
        });
    });
    if !write_errors.is_empty() {
        ui.add_space(8.0);
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-fence_view-body-cac5a24a",
                &[(
                    "write-errors-join-n-1",
                    (write_errors.join("\n")).clone().into(),
                )],
            ))
            .size(12.0)
            .color(t.danger),
        );
    }
    if let Some((said, error)) = &editor.said {
        ui.add_space(8.0);
        let color = if *error { t.danger } else { t.text_dim };
        ui.label(
            RichText::new(if *error {
                fence::display::diagnostic(said)
            } else {
                said.clone()
            })
            .size(12.0)
            .color(color),
        );
    }
    (save, close)
}

/// Saves `entries` as the list of `repo` (and `targets`) and writes them into the agents'
/// configs of each of `roots` (the worktrees). What to tell, and whether it is an error.
fn save(
    repo: &std::path::Path,
    entries: &[String],
    targets: fence::Targets,
    roots: &[std::path::PathBuf],
) -> (String, bool) {
    if let Err(e) = fence::write(repo, entries).and_then(|()| fence::write_targets(repo, targets)) {
        return (e, true);
    }
    let new = fence::parse_list(entries);
    let mut written = 0;
    let mut errors = Vec::new();
    for root in roots {
        match fence::write_all(root, &new, targets) {
            Ok(files) => written += files.len(),
            Err(e) => errors.push(format!("{}: {e}", crate::remote::display(root))),
        }
    }
    if errors.is_empty() {
        let n = new.len();
        (
            crate::i18n::message(
                "repo-workflows-fence-saved-summary",
                &[
                    ("entries", (n).into()),
                    ("files", (written).into()),
                    ("worktrees", (roots.len()).into()),
                ],
            ),
            false,
        )
    } else {
        (
            crate::i18n::message(
                "repo-workflows-fence_view-save-7bfa9a3b",
                &[("errors-join-n-1", (errors.join("\n")).clone().into())],
            ),
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
    save(
        repo,
        entries,
        fence::Targets {
            cursor: true,
            agents_md: true,
        },
        roots,
    )
}
