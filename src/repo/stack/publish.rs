//! Publishing a stack's pull requests from the tab: the preview of each one's base and
//! patch, publishing (or updating) them in the background, and what came of it.

use egui::RichText;

use super::{StackAct, trees};
use crate::forge::stack::{self, Input, LayerPlan, Report, Step, Target};
use crate::repo::actions::{BranchRef, compare_branches_action};
use crate::repo::messages::{Msg, spawn_or};
use crate::repo::{Action, OpKind, Refresh, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// A stack's pull requests being previewed.
pub(crate) struct PublishPreview {
    /// The branch at the bottom of the stack.
    pub root: String,
    /// The plan: `None` while it is worked out (the forge is asked what is open).
    pub plans: Option<Result<Vec<LayerPlan>, String>>,
    pub draft: bool,
    /// Which preview this is: a plan worked out for one closed since isn't taken.
    id: u64,
}

impl RepoTab {
    /// Previews publishing the pull requests of the stack `branch` is in (or at the bottom of).
    pub(super) fn open_publish(&mut self, branch: &str, ctx: &egui::Context) {
        let (Some(hosting), Some(_)) = (
            self.hosting.clone(),
            self.primary_forge_remote().map(str::to_owned),
        ) else {
            self.status = Some((
                "This repository isn't on a forge gitcraft knows: there is nowhere to publish to."
                    .to_owned(),
                true,
            ));
            return;
        };
        let Some(tree) = trees(&self.stacks.layers)
            .into_iter()
            .find(|t| t.root == branch || t.rows.iter().any(|(_, l)| l.branch == branch))
        else {
            return;
        };
        if let Some(layer) = tree.rows.iter().map(|(_, l)| l).find(|l| l.parent_missing) {
            self.status = Some((
                format!(
                    "{} is gone: pick what {} is based on first.",
                    layer.parent, layer.branch
                ),
                true,
            ));
            return;
        }
        let inputs: Vec<Input> = tree
            .rows
            .iter()
            .map(|(_, l)| Input {
                branch: l.branch.clone(),
                parent: l.parent.clone(),
            })
            .collect();
        static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.stacks.publish = Some(PublishPreview {
            root: tree.root.clone(),
            plans: None,
            draft: false,
            id,
        });
        let path = self.path.clone();
        self.forge_then(
            ctx,
            move || {
                // Each branch's pull request asked of the forge now, whatever its state.
                let provider = hosting.forge.provider();
                let find = |branch: &str| provider.find(&path, &hosting, branch);
                stack::plan(&path, &inputs, &find)
            },
            move |tab, found, _| {
                if let Some(p) = tab.stacks.publish.as_mut().filter(|p| p.id == id) {
                    p.plans = Some(found);
                }
            },
        );
    }

    /// Publishes as previewed.
    pub(super) fn start_publish(&mut self, ctx: &egui::Context) {
        let (Some(hosting), Some(remote)) = (
            self.hosting.clone(),
            self.primary_forge_remote().map(str::to_owned),
        ) else {
            return;
        };
        let Some(PublishPreview {
            plans: Some(Ok(plans)),
            draft,
            ..
        }) = self.stacks.publish.take()
        else {
            return;
        };
        if let Some(busy) = &self.busy_op {
            self.status = Some((format!("Publish: run it again once {busy} is done"), true));
            return;
        }
        let noun = hosting.forge.noun();
        let label = format!("Publish {noun}s");
        self.busy_op = Some(label.clone());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        self.status = None;
        let path = self.path.clone();
        let creds = self.credentials.clone();
        let failed = {
            let label = label.clone();
            move |e| Msg::OpDone {
                label,
                result: Err(e),
                refresh: Refresh::All,
                kind: OpKind::plain(),
                quiet: false,
                stopped: 0,
            }
        };
        spawn_or(&self.forge_operation_sender(), ctx, failed, move || {
            let report = stack::publish(
                &path,
                &plans,
                &Target {
                    provider: hosting.forge.provider(),
                    hosting: &hosting,
                    remote: &remote,
                    draft,
                    creds: creds.as_ref(),
                },
            );
            Msg::Then(Box::new(move |tab, ctx| {
                tab.publish_done(noun, plans.len(), report, ctx)
            }))
        });
    }

    /// Takes in how publishing went.
    fn publish_done(&mut self, noun: &str, total: usize, report: Report, ctx: &egui::Context) {
        if let Some((_, e)) = &report.failed
            && crate::git::is_auth_error(e)
        {
            // Asked to sign in, as any push that needs it is (the layers done stay done).
            let label = format!("Publish {noun}s");
            self.op_done(
                label,
                Err(e.clone()),
                Refresh::All,
                OpKind::plain(),
                false,
                ctx,
            );
            return;
        }
        self.busy_op = None;
        let long = self
            .op_started
            .take()
            .is_some_and(|t| t.elapsed() > crate::repo::operations::LONG_OP);
        self.status = Some(match &report.failed {
            None => (summary(noun, &report), false),
            Some((branch, e)) => (
                format!(
                    "{} of {total} done, then {branch} failed: {e}. Publish again to go on: \
                     what was done stays, no {noun} is opened twice.",
                    report.done.len()
                ),
                true,
            ),
        });
        if long && let Some((message, failed)) = self.status.clone() {
            self.finished
                .push((format!("{} · {message}", self.name), failed));
        }
        // Asked again now, whenever they were last.
        self.prs_loaded = None;
        self.load_prs(ctx);
        self.reload(ctx);
    }

    /// The preview, when it is open.
    pub(in crate::repo) fn publish_dialog(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(preview) = &self.stacks.publish else {
            return;
        };
        let t = Tokens::get(ctx);
        let noun = self.forge().map_or("pull request", |f| f.noun());
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let idle = self.busy_op.is_none();
        let response = egui::Modal::new(egui::Id::new("gitcraft-stack-publish"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(500.0);
                ui.label(
                    RichText::new(format!("The {noun}s of the stack on {}", preview.root))
                        .font(theme::semibold(15.0)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Each branch is pushed, bottom first, and its {noun} goes into the branch \
                         below it. Publish again later to update them all: none is opened twice, \
                         and a branch is never pushed over commits it doesn't have."
                    ))
                    .color(t.text_dim),
                );
                ui.add_space(10.0);
                let mut can = idle;
                let mut draft = preview.draft;
                match &preview.plans {
                    None => {
                        can = false;
                        ui.horizontal(|ui| {
                            ui.add(crate::spinner::Spinner::new());
                            ui.label(
                                RichText::new(format!("Asking which {noun}s are open…"))
                                    .color(t.text_faint),
                            );
                        });
                    }
                    Some(Err(e)) => {
                        can = false;
                        ui.colored_label(t.danger, format!("Can't publish: {e}"));
                    }
                    Some(Ok(plans)) => {
                        egui::ScrollArea::vertical()
                            .max_height(360.0)
                            .show(ui, |ui| {
                                for (i, plan) in plans.iter().enumerate() {
                                    plan_row(ui, &t, noun, i, plan, idle, acts);
                                }
                            });
                        if plans
                            .iter()
                            .any(|p| matches!(p.step(), Step::Create { .. }))
                        {
                            ui.add_space(6.0);
                            ui.checkbox(&mut draft, format!("Open new {noun}s as drafts"));
                        }
                        if plans.iter().any(|p| p.reparent_from.is_some()) {
                            ui.label(
                                RichText::new(
                                    "Restack the branches moved down afterwards, so their \
                                     changes are only their own.",
                                )
                                .color(t.text_faint),
                            );
                        }
                    }
                }
                if draft != preview.draft {
                    acts.push(Action::Stack(StackAct::PublishDraft(draft)));
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Publish", can, 90.0).clicked() {
                        acts.push(Action::Stack(StackAct::StartPublish));
                    }
                    if ui
                        .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
                        .clicked()
                    {
                        acts.push(Action::Stack(StackAct::ClosePublish));
                    }
                });
            });
        if response.should_close() {
            acts.push(Action::Stack(StackAct::ClosePublish));
        }
    }
}

/// One layer of the preview: the branch, where its pull request goes, what happens to it
/// (the title of a new one, which can be changed), and its patch: commits, files changed, and
/// a button for its changes in full.
fn plan_row(
    ui: &mut egui::Ui,
    t: &Tokens,
    noun: &str,
    i: usize,
    plan: &LayerPlan,
    idle: bool,
    acts: &mut Vec<Action>,
) {
    let step = plan.step();
    let (what, color) = match &step {
        Step::Create { replaces: None } => (format!("new {noun} into {}", plan.base), t.accent),
        Step::Create { replaces: Some(n) } => (
            format!(
                "new {noun} into {} (#{n} was closed without merging)",
                plan.base
            ),
            t.accent,
        ),
        Step::Retarget { number, from } => (
            format!("#{number}: into {} instead of {from}", plan.base),
            t.warning,
        ),
        Step::Push(n) => (format!("#{n}: push the new commits"), t.text_dim),
        Step::Merged(n) => (format!("#{n} is merged: left alone"), t.text_faint),
        Step::Empty => (
            format!("nothing of its own: pushed, no {noun}"),
            t.text_faint,
        ),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&plan.branch).font(theme::medium(12.5)));
        ui.label(RichText::new(what).color(color));
        if !matches!(step, Step::Merged(_))
            && ui
                .small_button("Changes")
                .on_hover_text(format!(
                    "What {} adds to {}, in full",
                    plan.branch, plan.base
                ))
                .clicked()
        {
            acts.push(Action::Stack(StackAct::ClosePublish));
            acts.push(compare_branches_action(
                BranchRef::local(&plan.base),
                BranchRef::local(&plan.branch),
                true,
            ));
        }
    });
    if matches!(step, Step::Create { .. }) {
        let mut title = plan.title.clone();
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.add_enabled(
                idle,
                egui::TextEdit::singleline(&mut title)
                    .hint_text("Title")
                    .desired_width(ui.available_width() - 8.0),
            );
        });
        if title != plan.title {
            acts.push(Action::Stack(StackAct::PublishTitle(i, title)));
        }
    }
    if let Some(from) = &plan.reparent_from {
        ui.label(
            RichText::new(format!(
                "   {from}'s {noun} was merged: {} goes into {} now",
                plan.branch, plan.base
            ))
            .color(t.text_faint),
        );
    }
    if !matches!(step, Step::Merged(_)) {
        let n = plan.commits.len();
        let mut patch = format!("   {n} commit{}", if n == 1 { "" } else { "s" });
        if !plan.stat.is_empty() {
            patch.push_str(&format!(" · {}", plan.stat));
        }
        ui.label(RichText::new(patch).color(t.text_faint));
        for subject in plan.commits.iter().take(4) {
            ui.label(
                RichText::new(format!("     {subject}"))
                    .color(t.text_faint)
                    .small(),
            );
        }
        if n > 4 {
            ui.label(
                RichText::new(format!("     and {} more", n - 4))
                    .color(t.text_faint)
                    .small(),
            );
        }
    }
    ui.add_space(4.0);
}

/// "Opened #10, #11; #12 now into schema" and the like, for a publish that went through.
fn summary(noun: &str, report: &Report) -> String {
    if report.done.is_empty() {
        return format!("Publish {noun}s: nothing to do");
    }
    let parts: Vec<String> = report
        .done
        .iter()
        .map(|(branch, what)| format!("{branch} ({what})"))
        .collect();
    format!("Published {noun}s: {}", parts.join(", "))
}
