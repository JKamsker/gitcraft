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

/// How the branch stands, after the headline.
pub(super) fn explanation(r: &Rejection) -> String {
    let (remote, branch) = (r.target.label(), &r.branch);
    let moved = if r.moved {
        format!("{remote} changed again since it was last checked, so nothing was overwritten. ")
    } else {
        String::new()
    };
    let rest = match r.shape {
        Shape::Clear => "It may have been pushed from elsewhere meanwhile.".to_owned(),
        Shape::Behind => {
            format!("Your {branch} is just behind, with nothing new to push: pull to catch up.")
        }
        Shape::Diverged => format!(
            "Your {branch} has {} it doesn't: both moved on. Pull to join them, or force push \
             to replace what is on {remote}.",
            commits(r.ahead)
        ),
        Shape::Rewritten => format!(
            "Your {branch} was rewritten here (amended, rebased or undone) after {} pushed, \
             and has {} in {} place. Pulling would bring the old {} back beside the new; force \
             pushing replaces {}.",
            if r.behind == 1 {
                "that was"
            } else {
                "those were"
            },
            commits(r.ahead),
            if r.behind == 1 { "its" } else { "their" },
            if r.behind == 1 { "commit" } else { "commits" },
            if r.behind == 1 { "it" } else { "them" },
        ),
    };
    format!("{moved}{rest}")
}

/// `abc1234  Fix the parser (Ann)`.
fn commit_line(c: &RemoteCommit) -> String {
    format!("{}  {} ({})", c.short, c.subject, c.author)
}

/// The confirmation's words: what the force push removes from the remote, and what guards it.
pub(super) fn force_body(r: &Rejection) -> String {
    let (remote, branch) = (r.target.label(), &r.branch);
    let mut body = format!(
        "{remote} is replaced with your {branch} ({}). {} on {remote} {} not on your {branch}, \
         and the push removes {} from {remote}:\n",
        short(&r.local),
        if r.behind == 1 {
            "This commit".to_owned()
        } else {
            format!("These {} commits", r.behind)
        },
        if r.behind == 1 { "is" } else { "are" },
        if r.behind == 1 { "it" } else { "them" },
    );
    for c in &r.missing {
        body.push_str(&format!("\n    {}", commit_line(c)));
    }
    let listed = r.missing.len() as u32;
    if r.behind > listed {
        body.push_str(&format!("\n    and {} more", commits(r.behind - listed)));
    }
    debug_assert!(r.missing.len() <= LISTED);
    body.push_str(&format!(
        "\n\nIf {remote} has moved from {} since it was checked, or your {branch} from {}, \
         nothing is pushed (--force-with-lease).",
        short(&r.remote),
        short(&r.local)
    ));
    body
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
                        Banner::Gone { branch, target } => {
                            text(ui, &format!("{} is gone", target.label()), true);
                            text(
                                ui,
                                &format!(
                                    "The branch your {branch} went to was deleted there. Push \
                                     again to publish {branch} on it anew."
                                ),
                                false,
                            );
                            buttons(ui, |ui| {
                                primary(ui, "Push again", idle, || {
                                    acts.push(Action::PushFlow(PushAct::PushAgain))
                                });
                                cancel(ui, acts);
                            });
                        }
                        Banner::Detached => {
                            text(ui, "HEAD is detached: there is no branch to push", true);
                            text(
                                ui,
                                "Create a branch here (or check one out), then push it.",
                                false,
                            );
                            buttons(ui, |ui| {
                                primary(ui, "Create branch…", idle && !head.is_empty(), || {
                                    acts.push(Action::Dialog(Dialog::create_branch(&head, "HEAD")))
                                });
                                cancel(ui, acts);
                            });
                        }
                    });
                });
            });
    }
}

fn rejected(ui: &mut Ui, r: &Rejection, idle: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    text(ui, &headline(r), true);
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
            RichText::new(format!("and {} more", commits(r.behind - named)))
                .size(11.5)
                .color(t.text_faint),
        );
    }
    let mode = Settings::get(ui.ctx()).pull_mode;
    buttons(ui, |ui| {
        match r.shape {
            Shape::Clear => {
                primary(ui, "Push again", idle, || {
                    acts.push(Action::PushFlow(PushAct::PushAgain))
                });
            }
            Shape::Behind => {
                primary(ui, &format!("Pull {}", r.target.label()), idle, || {
                    acts.push(Action::PushFlow(PushAct::PullThenPush(None)))
                });
            }
            Shape::Diverged | Shape::Rewritten => {
                // After a rewrite, replacing is what is most likely meant; else joining.
                let rewritten = r.shape == Shape::Rewritten;
                let force = "Force push (with lease)…";
                if rewritten {
                    primary(ui, force, idle, || {
                        acts.push(Action::PushFlow(PushAct::ConfirmForce))
                    });
                }
                pull_buttons(ui, mode, !rewritten, idle, acts);
                if !rewritten
                    && ui
                        .add_enabled(idle, egui::Button::new(force))
                        .on_hover_text("Asks first, naming the commits it removes")
                        .clicked()
                {
                    acts.push(Action::PushFlow(PushAct::ConfirmForce));
                }
            }
        }
        if ui
            .add_enabled(idle, egui::Button::new("Check again"))
            .on_hover_text("Fetch the remote branch and count again")
            .clicked()
        {
            acts.push(Action::PushFlow(PushAct::CheckAgain));
        }
        cancel(ui, acts);
    });
}

/// _Pull, then push_ the way the settings pull; fast-forward only can't join branches that
/// diverged, so then both ways that can are offered.
fn pull_buttons(ui: &mut Ui, mode: PullMode, first: bool, idle: bool, acts: &mut Vec<Action>) {
    let ways: Vec<(String, Option<PullMode>, String)> = if mode == PullMode::FastForwardOnly {
        vec![
            (
                "Merge, then push".to_owned(),
                Some(PullMode::Merge),
                "Pull is set to fast-forward only, which can't join branches that diverged"
                    .to_owned(),
            ),
            (
                "Rebase, then push".to_owned(),
                Some(PullMode::Rebase),
                "Pull is set to fast-forward only, which can't join branches that diverged"
                    .to_owned(),
            ),
        ]
    } else {
        vec![(
            "Pull, then push".to_owned(),
            None,
            format!("{}, then push", mode.label()),
        )]
    };
    for (i, (label, way, tip)) in ways.into_iter().enumerate() {
        let act = Action::PushFlow(PushAct::PullThenPush(way));
        if first && i == 0 {
            if primary(ui, &label, idle, || {}) {
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
        &format!("The remote refused the push of {branch}"),
        true,
    );
    text(
        ui,
        "A hook or a branch protection there said no; force pushing would not get past it. \
         What git said:",
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
        copy = ui.button("Copy message").clicked();
        cancel(ui, acts);
    });
    if copy {
        ui.ctx().copy_text(message.to_owned());
    }
}

fn no_upstream(ui: &mut Ui, branch: &str, remotes: &[String], idle: bool, acts: &mut Vec<Action>) {
    text(ui, &format!("{branch} isn't on any remote yet"), true);
    text(
        ui,
        &format!("Publish it on one, which {branch} then tracks:"),
        false,
    );
    buttons(ui, |ui| {
        // `origin` first, as the one most often meant.
        let mut sorted: Vec<&String> = remotes.iter().collect();
        sorted.sort_by_key(|r| *r != "origin");
        for (i, remote) in sorted.into_iter().enumerate() {
            let label = format!("Publish to {remote}");
            let mut act = || acts.push(Action::PushFlow(PushAct::Publish(remote.clone())));
            if i == 0 {
                primary(ui, &label, idle, act);
            } else if ui.add_enabled(idle, egui::Button::new(label)).clicked() {
                act();
            }
        }
        cancel(ui, acts);
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

/// The button of what is most likely meant; `clicked` runs when it is clicked, and the
/// return says whether it was.
fn primary(ui: &mut Ui, label: &str, enabled: bool, clicked: impl FnOnce()) -> bool {
    let t = Tokens::get(ui.ctx());
    let button = egui::Button::new(
        RichText::new(label)
            .color(t.accent_text)
            .font(theme::medium(12.0)),
    )
    .fill(t.accent);
    let hit = ui.add_enabled(enabled, button).clicked();
    if hit {
        clicked();
    }
    hit
}

fn cancel(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui.button("Cancel").clicked() {
        acts.push(Action::PushFlow(PushAct::Dismiss));
    }
}
