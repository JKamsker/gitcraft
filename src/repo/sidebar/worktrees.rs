//! The repository's worktrees.

use egui::{Align2, Rect, Response, Ui, pos2, vec2};

use crate::a11y::labels;
use crate::git::Worktree;
use crate::icons;
use crate::menu_item::MenuItem;
use crate::repo::pinned_sections::{Button, Header, Sections};
use crate::repo::{Action, Dialog, Refresh, RepoTab, ops, widgets, worktree_tab};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn worktree_section(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        idle: bool,
        sections: &mut Sections,
    ) {
        let open = sections.add(
            ui,
            Header {
                key: "worktrees",
                icon: "folder-tree",
                title: "WORKTREES",
                count: self.worktrees.len(),
                default_open: true,
                button: Some(Button {
                    icon: "plus",
                    tooltip: "Add a worktree…",
                    action: idle.then(|| {
                        Action::Dialog(Dialog::add_worktree(&self.path, None, &self.refs.head))
                    }),
                }),
            },
        );
        if !open {
            return;
        }
        let same_tab = Settings::get(ui.ctx()).worktrees_in_same_tab;
        let any_prunable = self.worktrees.iter().any(|w| w.prunable.is_some());
        for wt in &self.worktrees {
            let fenced = self.fence.violations(&wt.path);
            let mut tip = worktree_tip(wt, same_tab);
            if !fenced.is_empty() {
                tip.push_str(&format!(
                    "\nTouched the agent fence; merging its branch asks first:\n{}",
                    crate::repo::fence_view::list_violations(fenced)
                ));
            } else if let Some(e) = self.fence.error(&wt.path) {
                tip.push_str(&format!(
                    "\n{e}\nUntil it can be, merging its branch asks first."
                ));
            }
            let r = worktree_row(ui, wt, self.fence.red(&wt.path)).on_hover_text(tip);
            let can_open = !wt.current && wt.prunable.is_none();
            if r.double_clicked() && can_open {
                acts.push(Action::OpenWorktree(wt.path.clone()));
            } else if r.clicked() && !wt.head.is_empty() {
                acts.push(Action::SelectHash(wt.head.clone()));
            }
            crate::menu_item::context(&r)
                .style(crate::theme::menu_style)
                .show(|ui| {
                    worktree_menu_items(ui, acts, wt, idle, same_tab, can_open, any_prunable);
                });
        }
    }
}

/// A worktree: its icon (a dot for uncommitted changes), name, branch and what is wrong with it;
/// red when it touched the agent fence (`fenced`).
fn worktree_row(ui: &mut Ui, wt: &Worktree, fenced: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    // Known by its folder, not its place: one added or removed in the terminal shifts the rows,
    // and a menu left open (Lock, Prune…) must stay this worktree's.
    let (rect, r) = widgets::keyed_row(ui, 36.0, ("worktree-row", &wt.path));
    widgets::a11y(&r, egui::WidgetType::Button, || {
        let notes = [
            (wt.current, "open here"),
            (wt.locked.is_some(), "locked"),
            (wt.prunable.is_some(), "prunable"),
        ];
        let path = crate::remote::display(&wt.path);
        labels::worktree(&path, wt.branch.as_deref(), wt.changes, &notes)
    });
    let painter = ui.painter_at(rect);
    if wt.current {
        painter.rect_filled(rect, 0.0, t.accent.gamma_multiply(0.14));
    }
    if fenced {
        painter.rect_filled(rect, 0.0, t.danger.gamma_multiply(0.16));
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
            0.0,
            t.danger,
        );
    }
    if r.hovered() {
        painter.rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;
    let icon_at = pos2(rect.left() + 34.0, y);
    if worktree_tab::changes_elsewhere(wt).is_some() {
        worktree_tab::paint_dirty(&painter, &t, icon_at);
    } else {
        let icon = if wt.current { "check" } else { "folder-git-2" };
        icons::paint(
            ui,
            Rect::from_center_size(icon_at, vec2(16.0, 16.0)),
            icon,
            14.0,
            if wt.current { t.accent } else { t.text_faint },
        );
    }
    let font = if wt.current {
        theme::semibold(12.5)
    } else {
        egui::FontId::proportional(12.5)
    };
    let mut right = rect.right() - 8.0;
    for (on, label, color) in [
        (fenced, "fence", t.danger),
        (wt.prunable.is_some(), "missing", t.danger),
        (wt.locked.is_some(), "locked", t.warning),
        (wt.main, "main", t.text_faint),
    ] {
        if on {
            let r = painter.text(
                pos2(right, y),
                Align2::RIGHT_CENTER,
                label,
                theme::medium(10.5),
                color,
            );
            right = r.left() - 6.0;
        }
    }
    let clip = painter.with_clip_rect(Rect::from_min_max(rect.min, pos2(right, rect.bottom())));
    clip.text(
        pos2(rect.left() + 48.0, y - 7.0),
        Align2::LEFT_CENTER,
        wt.name(),
        font,
        t.text,
    );
    let sub = match (&wt.branch, wt.bare) {
        (_, true) => "bare".to_owned(),
        (Some(branch), _) => branch.clone(),
        (None, _) => format!("detached at {}", wt.head.get(..8).unwrap_or(&wt.head)),
    };
    clip.text(
        pos2(rect.left() + 48.0, y + 8.0),
        Align2::LEFT_CENTER,
        sub,
        egui::FontId::proportional(11.0),
        t.text_faint,
    );
    r
}

/// The hover text of a worktree: where it is, what is going on in it, how to open it.
fn worktree_tip(wt: &Worktree, same_tab: bool) -> String {
    let mut tip = wt.path.display().to_string();
    if let Some(line) = worktree_tab::changes_line(wt) {
        tip.push_str(&format!("\n{line}"));
    }
    if let Some(reason) = wt.locked.as_deref().filter(|r| !r.is_empty()) {
        tip.push_str(&format!("\nLocked: {reason}"));
    }
    if let Some(reason) = &wt.prunable {
        tip.push_str(&format!("\nMissing: {reason}"));
    }
    if !wt.current {
        tip.push_str(if same_tab {
            "\nDouble-click to open"
        } else {
            "\nDouble-click to open in a tab"
        });
    }
    tip
}

fn worktree_menu_items(
    ui: &mut Ui,
    acts: &mut Vec<Action>,
    wt: &Worktree,
    idle: bool,
    same_tab: bool,
    can_open: bool,
    any_prunable: bool,
) {
    // As git (and a shell) on the worktree's machine names it.
    let path = crate::remote::git_path(&wt.path);
    if same_tab
        && crate::menu_item::MenuItem::new("Open")
            .icon("eye")
            .refused((!can_open).then_some(if wt.current {
                "This worktree is already open here"
            } else {
                "This worktree is missing"
            }))
            .show(ui)
            .clicked()
    {
        acts.push(Action::OpenWorktree(wt.path.clone()));
    }
    let new_tab = if same_tab {
        "Open in a new tab"
    } else {
        "Open in a tab"
    };
    if crate::menu_item::MenuItem::new(new_tab)
        .icon("command")
        .refused((!can_open).then_some(if wt.current {
            "This worktree is already open here"
        } else {
            "This worktree is missing"
        }))
        .show(ui)
        .clicked()
    {
        acts.push(Action::OpenTab(wt.path.clone()));
    }
    if crate::menu_item::MenuItem::new("Open terminal here")
        .icon("square-terminal")
        .show(ui)
        .clicked()
    {
        acts.push(Action::NewTerminal {
            shell: None,
            cwd: Some(wt.path.clone()),
        });
    }
    let openable = crate::remote::openable_here(&wt.path).is_ok();
    if crate::menu_item::MenuItem::new(crate::repo::reveal_label())
        .icon("folder-open")
        .refused((!openable).then_some("This worktree cannot be opened from this machine"))
        .show(ui)
        .clicked()
    {
        let _ = crate::remote::open_here(wt.path.clone());
    }
    if crate::menu_item::MenuItem::new("Copy path")
        .icon("copy")
        .show(ui)
        .clicked()
    {
        ui.ctx().copy_text(path.clone());
    }
    if crate::menu_item::MenuItem::new("Collision matrix…")
        .icon("command")
        .show(ui)
        .on_hover_text(
            "Which worktree branches would conflict when merged, and the order to land them in",
        )
        .clicked()
    {
        acts.push(Action::OpenCollisions);
    }
    if crate::menu_item::MenuItem::new("Agent fence…")
        .icon("command")
        .show(ui)
        .on_hover_text("Paths agents must stay out of, written into their configs and checked")
        .clicked()
    {
        acts.push(Action::OpenFence);
    }
    if !wt.main
        && crate::menu_item::MenuItem::new("Scope drift…")
            .icon("command")
            .refused(
                wt.head
                    .is_empty()
                    .then_some("This worktree has no commit yet"),
            )
            .show(ui)
            .on_hover_text(
                "What it changed beyond its task: folders, dependencies, signatures, formatting",
            )
            .clicked()
    {
        acts.push(Action::OpenDrift(wt.path.clone()));
    }
    if !wt.main {
        ui.separator();
        let (label, args) = if wt.locked.is_some() {
            ("Unlock", vec!["worktree", "unlock", path.as_str()])
        } else {
            ("Lock", vec!["worktree", "lock", path.as_str()])
        };
        if crate::menu_item::MenuItem::new(label)
            .icon("command")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Ops {
                label: format!("{label} worktree {}", wt.name()),
                ops: ops(&[&args]),
                refresh: Refresh::All,
            });
        }
        if MenuItem::new("Remove…")
            .icon("minus")
            .enabled(idle)
            .refused(
                wt.current
                    .then_some("Open another worktree to remove this one"),
            )
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::confirm(
                "Remove worktree",
                format!("Remove the worktree at {path}? Its folder is deleted; the branch stays."),
                crate::i18n::text("repo-core-0305"),
                ops(&[&["worktree", "remove", &path]]),
                Some((
                    "Force, even with uncommitted changes or locked".to_owned(),
                    ops(&[&["worktree", "remove", "--force", "--force", &path]]),
                )),
            )));
        }
    }
    if any_prunable {
        ui.separator();
        if crate::menu_item::MenuItem::new("Prune missing worktrees")
            .icon("minus")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Ops {
                label: "Prune worktrees".into(),
                ops: ops(&[&["worktree", "prune"]]),
                refresh: Refresh::All,
            });
        }
    }
}
