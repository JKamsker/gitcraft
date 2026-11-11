//! Branch publication, force-push review, and base-branch selection.

use super::*;

impl RepoTab {
    /// Push `b` (checked out or not), set what it tracks, and open or create its pull request.
    pub(super) fn publish_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        if let Some((to, push)) = self.push_branch(b)
            && crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-core-3408",
                &[("to", to.to_string().into())],
            ))
            .icon("arrow-up-from-line")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(push);
        }
        if ui.button(crate::i18n::text("repo-core-0088")).clicked() {
            acts.push(Action::RepositoryTools(
                crate::repo::repository_tools::Act::Routes(b.name.clone()),
            ));
        }
        self.force_push_item(ui, acts, b, idle);
        if let Some(remote) = self.default_remote()
            && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3409"))
                .icon("git-branch")
                .enabled(idle)
                .show(ui)
                .clicked()
        {
            let (remote, name) = self
                .split_upstream(&b.upstream)
                .map(|(r, n)| (r, n.to_owned()))
                .unwrap_or((remote, b.name.clone()));
            let undo = if b.upstream.is_empty() {
                ops(&[&["branch", "--unset-upstream", &b.name]])
            } else {
                self.track_ops(&b.name, &b.upstream)
            };
            acts.push(Action::Dialog(Dialog::set_upstream(
                &b.name, remote, &name, undo,
            )));
        }
        if !b.upstream.is_empty()
            && crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-core-0348",
                &[("upstream", b.upstream.to_string().into())],
            ))
            .icon("git-branch")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(self.unset_upstream_action(b));
        }
        let Some(forge) = self.forge() else { return };
        match self.prs.iter().find(|p| p.branch == b.name) {
            Some(pr) => {
                if crate::menu_item::MenuItem::new(crate::i18n::message(
                    "repo-core-3410",
                    &[
                        ("noun", forge.noun_label().to_string().into()),
                        ("number", format!("{}", pr.number).into()),
                    ],
                ))
                .icon("external-link")
                .show(ui)
                .clicked()
                {
                    acts.push(Action::OpenUrl(pr.url.clone()));
                }
            }
            None => {
                if crate::menu_item::MenuItem::new(crate::i18n::message(
                    "repo-core-3412",
                    &[("noun", forge.noun_label().to_string().into())],
                ))
                .icon("external-link")
                .enabled(idle)
                .show(ui)
                .clicked()
                {
                    // A branch made for an issue: its title, closing it once merged, and into the
                    // branch it started from.
                    let (title, body, base) =
                        self.issue_pull_request(&b.name).unwrap_or_else(|| {
                            let summary = self.history.iter().find(|c| c.hash == b.target);
                            (
                                summary.map(|c| c.summary.clone()).unwrap_or_default(),
                                String::new(),
                                self.issue_base().name,
                            )
                        });
                    acts.push(Action::Dialog(Dialog::create_pr(
                        forge.noun_label(),
                        &b.name,
                        &base,
                        &title,
                        &body,
                    )));
                }
            }
        }
    }

    /// Force push (with lease) of `b` to the branch it tracks, when that has commits `b`
    /// hasn't (a plain push is refused). Hovering it shows what would leave the remote.
    pub(super) fn force_push_item(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        if self.remotes.is_empty() {
            return;
        }
        let button = crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3413"))
            .icon("arrow-up-from-line")
            .enabled(idle)
            .show(ui);
        offer(&button, || GhostOp::ForcePush {
            branch: b.name.clone(),
        });
        if button.clicked() {
            acts.push(Action::RepositoryTools(
                crate::repo::repository_tools::Act::ForceBranch(b.name.clone(), b.target.clone()),
            ));
        }
    }

    /// The branch to compare the current one with: the local default base, or its remote one.
    /// The name of the branch pull requests go into (see [`Self::base_ref`]).
    pub(in crate::repo) fn base_branch_name(&self) -> Option<String> {
        self.base_ref().map(|b| b.name)
    }

    pub(in crate::repo) fn base_ref(&self) -> Option<OwnedBranch> {
        let base = self.default_base();
        if self.refs.local.iter().any(|b| b.name == base) {
            return Some(OwnedBranch {
                name: base,
                remote: false,
            });
        }
        let remote = format!("{}/{base}", self.default_remote()?);
        self.refs
            .remotes
            .iter()
            .flat_map(|(_, list)| list)
            .any(|b| b.name == remote)
            .then_some(OwnedBranch {
                name: remote,
                remote: true,
            })
    }

    /// The branch pull requests go into by default: main, master or develop on the default
    /// remote, else main.
    pub(in crate::repo) fn default_base(&self) -> String {
        let remote = self.default_remote().unwrap_or_else(|| "origin".to_owned());
        let names: Vec<&str> = self
            .remote_branches(&remote)
            .filter_map(|b| remote_short(&b.name, &remote))
            .collect();
        for candidate in ["main", "master", "develop"] {
            if names.contains(&candidate) {
                return candidate.to_owned();
            }
        }
        "main".to_owned()
    }
}
