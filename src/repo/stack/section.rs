//! The sidebar's STACKS: each stack's branches bottom to top, under its title, with what
//! each layer adds, its pull request and whether it needs restacking; and the entries the
//! branch menu gets for stacks.

use egui::{Align2, Rect, Response, Ui, pos2, vec2};

use super::{StackAct, Tree, looped, trees};
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
        if stacks.is_empty() && looped.is_empty() && self.stacks.error.is_none() {
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
        // Filtered, a stack shows only its layers that match, under its title.
        let shown = |l: &Layer| filter.is_empty() || l.branch.to_lowercase().contains(filter);
        for tree in &stacks {
            if !tree.rows.iter().any(|(_, l)| shown(l)) {
                continue;
            }
            title_row(ui, tree);
            for (depth, layer) in tree.rows.iter().filter(|(_, l)| shown(l)) {
                let r = self.layer_row(ui, layer, *depth);
                self.layer_clicks(r, layer, idle, acts);
            }
        }
        if !looped.is_empty() {
            self.loop_row(ui, &looped, idle, acts);
        }
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
        egui::Popup::context_menu(&r)
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
            && ui
                .button(format!("Changes {} adds to {}", b.name, r.parent))
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
        if ui
            .add_enabled(idle, egui::Button::new("This branch is based on…"))
            .on_hover_text(hover)
            .clicked()
        {
            acts.push(Action::Stack(StackAct::OpenBasedOn(b.name.clone())));
        }
        if self.stacks.recorded.contains_key(&b.name)
            && ui
                .add_enabled(idle, egui::Button::new("Remove from its stack"))
                .clicked()
        {
            acts.push(Action::Stack(StackAct::RemoveParent(b.name.clone())));
        }
    }
}

/// A stack's title: `main → schema → api → ui`, and how many need restacking.
fn title_row(ui: &mut Ui, tree: &Tree) {
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
    resp.on_hover_text(title);
}

/// `noun` with a capital first letter: "Merge request".
fn capitalised(noun: &str) -> String {
    let mut chars = noun.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
