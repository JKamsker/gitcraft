//! _Copy link_ and _Open on the forge_ for a commit, a branch, a file or some of its lines: the
//! menu entries (one per remote to choose from, when it isn't clear which), and the link made in
//! the background, copied or opened, with a warning when what it points at isn't pushed.

use egui::Ui;

use super::{Action, RepoTab};
use crate::forge::links::{self, Link, Target};
use crate::git::Remote;

/// A link asked for from a menu: to `target`, on the forge `remote` is on.
pub(crate) struct LinkRequest {
    pub target: Target,
    pub remote: String,
    /// Open it in the browser rather than copy it.
    pub open: bool,
}

/// The remotes a menu's links can go to, each with what it's called in the menu.
#[derive(Clone)]
pub(crate) struct LinkMenu {
    choices: Vec<Choice>,
}

#[derive(Clone)]
struct Choice {
    remote: String,
    /// "GitHub", or "the forge" when its host doesn't tell.
    forge: String,
    /// `github.com/owner/repo`, to tell remotes apart.
    place: String,
}

impl LinkMenu {
    /// "Copy link to {what}" and "Open {what} on GitHub" for `target`; each a submenu of the
    /// remotes when there are several to choose from.
    pub(crate) fn items(&self, ui: &mut Ui, what: &str, target: &Target, acts: &mut Vec<Action>) {
        let ask = |remote: &str, open: bool| {
            Action::ForgeLink(LinkRequest {
                target: target.clone(),
                remote: remote.to_owned(),
                open,
            })
        };
        if let [one] = self.choices.as_slice() {
            let tip = match target {
                Target::Branch { .. } => crate::i18n::message(
                    "repo-workflows-forge_link-items-ea6a597e",
                    &[
                        ("one-remote-1", (one.remote).clone().into()),
                        ("one-place-2", (one.place).clone().into()),
                    ],
                ),
                _ => crate::i18n::message(
                    "repo-workflows-forge_link-items-6bb03074",
                    &[
                        ("one-remote-1", (one.remote).clone().into()),
                        ("one-place-2", (one.place).clone().into()),
                    ],
                ),
            };
            if crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-workflows-forge_link-items-a5232678",
                &[("what", (what).clone().into())],
            ))
            .icon("copy")
            .show(ui)
            .on_hover_text(&tip)
            .clicked()
            {
                acts.push(ask(&one.remote, false));
            }
            if crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-workflows-forge_link-items-78bef62f",
                &[
                    ("what", (what).clone().into()),
                    ("one-forge-1", (one.forge).clone().into()),
                ],
            ))
            .icon("external-link")
            .show(ui)
            .on_hover_text(&tip)
            .clicked()
            {
                acts.push(ask(&one.remote, true));
            }
            return;
        }
        for open in [false, true] {
            let label = if open {
                crate::i18n::message(
                    "repo-workflows-forge_link-items-44c5ece2",
                    &[("what", (what).clone().into())],
                )
            } else {
                crate::i18n::message(
                    "repo-workflows-forge_link-items-a5232678",
                    &[("what", (what).clone().into())],
                )
            };
            crate::menu_item::MenuItem::new(label)
                .icon("command")
                .submenu(ui, |ui| {
                    for c in &self.choices {
                        let text = format!("{} ({})", c.remote, c.place);
                        let tip = crate::i18n::message(
                            "repo-workflows-forge_link-items-55932e34",
                            &[("c-forge-1", (c.forge).clone().into())],
                        );
                        let b = crate::menu_item::MenuItem::new(text)
                            .icon("command")
                            .show(ui)
                            .on_hover_text(tip);
                        if b.clicked() {
                            acts.push(ask(&c.remote, open));
                            ui.close();
                        }
                    }
                });
        }
    }
}

impl RepoTab {
    /// The menu of links for what is on `branch` (the checked-out one when `None`): to the
    /// remote it tracks, else `origin`, else a choice. `None` when no remote is on a forge.
    pub(in crate::repo) fn link_menu(&self, branch: Option<&str>) -> Option<LinkMenu> {
        // A stash is never pushed: what it holds has no page on any forge.
        if self.stash_view.is_some() {
            return None;
        }
        let branch = branch.or(self.refs.current.as_deref());
        let tracked = branch.and_then(|name| self.tracked_remote(name));
        self.link_menu_of(links::candidates(&self.remotes, tracked.as_deref()))
    }

    /// The menu of links to the branch `name` (local, or `remote/branch`), and what the remote
    /// calls it: a local branch goes to the remote branch it tracks, a remote one to its remote.
    pub(in crate::repo) fn branch_link_menu(&self, name: &str) -> Option<(LinkMenu, Target)> {
        let (remotes, on_remote) = match self.refs.local.iter().find(|b| b.name == name) {
            Some(b) => {
                let tracked = links::upstream_remote(&b.upstream, &self.remotes);
                let remotes =
                    links::candidates(&self.remotes, tracked.as_ref().map(|(r, _)| r.as_str()));
                // The upstream's name is only the branch's name on the remote it is on.
                let on_remote = match (&tracked, remotes.as_slice()) {
                    (Some((remote, branch)), [one]) if one == remote => branch.clone(),
                    _ => name.to_owned(),
                };
                (remotes, on_remote)
            }
            None => {
                let (remote, branch) = links::upstream_remote(name, &self.remotes)?;
                let on_forge = self
                    .remotes
                    .iter()
                    .any(|r| r.name == remote && links::linkable(r));
                if !on_forge {
                    return None;
                }
                (vec![remote], branch)
            }
        };
        let menu = self.link_menu_of(remotes)?;
        Some((menu, Target::Branch { name: on_remote }))
    }

    /// The remote whose branch the local branch `name` tracks, or whose branch `name` is.
    fn tracked_remote(&self, name: &str) -> Option<String> {
        let tracked = match self.refs.local.iter().find(|b| b.name == name) {
            Some(b) => b.upstream.as_str(),
            None => name,
        };
        links::upstream_remote(tracked, &self.remotes).map(|(remote, _)| remote)
    }

    fn link_menu_of(&self, names: Vec<String>) -> Option<LinkMenu> {
        let choices: Vec<Choice> = names
            .into_iter()
            .filter_map(|name| {
                let remote = self.remotes.iter().find(|r| r.name == name)?;
                Some(choice(remote, self.hosting.as_ref()))
            })
            .collect();
        (!choices.is_empty()).then_some(LinkMenu { choices })
    }

    /// Makes the link asked for in the background (it runs git, and over SSH or in WSL a
    /// round trip), then copies or opens it.
    pub(in crate::repo) fn forge_link(&mut self, request: LinkRequest, ctx: &egui::Context) {
        let path = self.path.clone();
        let remotes = self.remotes.clone();
        let LinkRequest {
            target,
            remote,
            open,
        } = request;
        self.then(
            ctx,
            move || links::resolve(&path, &remotes, &remote, &target),
            move |tab, result, ctx| tab.status = Some(deliver(result, open, ctx)),
        );
    }
}

fn choice(remote: &Remote, hosting: Option<&crate::forge::Hosting>) -> Choice {
    let place = crate::forge::location(&remote.url).map_or_else(
        || remote.url.clone(),
        |(host, path)| format!("{host}/{path}"),
    );
    Choice {
        remote: remote.name.clone(),
        forge: links::forge_name(remote, hosting).to_owned(),
        place,
    }
}

/// Copies or opens the link; what the status line says of it (and whether as a warning).
fn deliver(result: Result<Link, String>, open: bool, ctx: &egui::Context) -> (String, bool) {
    let link = match result {
        Ok(link) => link,
        Err(e) => {
            let what = if open {
                crate::i18n::text("repo-workflows-forge_link-deliver-aab63f85")
            } else {
                crate::i18n::text("repo-workflows-forge_link-deliver-dbf362d4")
            };
            return (format!("{what}: {e}"), true);
        }
    };
    let done = if open {
        if !crate::pull_request::is_web_url(&link.url) {
            return (
                crate::i18n::message(
                    "repo-workflows-forge_link-deliver-f9eecf25",
                    &[("link-url-1", (link.url).clone().into())],
                ),
                true,
            );
        }
        if let Err(e) = open::that_detached(&link.url) {
            return (
                crate::i18n::message(
                    "repo-workflows-forge_link-deliver-5add4905",
                    &[
                        ("link-url-1", (link.url).clone().into()),
                        ("e", (e).to_string().into()),
                    ],
                ),
                true,
            );
        }
        crate::i18n::text("repo-workflows-forge_link-deliver-b19fb8d1")
    } else {
        ctx.copy_text(link.url.clone());
        crate::i18n::text("repo-workflows-forge_link-deliver-8d525e5f")
    };
    // The warning before the address, which is long and would push it out of sight.
    match link.warning {
        Some(warning) => (
            crate::i18n::message(
                "repo-workflows-forge_link-deliver-c5e2849f",
                &[
                    ("done", (done).clone().into()),
                    ("warning", (warning).clone().into()),
                    ("link-url-1", (link.url).clone().into()),
                ],
            ),
            true,
        ),
        None => (format!("{done} {}", link.url), false),
    }
}

#[cfg(test)]
mod tests;
