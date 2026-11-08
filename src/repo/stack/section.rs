//! The sidebar's STACKS: each stack's branches bottom to top, under its title, with what
//! each layer adds, its pull request and whether it needs restacking; and the entries the
//! branch menu gets for stacks.

use egui::{Align2, Rect, Response, Ui, pos2, vec2};

use super::{StackAct, Tree, dependents, looped, to_restack, trees};
use crate::git::Branch;
use crate::git::stack::Layer;
use crate::icons;
use crate::repo::actions::{BranchRef, compare_branches_action};
use crate::repo::pinned_sections::{Header, Sections};
use crate::repo::sidebar::ROW_H;
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// How far in a layer is per layer below it.
const INDENT: f32 = 14.0;

impl RepoTab {
    /// The section, when the repository has any stack.
    pub(in crate::repo) fn stack_section(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        idle: bool,
        filter: &str,
        sections: &mut Sections,
    ) {
        let stacks = trees(&self.stacks.layers);
        let looped = looped(&self.stacks.layers);
        let pending = self.stacks.pending.as_ref();
        if stacks.is_empty()
            && looped.is_empty()
            && self.stacks.error.is_none()
            && pending.is_none()
        {
            return;
        }
        let open = sections.add(
            ui,
            Header {
                key: "stacks",
                icon: "rows-2",
                title: "STACKS",
                count: stacks.len(),
                default_open: true,
                button: None,
            },
        );
        if !open {
            return;
        }
        let t = Tokens::get(ui.ctx());
        if let Some(e) = &self.stacks.error {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                ui.colored_label(t.danger, e);
            });
        }
        self.restack_banner(ui, idle, acts);
        // Filtered, a stack shows only its layers that match, under its title.
        let shown = |l: &Layer| filter.is_empty() || l.branch.to_lowercase().contains(filter);
        for tree in &stacks {
            if !tree.rows.iter().any(|(_, l)| shown(l)) {
                continue;
            }
            let r = title_row(ui, tree, self.forge().map(|f| f.noun()));
            if let Some(forge) = self.forge() {
                crate::menu_item::context(&r)
                    .style(crate::theme::menu_style)
                    .show(|ui| {
                        if crate::menu_item::MenuItem::new(format!(
                            "Publish its {}s…",
                            forge.noun()
                        ))
                        .icon("external-link")
                        .enabled(idle)
                        .show(ui)
                        .on_hover_text(
                            "Preview each one's base and patch, then open or update them bottom-up",
                        )
                        .clicked()
                        {
                            acts.push(Action::Stack(StackAct::OpenPublish(tree.root.clone())));
                        }
                    });
            }
            for (depth, layer) in tree.rows.iter().filter(|(_, l)| shown(l)) {
                let r = self.layer_row(ui, layer, *depth);
                self.layer_clicks(r, layer, idle, acts);
            }
        }
        if !looped.is_empty() {
            self.loop_row(ui, &looped, idle, acts);
        }
    }

    /// Above the stacks: the restack that stopped, with Continue and Abort; else how many
    /// branches need updating, with Restack….
    fn restack_banner(&self, ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let frame = egui::Frame::new()
            .fill(t.warning.gamma_multiply(0.14))
            .corner_radius(t.radius)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .outer_margin(egui::Margin::symmetric(8, 4));
        if let Some(pending) = &self.stacks.pending {
            let branch = pending.current().map_or("", |s| s.branch.as_str());
            // Mid-rebase its branch reads as detached there: known by the folder kept.
            let elsewhere = pending.at.as_ref();
            // In this worktree, whether its rebase is still stopped (finished or aborted
            // meanwhile, Continue restack goes on from here).
            let stopped_here = elsewhere.is_none() && self.work.in_progress.is_some();
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::icons::inline(ui, "circle-alert", 16.0, t.warning);
                    ui.label(
                        egui::RichText::new(format!(
                            "Restack paused at {branch} ({} of {})",
                            pending.done + 1,
                            pending.steps.len()
                        ))
                        .color(t.warning)
                        .font(theme::medium(12.5)),
                    );
                });
                let what = match (elsewhere, stopped_here) {
                    (Some(wt), _) => format!(
                        "Resolve its conflicts in its worktree at {}, then Continue restack.",
                        wt.display()
                    ),
                    (None, true) => {
                        "Resolve its conflicts, mark them resolved, then Continue restack."
                            .to_owned()
                    }
                    (None, false) => "Continue restack goes on from here.".to_owned(),
                };
                ui.label(egui::RichText::new(what).color(t.text_dim).small());
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(idle, egui::Button::new("Continue restack"))
                        .clicked()
                    {
                        acts.push(Action::Stack(StackAct::ResumeRestack));
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new("Abort restack"))
                        .on_hover_text("Every branch back where it was before the restack")
                        .clicked()
                    {
                        acts.push(Action::Stack(StackAct::AbortRestack));
                    }
                    if let Some(wt) = elsewhere
                        && ui.button("Open its worktree").clicked()
                    {
                        acts.push(Action::OpenWorktree(wt.clone()));
                    }
                });
            });
            return;
        }
        let n = to_restack(&self.stacks.layers).len();
        if n == 0 {
            return;
        }
        frame.show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(if n == 1 {
                        "1 dependent branch needs updating".to_owned()
                    } else {
                        format!("{n} dependent branches need updating")
                    })
                    .color(t.text),
                );
                if ui
                    .add_enabled(idle, egui::Button::new("Restack…"))
                    .on_hover_text(
                        "Preview each one's new base, then replay their commits onto their \
                         parents",
                    )
                    .clicked()
                {
                    acts.push(Action::Stack(StackAct::OpenRestack(None)));
                }
            });
        });
    }

    /// Branches based on each other in a loop (the config edited by hand): named, with a way
    /// out.
    fn loop_row(&self, ui: &mut Ui, looped: &[String], idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            ui.add_space(22.0);
            ui.label(
                egui::RichText::new(format!("{} are based on each other.", looped.join(", ")))
                    .color(t.warning),
            );
            if let Some(first) = looped.first()
                && ui
                    .add_enabled(
                        idle,
                        egui::Button::new(format!("Pick {first}'s parent…")).small(),
                    )
                    .clicked()
            {
                acts.push(Action::Stack(StackAct::OpenBasedOn(first.clone())));
            }
        });
    }

    /// A layer: its name, how many commits it adds, its pull request, and a warning when its
    /// parent moved on.
    fn layer_row(&self, ui: &mut Ui, layer: &Layer, depth: usize) -> Response {
        let t = Tokens::get(ui.ctx());
        let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("stack-row", &layer.branch));
        let painter = ui.painter_at(rect);
        let is_head = self.refs.current.as_deref() == Some(layer.branch.as_str());
        if is_head {
            painter.rect_filled(rect, 0.0, t.accent.gamma_multiply(0.14));
        }
        if resp.hovered() {
            painter.rect_filled(rect, 0.0, t.hover);
        }
        let y = rect.center().y;
        let indent = INDENT * (depth.saturating_sub(1)) as f32;
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 30.0 + indent, y), vec2(16.0, 16.0));
        let (icon, tint) = if is_head {
            ("check", t.accent)
        } else {
            ("git-branch", t.text_faint)
        };
        icons::paint(ui, icon_rect, icon, 13.0, tint);
        let mut right = rect.right() - 8.0;
        if layer.needs_restack() || layer.parent_missing {
            let ir = Rect::from_center_size(pos2(right - 6.0, y), vec2(12.0, 12.0));
            icons::paint(ui, ir, "circle-alert", 12.0, t.warning);
            right = ir.left() - 6.0;
        }
        if let Some(pr) = self.prs.iter().find(|p| p.branch == layer.branch) {
            let r = painter.text(
                pos2(right, y),
                Align2::RIGHT_CENTER,
                format!("#{}", pr.number),
                theme::mono(11.0),
                t.success,
            );
            right = r.left() - 6.0;
        }
        let r = painter.text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            format!("+{}", layer.own),
            theme::mono(11.0),
            t.text_dim,
        );
        right = r.left() - 6.0;
        let font = if is_head {
            theme::semibold(12.5)
        } else {
            egui::FontId::proportional(12.5)
        };
        painter
            .with_clip_rect(Rect::from_min_max(rect.min, pos2(right, rect.bottom())))
            .text(
                pos2(rect.left() + 44.0 + indent, y),
                Align2::LEFT_CENTER,
                &layer.branch,
                font,
                t.text,
            );
        resp.on_hover_text(self.layer_tip(layer))
    }

    /// What a layer's tooltip says.
    fn layer_tip(&self, layer: &Layer) -> String {
        let commits = |n: u32| format!("{n} commit{}", if n == 1 { "" } else { "s" });
        let mut tip = format!(
            "{}\nBased on {} · {} of its own",
            layer.branch,
            layer.parent,
            commits(layer.own)
        );
        if let Some(pr) = self.prs.iter().find(|p| p.branch == layer.branch) {
            let noun = self.forge().map_or("pull request", |f| f.noun());
            tip.push_str(&format!(
                "\n{} #{}: {}",
                capitalised(noun),
                pr.number,
                pr.title
            ));
        }
        if layer.parent_missing {
            tip.push_str(&format!(
                "\n{} is gone: pick the branch it is based on now (This branch is based on…)",
                layer.parent
            ));
        } else if layer.needs_restack() {
            tip.push_str(&format!(
                "\n{} moved on ({} not in {}): needs restacking onto it",
                layer.parent,
                commits(layer.behind),
                layer.branch
            ));
        }
        tip.push_str("\nDouble-click to check out; right-click for the changes it adds");
        tip
    }

    /// Clicking a layer selects its tip, double-clicking checks it out, and its menu is the
    /// branch's.
    fn layer_clicks(&self, r: Response, layer: &Layer, idle: bool, acts: &mut Vec<Action>) {
        let Some(branch) = self.refs.local.iter().find(|b| b.name == layer.branch) else {
            return;
        };
        if r.double_clicked() && !branch.is_head && idle {
            acts.push(Action::Checkout(branch.name.clone()));
        } else if r.clicked() {
            acts.push(Action::SelectHash(branch.target.clone()));
        }
        crate::menu_item::context(&r)
            .style(crate::theme::menu_style)
            .show(|ui| self.branch_menu_items(ui, acts, branch, idle));
    }

    /// The branch menu's entries for stacks: pick the branch it is based on, or take it out of
    /// its stack.
    pub(in crate::repo) fn stack_menu_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        let recorded = self.stacks.recorded.get(&b.name);
        // Its own changes: since it forked from its parent, as its pull request will show them.
        if let Some(r) = recorded
            && self.refs.local.iter().any(|l| l.name == r.parent)
            && crate::menu_item::MenuItem::new(format!("Changes {} adds to {}", b.name, r.parent))
                .icon("command")
                .show(ui)
                .on_hover_text("Its own commits in the stack: since it forked from its parent")
                .clicked()
        {
            acts.push(compare_branches_action(
                BranchRef::local(&r.parent),
                BranchRef::local(&b.name),
                true,
            ));
        }
        let hover = match recorded {
            Some(r) => format!("Now based on {}", r.parent),
            None => "The branch it is a layer on in a stack".to_owned(),
        };
        if crate::menu_item::MenuItem::new("This branch is based on…")
            .icon("git-branch-plus")
            .enabled(idle)
            .show(ui)
            .on_hover_text(hover)
            .clicked()
        {
            acts.push(Action::Stack(StackAct::OpenBasedOn(b.name.clone())));
        }
        let on_it = dependents(&self.stacks.layers, &b.name).len();
        if on_it > 0
            && crate::menu_item::MenuItem::new(if on_it == 1 {
                "Restack the branch on it…".to_owned()
            } else {
                format!("Restack the {on_it} branches on it…")
            })
            .icon("git-branch-plus")
            .enabled(idle)
            .refused(
                self.stacks
                    .pending
                    .is_some()
                    .then_some("A restack is paused: continue or abort it in STACKS first"),
            )
            .show(ui)
            .on_hover_text("Replay their own commits onto its new tip")
            .on_disabled_hover_text("A restack is paused: continue or abort it in STACKS first")
            .clicked()
        {
            acts.push(Action::Stack(StackAct::OpenRestack(Some(b.name.clone()))));
        }
        if let Some(forge) = self.forge()
            && self.stacks.layer(&b.name).is_some()
            && crate::menu_item::MenuItem::new(format!("Publish its stack's {}s…", forge.noun()))
                .icon("external-link")
                .enabled(idle)
                .show(ui)
                .on_hover_text(
                    "Preview each one's base and patch, then open or update them bottom-up",
                )
                .clicked()
        {
            acts.push(Action::Stack(StackAct::OpenPublish(b.name.clone())));
        }
        if self.stacks.recorded.contains_key(&b.name)
            && crate::menu_item::MenuItem::new("Remove from its stack")
                .icon("minus")
                .enabled(idle)
                .show(ui)
                .clicked()
        {
            acts.push(Action::Stack(StackAct::RemoveParent(b.name.clone())));
        }
    }
}

/// A stack's title: `main → schema → api → ui`, and how many need restacking.
/// `noun`: what the forge calls its pull requests, when there is one to publish them on.
fn title_row(ui: &mut Ui, tree: &Tree, noun: Option<&str>) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("stack-title", &tree.root));
    let painter = ui.painter_at(rect);
    let y = rect.center().y;
    let mut right = rect.right() - 8.0;
    let (stale, gone) = tree.stale();
    let note = match (stale, gone) {
        (0, 0) => None,
        (n, 0) => Some(format!("{n} to restack")),
        (0, g) => Some(format!("{g} parent{} gone", if g == 1 { "" } else { "s" })),
        (n, g) => Some(format!("{n} to restack · {g} gone")),
    };
    if let Some(note) = note {
        let r = painter.text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            note,
            theme::medium(10.5),
            t.warning,
        );
        right = r.left() - 6.0;
    }
    let title = tree.title();
    painter
        .with_clip_rect(Rect::from_min_max(rect.min, pos2(right, rect.bottom())))
        .text(
            pos2(rect.left() + 22.0, y),
            Align2::LEFT_CENTER,
            &title,
            theme::medium(11.5),
            t.text_dim,
        );
    match noun {
        Some(noun) => resp.on_hover_text(format!("{title}\nRight-click to publish its {noun}s")),
        None => resp.on_hover_text(title),
    }
}

/// `noun` with a capital first letter: "Merge request".
fn capitalised(noun: &str) -> String {
    let mut chars = noun.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
