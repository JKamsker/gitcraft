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
        let (Some(hosting), Some(_)) = (self.hosting.clone(), self.default_remote()) else {
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
        self.then(
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
        let (Some(hosting), Some(remote)) = (self.hosting.clone(), self.default_remote()) else {
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
        spawn_or(&self.tx, ctx, failed, move || {
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
        let noun = self
            .forge()
            .unwrap_or(crate::forge::Forge::GitHub)
            .noun_label();
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
                    RichText::new(crate::i18n::message(
                        "repo-workflows-stack-publish-dialog-e512538a",
                        &[
                            ("noun", (noun).clone().into()),
                            ("preview-root-1", (preview.root).clone().into()),
                        ],
                    ))
                    .font(theme::semibold(15.0)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-stack-publish-dialog-55798dde",
                        &[("noun", (noun).clone().into())],
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
                                RichText::new(crate::i18n::message(
                                    "repo-workflows-stack-publish-dialog-64d68875",
                                    &[("noun", (noun).clone().into())],
                                ))
                                .color(t.text_faint),
                            );
                        });
                    }
                    Some(Err(e)) => {
                        can = false;
                        ui.colored_label(
                            t.danger,
                            crate::i18n::message(
                                "repo-workflows-stack-publish-dialog-1638800f",
                                &[("e", (e).clone().into())],
                            ),
                        );
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
                            ui.checkbox(
                                &mut draft,
                                crate::i18n::message(
                                    "repo-workflows-stack-publish-dialog-d530df80",
                                    &[("noun", (noun).clone().into())],
                                ),
                            );
                        }
                        if plans.iter().any(|p| p.reparent_from.is_some()) {
                            ui.label(
                                RichText::new(crate::i18n::text(
                                    "repo-workflows-stack-publish-dialog-44cb25f8",
                                ))
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
                    if widgets::primary_button(
                        ui,
                        crate::i18n::text("repo-workflows-stack-publish-dialog-859390eb"),
                        can,
                        90.0,
                    )
                    .clicked()
                    {
                        acts.push(Action::Stack(StackAct::StartPublish));
                    }
                    if ui
                        .add(
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-stack-publish-dialog-19766ed6",
                            ))
                            .min_size(egui::vec2(80.0, 30.0)),
                        )
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
        Step::Create { replaces: None } => (
            crate::i18n::message(
                "repo-workflows-stack-plan-row-512a4ebf",
                &[
                    ("noun", (noun).clone().into()),
                    ("plan-base-1", (plan.base).clone().into()),
                ],
            ),
            t.accent,
        ),
        Step::Create { replaces: Some(n) } => (
            crate::i18n::message(
                "repo-workflows-stack-plan-row-3c8950f2",
                &[
                    ("noun", (noun).clone().into()),
                    ("plan-base-1", (plan.base).clone().into()),
                    ("n", (n).to_string().into()),
                ],
            ),
            t.accent,
        ),
        Step::Retarget { number, from } => (
            crate::i18n::message(
                "repo-workflows-stack-plan-row-1004d25d",
                &[
                    ("number", (number).to_string().into()),
                    ("plan-base-1", (plan.base).clone().into()),
                    ("from", (from).clone().into()),
                ],
            ),
            t.warning,
        ),
        Step::Push(n) => (
            crate::i18n::message(
                "repo-workflows-stack-plan-row-c5790350",
                &[("n", (n).to_string().into())],
            ),
            t.text_dim,
        ),
        Step::Merged(n) => (
            crate::i18n::message(
                "repo-workflows-stack-plan-row-6a96cf4f",
                &[("n", (n).to_string().into())],
            ),
            t.text_faint,
        ),
        Step::Empty => (
            crate::i18n::message(
                "repo-workflows-stack-plan-row-25b842ca",
                &[("noun", (noun).clone().into())],
            ),
            t.text_faint,
        ),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&plan.branch).font(theme::medium(12.5)));
        ui.label(RichText::new(what).color(color));
        if !matches!(step, Step::Merged(_))
            && ui
                .small_button(crate::i18n::text("repo-workflows-stack-plan-row-bbd4b6a8"))
                .on_hover_text(crate::i18n::message(
                    "repo-workflows-stack-plan-row-ff1f70b3",
                    &[
                        ("plan-branch-1", (plan.branch).clone().into()),
                        ("plan-base-2", (plan.base).clone().into()),
                    ],
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
                    .hint_text(crate::i18n::text("repo-workflows-stack-plan-row-7e8cd205"))
                    .desired_width(ui.available_width() - 8.0),
            );
        });
        if title != plan.title {
            acts.push(Action::Stack(StackAct::PublishTitle(i, title)));
        }
    }
    if let Some(from) = &plan.reparent_from {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-stack-plan-row-96f34b27",
                &[
                    ("from", (from).clone().into()),
                    ("noun", (noun).clone().into()),
                    ("plan-branch-1", (plan.branch).clone().into()),
                    ("plan-base-2", (plan.base).clone().into()),
                ],
            ))
            .color(t.text_faint),
        );
    }
    if !matches!(step, Step::Merged(_)) {
        let n = plan.commits.len();
        let mut patch =
            crate::i18n::message("repo-workflows-stack-patch-count", &[("count", (n).into())]);
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
                RichText::new(crate::i18n::message(
                    "repo-workflows-stack-plan-row-140261ea",
                    &[("n-4-1", (n - 4).clone().into())],
                ))
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
