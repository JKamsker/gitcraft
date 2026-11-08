//! The banner under the toolbar after a refused push, and the words of it and of the force
//! push's confirmation.

use egui::{RichText, Stroke, Ui};

use super::check::{LISTED, Rejection, RemoteCommit, Shape};
use super::{Banner, PushAct};
use crate::icons;
use crate::repo::pull::PullMode;
use crate::repo::{Action, Dialog, RepoTab, short};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// How many commits the banner itself names (the confirmation names up to [`LISTED`]).
const IN_BANNER: usize = 3;

/// `1 commit`, `3 commits`.
fn commits(n: u32) -> String {
    if n == 1 {
        "1 commit".to_owned()
    } else {
        format!("{n} commits")
    }
}

/// What the banner says first, also the status line's words.
pub(super) fn headline(r: &Rejection) -> String {
    let remote = r.target.label();
    match r.shape {
        Shape::Clear => format!("{remote} has nothing you don't have now: push again"),
        _ => format!("{remote} has {} you don't have", commits(r.behind)),
    }
}

fn display_headline(r: &Rejection) -> String {
    crate::i18n::message(
        "repo-workflows-push-headline",
        &[
            (
                "clear",
                (if r.shape == Shape::Clear { "yes" } else { "no" }).into(),
            ),
            ("remote", (r.target.label()).into()),
            ("count", (r.behind).into()),
        ],
    )
}

/// `one` for a single commit, `many` for several.///
/// How the branch stands, after the headline.
pub(super) fn explanation(r: &Rejection) -> String {
    crate::i18n::message(
        "repo-workflows-push-explanation",
        &[
            ("moved", (if r.moved { "yes" } else { "no" }).into()),
            (
                "shape",
                (match r.shape {
                    Shape::Clear => "clear",
                    Shape::Behind => "behind",
                    Shape::Diverged => "diverged",
                    Shape::Rewritten if r.ahead == 0 => "back",
                    Shape::Rewritten => "rewritten",
                })
                .into(),
            ),
            ("remote", (r.target.label()).into()),
            ("branch", (r.branch.as_str()).into()),
            ("ahead", (r.ahead).into()),
            ("behind", (r.behind).into()),
        ],
    )
}

/// `abc1234  Fix the parser (Ann)`.
fn commit_line(c: &RemoteCommit) -> String {
    format!("{}  {} ({})", c.short, c.subject, c.author)
}

/// The confirmation's words: what the force push removes from the remote, and what guards it.
pub(super) fn force_body(r: &Rejection) -> String {
    debug_assert!(r.missing.len() <= LISTED);
    crate::i18n::message(
        "repo-workflows-push-force-body",
        &[
            ("remote", (r.target.label()).into()),
            ("branch", (r.branch.as_str()).into()),
            ("local", (short(&r.local)).into()),
            ("count", (r.behind).into()),
            (
                "commits",
                (r.missing.iter().map(commit_line).collect::<Vec<_>>().join(
                    "
    ",
                ))
                .into(),
            ),
            (
                "more",
                (r.behind.saturating_sub(r.missing.len() as u32)).into(),
            ),
            ("remote-commit", (short(&r.remote)).into()),
        ],
    )
}

impl RepoTab {
    /// The banner, when there is one: under the toolbar, above everything else.
    pub(crate) fn push_banner(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        // About a branch that is no longer the one checked out.
        let stale = match &self.push_flow.banner {
            Some(Banner::Detached) => self.refs.current.is_some(),
            Some(b) => b.branch() != self.refs.current.as_deref(),
            None => return,
        };
        if stale {
            self.push_flow.banner = None;
            return;
        }
        let Some(banner) = &self.push_flow.banner else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let head = self.refs.head.clone();
        egui::Panel::top("push_banner")
            .frame(
                egui::Frame::new()
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.separator))
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    icons::inline(ui, "circle-alert", 16.0, t.warning);
                    ui.vertical(|ui| match banner {
                        Banner::Rejected(r) => rejected(ui, r, idle, acts),
                        Banner::Refused { branch, message } => refused(ui, branch, message, acts),
                        Banner::NoUpstream { branch, remotes } => {
                            no_upstream(ui, branch, remotes, idle, acts)
                        }
                        Banner::Gone { branch, target, .. } => {
                            text(
                                ui,
                                &crate::i18n::message(
                                    "repo-workflows-push-push-banner-7361cc7b",
                                    &[("target-label-1", (target.label()).clone().into())],
                                ),
                                true,
                            );
                            text(
                                ui,
                                &crate::i18n::message(
                                    "repo-workflows-push-push-banner-39525827",
                                    &[
                                        ("branch", (branch).clone().into()),
                                        ("target-remote-1", (target.remote).clone().into()),
                                    ],
                                ),
                                false,
                            );
                            buttons(ui, |ui| {
                                primary(
                                    ui,
                                    crate::i18n::text("repo-workflows-push-push-banner-e2a4155b"),
                                    "",
                                    idle,
                                    || acts.push(Action::PushFlow(PushAct::PushAgain)),
                                );
                                dismiss(ui, acts);
                            });
                        }
                        Banner::Detached => {
                            text(
                                ui,
                                crate::i18n::text("repo-workflows-push-push-banner-e4671d42"),
                                true,
                            );
                            text(
                                ui,
                                crate::i18n::text("repo-workflows-push-push-banner-bb23a164"),
                                false,
                            );
                            buttons(ui, |ui| {
                                let can = idle && !head.is_empty();
                                primary(
                                    ui,
                                    crate::i18n::text("repo-workflows-push-push-banner-04e6949e"),
                                    "",
                                    can,
                                    || {
                                        acts.push(Action::Dialog(Dialog::create_branch(
                                            &head, "HEAD",
                                        )))
                                    },
                                );
                                dismiss(ui, acts);
                            });
                        }
                    });
                });
            });
    }
}

fn rejected(ui: &mut Ui, r: &Rejection, idle: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    text(ui, &display_headline(r), true);
    text(ui, &explanation(r), false);
    for c in r.missing.iter().take(IN_BANNER) {
        ui.label(
            RichText::new(commit_line(c))
                .font(theme::mono(11.5))
                .color(t.text_dim),
        );
    }
    let named = r.missing.len().min(IN_BANNER) as u32;
    if r.behind > named && named > 0 {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-push-rejected-683768ab",
                &[("r-behind-named-1", (r.behind - named).into())],
            ))
            .size(11.5)
            .color(t.text_faint),
        );
    }
    let mode = Settings::get(ui.ctx()).pull_mode;
    buttons(ui, |ui| {
        match r.shape {
            Shape::Clear => {
                primary(
                    ui,
                    crate::i18n::text("repo-workflows-push-rejected-e2a4155b"),
                    "",
                    idle,
                    || acts.push(Action::PushFlow(PushAct::PushAgain)),
                );
            }
            Shape::Behind => {
                let label = crate::i18n::message(
                    "repo-workflows-push-rejected-ab020f51",
                    &[("r-target-label-1", (r.target.label()).clone().into())],
                );
                let tip = crate::i18n::message(
                    "repo-workflows-push-rejected-0db0e893",
                    &[("mode-label-1", (mode.label()).into())],
                );
                primary(ui, &label, &tip, idle, || {
                    acts.push(Action::PushFlow(PushAct::Pull))
                });
            }
            Shape::Diverged | Shape::Rewritten => {
                // After a rewrite, replacing is what is most likely meant; else joining.
                let rewritten = r.shape == Shape::Rewritten;
                let force = crate::i18n::text("repo-workflows-push-rejected-04b2194e");
                let force_tip = crate::i18n::text("repo-workflows-push-rejected-2c2c46aa");
                if rewritten {
                    primary(ui, force, force_tip, idle, || {
                        acts.push(Action::PushFlow(PushAct::ConfirmForce))
                    });
                }
                pull_buttons(ui, mode, !rewritten, idle, acts);
                if !rewritten
                    && ui
                        .add_enabled(idle, egui::Button::new(force))
                        .on_hover_text(force_tip)
                        .clicked()
                {
                    acts.push(Action::PushFlow(PushAct::ConfirmForce));
                }
            }
        }
        if ui
            .add_enabled(
                idle,
                egui::Button::new(crate::i18n::text("repo-workflows-push-rejected-fb7099ad")),
            )
            .on_hover_text(crate::i18n::text("repo-workflows-push-rejected-9e1d29fb"))
            .clicked()
        {
            acts.push(Action::PushFlow(PushAct::CheckAgain));
        }
        dismiss(ui, acts);
    });
}

/// _Pull, then push_ the way the settings pull; fast-forward only can't join branches that
/// diverged, so then both ways that can are offered.
fn pull_buttons(ui: &mut Ui, mode: PullMode, first: bool, idle: bool, acts: &mut Vec<Action>) {
    let ways: Vec<(String, Option<PullMode>, String)> = if mode == PullMode::FastForwardOnly {
        vec![
            (
                crate::i18n::text("repo-workflows-push-pull-buttons-5e584d52").to_owned(),
                Some(PullMode::Merge),
                crate::i18n::text("repo-workflows-push-pull-buttons-aca90f08").to_owned(),
            ),
            (
                crate::i18n::text("repo-workflows-push-pull-buttons-dc0810af").to_owned(),
                Some(PullMode::Rebase),
                crate::i18n::text("repo-workflows-push-pull-buttons-aca90f08").to_owned(),
            ),
        ]
    } else {
        vec![(
            crate::i18n::text("repo-workflows-push-pull-buttons-ca69dd73").to_owned(),
            None,
            crate::i18n::message(
                "repo-workflows-push-pull-buttons-43992a40",
                &[("mode-label-1", (mode.label()).into())],
            ),
        )]
    };
    for (i, (label, way, tip)) in ways.into_iter().enumerate() {
        let act = Action::PushFlow(PushAct::PullThenPush(way));
        if first && i == 0 {
            if primary(ui, &label, &tip, idle, || {}) {
                acts.push(act);
            }
        } else if ui
            .add_enabled(idle, egui::Button::new(label))
            .on_hover_text(tip)
            .clicked()
        {
            acts.push(act);
        }
    }
}

fn refused(ui: &mut Ui, branch: &str, message: &str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    text(
        ui,
        &crate::i18n::message(
            "repo-workflows-push-refused-ff02e2eb",
            &[("branch", (branch).into())],
        ),
        true,
    );
    text(
        ui,
        crate::i18n::text("repo-workflows-push-refused-d5689de7"),
        false,
    );
    egui::ScrollArea::vertical()
        .max_height(140.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(message)
                        .font(theme::mono(11.5))
                        .color(t.text_dim),
                )
                .selectable(true)
                .wrap(),
            );
        });
    let mut copy = false;
    buttons(ui, |ui| {
        copy = ui
            .button(crate::i18n::text("repo-workflows-push-refused-457efe53"))
            .clicked();
        dismiss(ui, acts);
    });
    if copy {
        ui.ctx().copy_text(message.to_owned());
    }
}

fn no_upstream(ui: &mut Ui, branch: &str, remotes: &[String], idle: bool, acts: &mut Vec<Action>) {
    text(
        ui,
        &crate::i18n::message(
            "repo-workflows-push-no-upstream-9ab92941",
            &[("branch", (branch).into())],
        ),
        true,
    );
    text(
        ui,
        &crate::i18n::message(
            "repo-workflows-push-no-upstream-86cb1261",
            &[("branch", (branch).into())],
        ),
        false,
    );
    buttons(ui, |ui| {
        // `origin` first, as the one most often meant.
        let mut sorted: Vec<&String> = remotes.iter().collect();
        sorted.sort_by_key(|r| *r != "origin");
        for (i, remote) in sorted.into_iter().enumerate() {
            let label = crate::i18n::message(
                "repo-workflows-push-no-upstream-d32eee95",
                &[("remote", (remote).clone().into())],
            );
            let mut act = || acts.push(Action::PushFlow(PushAct::Publish(remote.clone())));
            if i == 0 {
                primary(ui, &label, "", idle, act);
            } else if ui.add_enabled(idle, egui::Button::new(label)).clicked() {
                act();
            }
        }
        dismiss(ui, acts);
    });
}

/// A line of the banner: its headline in semibold, else wrapped and dimmer.
fn text(ui: &mut Ui, s: &str, head: bool) {
    let t = Tokens::get(ui.ctx());
    let rich = if head {
        RichText::new(s).font(theme::semibold(13.0)).color(t.text)
    } else {
        RichText::new(s).color(t.text_dim)
    };
    ui.add(egui::Label::new(rich).wrap());
}

fn buttons(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(add);
}

/// The button of what is most likely meant, with `tip` (if any) on hover; `clicked` runs when
/// it is clicked, and the return says whether it was.
fn primary(ui: &mut Ui, label: &str, tip: &str, enabled: bool, clicked: impl FnOnce()) -> bool {
    let t = Tokens::get(ui.ctx());
    let button = egui::Button::new(
        RichText::new(label)
            .color(t.accent_text)
            .font(theme::medium(12.0)),
    )
    .fill(t.accent);
    let mut response = ui.add_enabled(enabled, button);
    if !tip.is_empty() {
        response = response.on_hover_text(tip);
    }
    let hit = response.clicked();
    if hit {
        clicked();
    }
    hit
}

fn dismiss(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui
        .button(crate::i18n::text("repo-workflows-push-dismiss-48845bff"))
        .clicked()
    {
        acts.push(Action::PushFlow(PushAct::Dismiss));
    }
}
