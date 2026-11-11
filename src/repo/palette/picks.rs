//! The verbs whose branch (or reset mode) is picked in a second step, and those choices.

use super::{PaletteItem, PaletteRun, Pick};
use crate::git::Branch;
use crate::repo::actions::{
    BranchRef, compare_branches_action, interactive_rebase_action, merge_action, rebase_action,
};
use crate::repo::commit_actions::{RESET_MODES, Rewrite, reset_action};
use crate::repo::{Action, RepoTab, short};

impl RepoTab {
    /// The verbs, each offered when it has something to pick.
    pub(super) fn pick_items(&self) -> Vec<PaletteItem> {
        let verb = |label: String, hint: &str, pick: Pick| {
            let key = format!("pick:{}", pick.key());
            PaletteItem::new(label, hint, PaletteRun::Pick(pick)).key(key)
        };
        let mut items = Vec::new();
        let others = self.other_branches().next().is_some();
        if others {
            items.push(verb(
                crate::i18n::text("repo-core-2971").to_owned(),
                "branch",
                Pick::Checkout,
            ));
        }
        if let Some(current) = &self.refs.current
            && others
        {
            items.extend([
                verb(
                    crate::i18n::message(
                        "repo-core-2972",
                        &[("current", current.to_string().into())],
                    ),
                    "merge",
                    Pick::Merge,
                ),
                verb(
                    crate::i18n::message(
                        "repo-core-2973",
                        &[("current", current.to_string().into())],
                    ),
                    "rebase",
                    Pick::Rebase,
                ),
                verb(
                    crate::i18n::message(
                        "repo-core-2974",
                        &[("current", current.to_string().into())],
                    ),
                    "rebase",
                    Pick::InteractiveRebase,
                ),
                verb(
                    crate::i18n::message(
                        "repo-core-2975",
                        &[("current", current.to_string().into())],
                    ),
                    "compare",
                    Pick::ChangesAdded,
                ),
                verb(
                    crate::i18n::message(
                        "repo-core-2976",
                        &[("current", current.to_string().into())],
                    ),
                    "compare",
                    Pick::Compare,
                ),
                verb(
                    crate::i18n::message(
                        "dev-integration-ui-rehearse",
                        &[("branch", current.to_string().into())],
                    ),
                    "rehearse",
                    Pick::Rehearse,
                ),
            ]);
        }
        if let Some(current) = &self.refs.current
            && !self.interactions.going()
            && self.refs.local.iter().any(|b| b.name != *current)
        {
            items.push(verb(
                crate::i18n::message("repo-core-2977", &[("current", current.to_string().into())]),
                "interaction",
                Pick::Interaction,
            ));
        }
        if self.pushable().next().is_some() {
            items.push(verb(
                crate::i18n::text("repo-core-2979").to_owned(),
                "remote",
                Pick::Push,
            ));
        }
        items
    }

    /// What to pick from once `pick` is chosen.
    pub(crate) fn palette_choices(&self, pick: &Pick) -> Vec<PaletteItem> {
        let key = |name: &str| format!("{}:{name}", pick.key());
        match pick {
            Pick::Checkout => self
                .other_branches()
                .map(|(b, remote)| {
                    let action = match remote {
                        Some(_) => Action::CheckoutRemote(b.name.clone()),
                        None => Action::Checkout(b.name.clone()),
                    };
                    PaletteItem::named_action(
                        &b.name,
                        remote.unwrap_or(crate::i18n::text("repo-core-2601")),
                        action,
                    )
                    .key(key(&b.name))
                })
                .collect(),
            Pick::Push => self
                .pushable()
                .map(|(b, to, push)| {
                    PaletteItem::named_action(
                        &b.name,
                        &crate::i18n::message("repo-core-2981", &[("to", to.to_string().into())]),
                        push,
                    )
                    .key(key(&b.name))
                })
                .collect(),
            Pick::Reset(hash) => self.reset_choices(hash),
            Pick::Interaction => self
                .interaction_palette_items()
                .into_iter()
                .map(|(label, hint, action)| {
                    let item_key = key(&label);
                    PaletteItem::action(label, &hint, action).key(item_key)
                })
                .collect(),
            _ => {
                let Some(current) = self.refs.current.as_deref() else {
                    return Vec::new();
                };
                self.other_branches()
                    .filter_map(|(b, remote)| {
                        let at = BranchRef {
                            name: &b.name,
                            remote: remote.is_some(),
                        };
                        let action = branch_action(pick, at, current)?;
                        Some(
                            PaletteItem::named_action(
                                &b.name,
                                remote.unwrap_or(crate::i18n::text("repo-core-2601")),
                                action,
                            )
                            .key(key(&b.name)),
                        )
                    })
                    .collect()
            }
        }
    }

    /// The branches other than the checked-out one: local ones first, then those of each
    /// remote, with its name.
    fn other_branches(&self) -> impl Iterator<Item = (&Branch, Option<&str>)> {
        let local = self
            .refs
            .local
            .iter()
            .filter(|b| !b.is_head)
            .map(|b| (b, None));
        let remote = self
            .refs
            .remotes
            .iter()
            .flat_map(|(r, list)| list.iter().map(move |b| (b, Some(r.as_str()))));
        local.chain(remote)
    }

    /// The local branches, other than the checked-out one, that can be pushed: each with where
    /// to and how.
    fn pushable(&self) -> impl Iterator<Item = (&Branch, String, Action)> {
        self.refs
            .local
            .iter()
            .filter(|b| !b.is_head)
            .filter_map(|b| self.push_branch(b).map(|(to, push)| (b, to, push)))
    }

    /// Soft, mixed and hard reset of the checked-out branch to `hash`, each asking first.
    fn reset_choices(&self, hash: &str) -> Vec<PaletteItem> {
        let Some(commit) = self.index.get(hash).and_then(|&i| self.commits.get(i)) else {
            return Vec::new();
        };
        let target = self.refs.current.as_deref().unwrap_or("HEAD");
        let rewrite = Rewrite {
            hash,
            short: short(hash),
            summary: &commit.summary,
            target,
            head: &self.refs.head,
        };
        RESET_MODES
            .iter()
            .map(|mode| {
                PaletteItem::action(
                    format!(
                        "{} — {}",
                        super::super::messages::display_operation(mode.label),
                        crate::i18n::text(mode.short)
                    ),
                    "reset",
                    reset_action(&rewrite, mode),
                )
                // Not remembered: the way to reset is chosen each time, never preselected (a
                // hard reset last time is no reason to offer it first).
                .key(String::new())
            })
            .collect()
    }
}

/// What `pick` does with the branch `at`, for the checked-out branch `current`; `None` for the
/// verbs that don't bring a branch into the checked-out one.
fn branch_action(pick: &Pick, at: BranchRef<'_>, current: &str) -> Option<Action> {
    let here = BranchRef::local(current);
    Some(match pick {
        Pick::Rehearse => Action::Rehearse(crate::repo::rehearse::RehearseAct::Open {
            other: at.name.to_owned(),
            remote: at.remote,
        }),
        Pick::Merge => merge_action(at, current),
        Pick::Rebase => rebase_action(at, current),
        Pick::InteractiveRebase => interactive_rebase_action(at),
        Pick::ChangesAdded => compare_branches_action(here, at, true),
        Pick::Compare => compare_branches_action(here, at, false),
        Pick::Checkout | Pick::Push | Pick::Reset(_) | Pick::Interaction => return None,
    })
}
