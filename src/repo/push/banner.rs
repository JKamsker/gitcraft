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

/// `one` for a single commit, `many` for several.
fn pick<'a>(n: u32, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// How the branch stands, after the headline.
pub(super) fn explanation(r: &Rejection) -> String {
    let (remote, branch) = (r.target.label(), &r.branch);
    let moved = if r.moved {
        format!("{remote} changed again since it was last checked, so nothing was overwritten. ")
    } else {
        String::new()
    };
    let n = r.behind;
    let rest = match r.shape {
        Shape::Clear => "It may have been pushed from elsewhere meanwhile.".to_owned(),
        Shape::Behind => format!(
            "Your {branch} is only behind it, with nothing of its own to push: pull to catch up."
        ),
        Shape::Diverged => format!(
            "Your {branch} has {} that {remote} doesn't have: both moved on. Pull, then push \
             joins them; a force push replaces what is on {remote}.",
            commits(r.ahead)
        ),
        Shape::Rewritten if r.ahead == 0 => format!(
            "Your {branch} was moved back here (a commit undone, a reset) after {} pushed. \
             Pulling would bring {} back; a force push removes {} from {remote}.",
            pick(n, "it was", "they were"),
            pick(n, "it", "them"),
            pick(n, "it", "them"),
        ),
        Shape::Rewritten => format!(
            "Your {branch} was rewritten here (amended or rebased) after {} pushed, and has {} \
             in {} place. Pulling would bring the old {} back beside the new; a force push \
             replaces {}.",
            pick(n, "that commit was", "those commits were"),
            commits(r.ahead),
            pick(n, "its", "their"),
            pick(n, "one", "ones"),
            pick(n, "it", "them"),
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
    let n = r.behind;
    let mut body = format!(
        "Force pushing will replace {remote} with your {branch} ({}). {} on {remote} but not \
         on your {branch}; {} removed from {remote} (clones that already have {} keep {}):\n",
        short(&r.local),
        if n == 1 {
            "This commit is".to_owned()
        } else {
            format!("These {n} commits are")
        },
        pick(n, "it will be", "they will be"),
        pick(n, "it", "them"),
        pick(n, "it", "them"),
    );
    for c in &r.missing {
        body.push_str(&format!("\n    {}", commit_line(c)));
    }
    let listed = r.missing.len() as u32;
    if n > listed {
        body.push_str(&format!("\n    and {} more", n - listed));
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
                        Banner::Gone { branch, target, .. } => {
                            text(ui, &format!("{} is gone", target.label()), true);
                            text(
                                ui,
                                &format!(
                                    "The branch {branch} pushes to was deleted on {}. Push \
                                     again to create it there.",
                                    target.remote
                                ),
                                false,
                            );
                            buttons(ui, |ui| {
                                primary(ui, "Push again", "", idle, || {
                                    acts.push(Action::PushFlow(PushAct::PushAgain))
                                });
                                dismiss(ui, acts);
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
                                let can = idle && !head.is_empty();
                                primary(ui, "Create branch…", "", can, || {
                                    acts.push(Action::Dialog(Dialog::create_branch(&head, "HEAD")))
                                });
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
            RichText::new(format!("and {} more", r.behind - named))
                .size(11.5)
                .color(t.text_faint),
        );
    }
    let mode = Settings::get(ui.ctx()).pull_mode;
    buttons(ui, |ui| {
        match r.shape {
            Shape::Clear => {
                primary(ui, "Push again", "", idle, || {
                    acts.push(Action::PushFlow(PushAct::PushAgain))
                });
            }
            Shape::Behind => {
                let label = format!("Pull {}", r.target.label());
                let tip = format!("{}; nothing is pushed", mode.label());
                primary(ui, &label, &tip, idle, || {
                    acts.push(Action::PushFlow(PushAct::Pull))
                });
            }
            Shape::Diverged | Shape::Rewritten => {
                // After a rewrite, replacing is what is most likely meant; else joining.
                let rewritten = r.shape == Shape::Rewritten;
                let force = "Force push (with lease)…";
                let force_tip = "Asks first, naming the commits it removes";
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
            .add_enabled(idle, egui::Button::new("Check again"))
            .on_hover_text("Fetch the remote branch and count again")
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
        &format!("The remote refused the push of {branch}"),
        true,
    );
    text(
        ui,
        "The remote said no itself (a hook or a branch protection, for example); force pushing \
         won't get past that. What git said:",
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
        dismiss(ui, acts);
    });
    if copy {
        ui.ctx().copy_text(message.to_owned());
    }
}

fn no_upstream(ui: &mut Ui, branch: &str, remotes: &[String], idle: bool, acts: &mut Vec<Action>) {
    text(ui, &format!("{branch} isn't on any remote yet"), true);
    text(
        ui,
        &format!("Choose a remote to publish it on; {branch} will track it there."),
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
    if ui.button("Dismiss").clicked() {
        acts.push(Action::PushFlow(PushAct::Dismiss));
    }
}
