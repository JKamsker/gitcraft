//! The AGENTS section of the left pane: the repository's agent tasks, each with its worktree's
//! uncommitted changes, to open and review with the normal diff.

use egui::{Align2, Rect, Ui, pos2, vec2};

use super::AgentAct;
use crate::agent::AgentTask;
use crate::agent::tasks::agent_label;
use crate::git::Worktree;
use crate::icons;
use crate::repo::pinned_sections::{Button, Header, Sections};
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// A task as the list shows it, with its worktree when it is still there.
struct Row<'a> {
    task: &'a AgentTask,
    worktree: Option<&'a Worktree>,
}

impl Row<'_> {
    /// What the line under the branch says: the agent, and the worktree's changes.
    fn sub(&self) -> String {
        let agent = agent_label(&self.task.agent);
        let state = match self.worktree {
            None => crate::i18n::text("repo-workflows-agents-sub-25ba7976").to_owned(),
            Some(w) if w.prunable.is_some() => {
                crate::i18n::text("repo-workflows-agents-sub-96b72d81").to_owned()
            }
            Some(w) => match w.changes {
                Some(0) => crate::i18n::text("repo-workflows-agents-sub-b2c35ddf").to_owned(),
                Some(1) => crate::i18n::text("repo-workflows-agents-sub-32dfcdbc").to_owned(),
                Some(n) => crate::i18n::message(
                    "repo-workflows-agents-sub-a7109ae6",
                    &[("n", (n).clone().into())],
                ),
                None => crate::i18n::text("repo-workflows-agents-sub-308560dc").to_owned(),
            },
        };
        format!("{agent} · {state}")
    }

    fn openable(&self) -> bool {
        self.worktree
            .is_some_and(|w| !w.current && w.prunable.is_none())
    }
}

impl RepoTab {
    pub(in crate::repo) fn agent_section(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        idle: bool,
        sections: &mut Sections,
    ) {
        let open = sections.add(
            ui,
            Header {
                key: "agents",
                icon: "square-terminal",
                title: crate::i18n::text("repo-workflows-agents-agent-section-370f346f"),
                count: self.agents.tasks.len(),
                default_open: true,
                button: Some(Button {
                    icon: "plus",
                    tooltip: crate::i18n::text("repo-workflows-agents-agent-section-d3a1dde8"),
                    action: idle.then_some(Action::Agent(AgentAct::OpenStart)),
                }),
            },
        );
        if !open {
            return;
        }
        let t = Tokens::get(ui.ctx());
        if let Some(e) = &self.agents.error {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                ui.colored_label(t.danger, e);
            });
        }
        if self.agents.tasks.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(crate::i18n::text(
                        "repo-workflows-agents-agent-section-736dbccf",
                    ))
                    .small()
                    .color(t.text_faint),
                );
            });
            return;
        }
        for task in &self.agents.tasks {
            let worktree = self
                .worktrees
                .iter()
                .find(|w| same_path(&w.path, &task.path));
            let row = Row { task, worktree };
            let r = paint_row(ui, &row, &t);
            let r = r.on_hover_ui(|ui| {
                ui.label(tip(&row));
            });
            if r.clicked() && row.openable() {
                acts.push(Action::OpenWorktree(std::path::PathBuf::from(&task.path)));
            }
            crate::menu_item::context(&r)
                .style(theme::menu_style)
                .show(|ui| menu(ui, &row, acts));
        }
        if self.agents.tasks.len() >= 2 {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui
                    .small_button(crate::i18n::text(
                        "repo-workflows-agents-agent-section-c37ddff4",
                    ))
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-agents-agent-section-a6153a60",
                    ))
                    .clicked()
                {
                    acts.push(Action::Bake(crate::repo::bakeoff::BakeAct::OpenPicker));
                }
            });
        }
    }
}

/// Whether `listed` (a task's folder, as git listed it when it was made) is the worktree at
/// `path`: compared as written (a `/` at the end aside), every frame, without asking the disk.
fn same_path(path: &std::path::Path, listed: &str) -> bool {
    path == std::path::Path::new(listed)
}

fn paint_row(ui: &mut Ui, row: &Row, t: &Tokens) -> egui::Response {
    let (rect, r) = widgets::keyed_row(ui, 36.0, ("agent-row", &row.task.path));
    let painter = ui.painter_at(rect);
    let current = row.worktree.is_some_and(|w| w.current);
    if current {
        painter.rect_filled(rect, 0.0, t.accent.gamma_multiply(0.14));
    }
    if r.hovered() {
        painter.rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;
    let dirty = row.worktree.and_then(|w| w.changes).is_some_and(|n| n > 0);
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 34.0, y), vec2(16.0, 16.0)),
        "square-terminal",
        14.0,
        if dirty { t.warning } else { t.text_faint },
    );
    let clip = painter.with_clip_rect(rect.shrink2(vec2(8.0, 0.0)));
    clip.text(
        pos2(rect.left() + 48.0, y - 7.0),
        Align2::LEFT_CENTER,
        &row.task.branch,
        if current {
            theme::semibold(12.5)
        } else {
            egui::FontId::proportional(12.5)
        },
        t.text,
    );
    let gone = row.worktree.is_none_or(|w| w.prunable.is_some());
    clip.text(
        pos2(rect.left() + 48.0, y + 8.0),
        Align2::LEFT_CENTER,
        row.sub(),
        egui::FontId::proportional(11.0),
        if gone { t.danger } else { t.text_faint },
    );
    r
}

fn tip(row: &Row) -> String {
    let mut tip = crate::remote::display(std::path::Path::new(&row.task.path));
    if !row.task.prompt.is_empty() {
        tip.push_str(&crate::i18n::message(
            "repo-workflows-agents-tip-7d8e5553",
            &[("row-task-prompt-1", (row.task.prompt).clone().into())],
        ));
    }
    tip.push_str(&crate::i18n::message(
        "repo-workflows-agents-tip-95d5fbf7",
        &[("row-task-agent-1", (row.task.agent).clone().into())],
    ));
    if row.openable() {
        tip.push_str(crate::i18n::text("repo-workflows-agents-tip-5c1b8dd5"));
    } else if row.worktree.is_some_and(|w| w.current) {
        tip.push_str(crate::i18n::text("repo-workflows-agents-tip-1c0ba476"));
    }
    tip
}

fn menu(ui: &mut Ui, row: &Row, acts: &mut Vec<Action>) {
    let path = std::path::PathBuf::from(&row.task.path);
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-workflows-agents-menu-13b97894"))
        .icon("eye")
        .refused(
            (!row.openable()).then_some(crate::i18n::text("repo-workflows-agents-menu-b1e3c6b9")),
        )
        .show(ui)
        .clicked()
    {
        acts.push(Action::OpenWorktree(path.clone()));
    }
    let there = row.worktree.is_some_and(|w| w.prunable.is_none());
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-workflows-agents-menu-cbc89e47"))
        .icon("command")
        .refused((!there).then_some(crate::i18n::text("repo-workflows-agents-menu-11b6670c")))
        .show(ui)
        .on_hover_text(crate::i18n::text("repo-workflows-agents-menu-d4604b9b"))
        .clicked()
    {
        acts.push(Action::Agent(AgentAct::Restart(row.task.clone())));
    }
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-workflows-agents-menu-04eb42a7"))
        .icon("square-terminal")
        .refused((!there).then_some(crate::i18n::text("repo-workflows-agents-menu-11b6670c")))
        .show(ui)
        .clicked()
    {
        acts.push(Action::NewTerminal {
            shell: None,
            cwd: Some(path),
        });
    }
    ui.separator();
    if crate::menu_item::MenuItem::new(crate::i18n::text("repo-workflows-agents-menu-7b4c6a5c"))
        .icon("minus")
        .show(ui)
        .on_hover_text(crate::i18n::text("repo-workflows-agents-menu-24b65605"))
        .clicked()
    {
        acts.push(Action::Agent(AgentAct::Forget(row.task.path.clone())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_says_how_much_its_worktree_changed() {
        let task = AgentTask {
            agent: "codex --full-auto".to_owned(),
            ..AgentTask::default()
        };
        let mut wt = Worktree {
            changes: Some(3),
            ..Worktree::default()
        };
        assert_eq!(
            Row {
                task: &task,
                worktree: Some(&wt)
            }
            .sub(),
            "Codex · 3 files changed"
        );
        wt.changes = Some(1);
        assert_eq!(
            Row {
                task: &task,
                worktree: Some(&wt)
            }
            .sub(),
            "Codex · 1 file changed"
        );
        wt.changes = None;
        assert!(
            Row {
                task: &task,
                worktree: Some(&wt)
            }
            .sub()
            .ends_with("counting changes…")
        );
        assert_eq!(
            Row {
                task: &task,
                worktree: None
            }
            .sub(),
            "Codex · worktree removed"
        );
        assert!(
            !Row {
                task: &task,
                worktree: None
            }
            .openable()
        );
        wt.current = true;
        assert!(
            !Row {
                task: &task,
                worktree: Some(&wt)
            }
            .openable(),
            "already shown"
        );
    }
}
