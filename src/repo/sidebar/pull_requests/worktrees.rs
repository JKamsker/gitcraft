//! Pull-request worktree planning and creation on the repository machine.

use super::*;

impl RepoTab {
    /// The button (`in_menu`: the menu item) that opens the worktree of pull request `number`
    /// (proposing `branch`, at `sha`), making it first when there is none.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::repo) fn pr_worktree_button(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        number: u64,
        branch: &str,
        sha: &str,
        idle: bool,
        in_menu: bool,
    ) {
        let plan = self.pr_worktree_plan(number, branch, sha);
        let (enabled, tip) = match &plan {
            PrWorktree::Here => (false, crate::i18n::text("repo-core-3457").to_owned()),
            PrWorktree::Open(path) => (
                true,
                crate::i18n::message(
                    "repo-core-3458",
                    &[("path", crate::remote::display(path).to_string().into())],
                ),
            ),
            PrWorktree::Create { path, .. } => (
                idle,
                crate::i18n::message(
                    "repo-core-3459",
                    &[("path", crate::remote::display(path).to_string().into())],
                ),
            ),
        };
        let r = if in_menu {
            // Why it can't open, under it; what it would do, on hover.
            let here = matches!(plan, PrWorktree::Here);
            MenuItem::new(crate::i18n::text("repo-core-3460"))
                .icon("folder-git-2")
                .enabled(enabled || here)
                .refused(here.then_some(tip.as_str()))
                .show(ui)
                .on_hover_text(&tip)
                // Greyed out while an operation runs: what it would do, still.
                .on_disabled_hover_text(&tip)
        } else {
            let t = Tokens::get(ui.ctx());
            let button = egui::Button::image_and_text(
                icons::image("folder-git-2", 14.0, t.icon),
                crate::i18n::text("repo-core-3460"),
            )
            .min_size(vec2(0.0, 26.0));
            ui.add_enabled(enabled, button)
                .on_hover_text(&tip)
                .on_disabled_hover_text(tip)
        };
        if r.clicked() {
            acts.push(Action::OpenPrWorktree {
                number,
                branch: branch.to_owned(),
                sha: sha.to_owned(),
            });
        }
    }

    /// Opens the worktree of pull request `number` (proposing `branch`, at `sha`), making it
    /// first when there is none.
    pub(in crate::repo) fn open_pr_worktree(
        &mut self,
        number: u64,
        branch: &str,
        sha: &str,
        ctx: &egui::Context,
    ) {
        if self.hosting.is_some() && self.primary_forge_remote().is_none() {
            self.status = Some(crate::repo::Notice::plain((
                "The primary forge remote is unavailable; refresh before opening this pull request"
                    .into(),
                true,
            )));
            return;
        }
        match self.pr_worktree_plan(number, branch, sha) {
            PrWorktree::Here => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-3461",
                        &[("number", format!("{}", number).into())],
                    ),
                    false,
                )));
            }
            PrWorktree::Open(path) => {
                self.open_request = Some(crate::repo::OpenRequest::Worktree(path));
            }
            PrWorktree::Create { ops, path } => {
                if self.busy_op.is_some() {
                    return;
                }
                let label = crate::i18n::message(
                    "repo-core-3462",
                    &[("number", format!("{}", number).into())],
                );
                self.open_when_done = Some((label.clone(), path));
                self.run_ops(
                    &label,
                    ops,
                    crate::repo::Refresh::All,
                    crate::repo::OpKind::plain(),
                    ctx,
                );
            }
        }
    }

    /// How to get to the worktree of pull request `number`, proposing `branch` at `sha`: its
    /// branch where it is checked out, or a new worktree on it. The branch is the local one of
    /// that name, made from the remote branch when there is only that. A pull request from a
    /// fork that isn't a remote here is fetched from the forge into a branch of its own (named
    /// `pr-N` when one of the name is already another).
    pub(super) fn pr_worktree_plan(&self, number: u64, branch: &str, sha: &str) -> PrWorktree {
        // A branch is the pull request's only when it is at its commit (any, when that isn't
        // known): a fork's `main` is not ours.
        let at = |target: &str| sha.is_empty() || target == sha;
        let local_named = |name: &str| self.refs.local.iter().find(|b| b.name == name);
        let own_name = format!("pr-{number}");
        let local = local_named(branch);
        let remote = self.remote_branch_of(branch, sha).filter(|b| at(&b.target));
        // What the forge says is a name only where git takes it as one (never an option).
        let usable = plain_branch_name(branch);
        enum Source<'a> {
            Local(String),
            Track(&'a Branch),
            FromRemote(&'a Branch),
            Fetch,
        }
        let (name, source) = if let Some(b) = local.filter(|b| usable && at(&b.target)) {
            (b.name.clone(), Source::Local(b.name.clone()))
        } else if local_named(&own_name).is_some_and(|b| at(&b.target)) {
            (own_name.clone(), Source::Local(own_name.clone()))
        } else if let Some(remote) = remote.filter(|_| usable && local.is_none()) {
            (branch.to_owned(), Source::Track(remote))
        } else if let Some(remote) = remote {
            (own_name.clone(), Source::FromRemote(remote))
        } else if usable && local.is_none() {
            (branch.to_owned(), Source::Fetch)
        } else {
            (own_name.clone(), Source::Fetch)
        };
        if let Some(wt) = self
            .worktrees
            .iter()
            .filter(|w| w.prunable.is_none())
            .find(|w| w.branch.as_deref() == Some(name.as_str()))
        {
            return if wt.current {
                PrWorktree::Here
            } else {
                PrWorktree::Open(wt.path.clone())
            };
        }
        let path = self.free_folder(&self.worktree_folder(&name));
        // As git of the repository's machine names it.
        let path_arg = crate::remote::git_path(&path);
        let strings =
            |args: &[&str]| -> Vec<String> { args.iter().map(|a| (*a).to_owned()).collect() };
        // A worktree whose folder was deleted by hand still holds its branch for git.
        let mut ops = vec![strings(&["worktree", "prune"])];
        match source {
            Source::Local(name) => {
                ops.push(strings(&["worktree", "add", "--", &path_arg, &name]));
            }
            Source::Track(remote) => ops.push(strings(&[
                "worktree",
                "add",
                "--track",
                "-b",
                &name,
                "--",
                &path_arg,
                &remote.name,
            ])),
            Source::FromRemote(remote) => ops.push(strings(&[
                "worktree",
                "add",
                "-b",
                &name,
                "--",
                &path_arg,
                &remote.name,
            ])),
            Source::Fetch => {
                let pull_ref = match self.forge() {
                    Some(Forge::GitLab) => format!("refs/merge-requests/{number}/head"),
                    _ => format!("refs/pull/{number}/head"),
                };
                let from = self
                    .primary_forge_remote()
                    .map(str::to_owned)
                    .or_else(|| {
                        self.hosting
                            .is_none()
                            .then(|| self.default_remote())
                            .flatten()
                    })
                    .unwrap_or_else(|| "origin".to_owned());
                // Not forced: a `pr-N` of the user's own that went elsewhere is refused by git
                // rather than overwritten.
                ops.push(strings(&[
                    "fetch",
                    "--",
                    &from,
                    &format!("{pull_ref}:refs/heads/{name}"),
                ]));
                ops.push(strings(&["worktree", "add", "--", &path_arg, &name]));
            }
        }
        PrWorktree::Create { ops, path }
    }

    /// A branch folder under the repository's shared worktree root.
    pub(in crate::repo) fn worktree_folder(&self, branch: &str) -> PathBuf {
        let main = self
            .worktrees
            .first()
            .map_or(self.path.as_path(), |w| w.path.as_path());
        let root = crate::git::resolve_worktree_root(main, &self.worktree_root)
            .unwrap_or_else(|_| crate::git::default_worktree_root(main));
        crate::git::branch_worktree_folder(&root, branch)
    }

    /// `path`, or `path-2`, `path-3`… when there is something there already. On another
    /// machine, only the worktrees known here count: asking it would hold up the window, and
    /// git refuses a folder that isn't empty anyway.
    pub(in crate::repo) fn free_folder(&self, path: &Path) -> PathBuf {
        let taken = |p: &Path| {
            if crate::remote::is_remote(p) {
                self.worktrees.iter().any(|w| w.path == p)
            } else {
                p.exists()
            }
        };
        if !taken(path) {
            return path.to_path_buf();
        }
        // Appended to the path as written: a path of another machine keeps its `/` on Windows.
        let base = path.to_string_lossy();
        (2..1000)
            .map(|i| PathBuf::from(format!("{base}-{i}")))
            .find(|p| !taken(p))
            .unwrap_or_else(|| path.to_path_buf())
    }
}
