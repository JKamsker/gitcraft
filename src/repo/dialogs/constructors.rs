//! Dialog construction and confirmation configuration.

use super::*;

impl Dialog {
    pub fn create_branch(at: &str, at_label: &str) -> Self {
        Dialog::CreateBranch {
            at: at.to_owned(),
            at_label: at_label.to_owned(),
            name: String::new(),
            checkout: true,
            focus: true,
        }
    }

    pub fn confirm(
        title: &str,
        body: String,
        button: &str,
        ops: Vec<Vec<String>>,
        alternative: Option<(String, Vec<Vec<String>>)>,
    ) -> Self {
        Dialog::Confirm {
            title: title.to_owned(),
            body,
            button: button.to_owned(),
            ops,
            alternative,
            use_alternative: false,
            refresh: Refresh::All,
            undo: Undo::None,
            preview: Box::default(),
            safe: false,
        }
    }

    /// Has a confirmation show what its operation changes, read from git with `ask`; `heading`
    /// goes over the commits leaving.
    pub fn preview(mut self, ask: crate::git::PreviewAsk, heading: String) -> Self {
        if let Dialog::Confirm { preview, .. } = &mut self {
            preview.ask = ask;
            preview.heading = heading;
        }
        self
    }

    /// What the preview calls the files lost (`Unstaged changes lost`).
    pub fn files_heading(mut self, heading: &str) -> Self {
        if let Dialog::Confirm { preview, .. } = &mut self {
            heading.clone_into(&mut preview.files_heading);
        }
        self
    }

    /// [`Self::preview`] of an (ask, heading) pair.
    pub fn preview_of(self, (ask, heading): (crate::git::PreviewAsk, String)) -> Self {
        self.preview(ask, heading)
    }

    /// Has a confirmation take `action` rather than run its ops; `described` says what it runs.
    pub fn instead(mut self, action: Action, described: Vec<String>) -> Self {
        if let Dialog::Confirm { preview, .. } = &mut self {
            preview.instead = Some(Instead { action, described });
        }
        self
    }

    /// Confirm through a guarded action while still showing this dialog's Git commands.
    pub(crate) fn instead_ops(mut self, action: Action) -> Self {
        if let Dialog::Confirm { preview, ops, .. } = &mut self {
            preview.instead = Some(Instead {
                action,
                described: ops
                    .iter()
                    .filter(|op| !super::super::operations::is_expectation(op))
                    .map(|op| preview::command_line(op))
                    .collect(),
            });
        }
        self
    }

    /// A confirmation of something that loses nothing: its button is not the danger colour.
    pub fn safe(mut self) -> Self {
        if let Dialog::Confirm { safe, .. } = &mut self {
            *safe = true;
        }
        self
    }

    /// Makes a confirmed operation undoable.
    pub fn undo(mut self, how: Undo) -> Self {
        if let Dialog::Confirm { undo, .. } = &mut self {
            *undo = how;
        }
        self
    }

    /// The checklist of `candidates` to delete; `checking` while which are merged is still being
    /// read, `error` when reading it failed.
    pub fn cleanup(
        candidates: Vec<crate::repo::branch_cleanup::Candidate>,
        checking: bool,
        error: Option<String>,
    ) -> Self {
        Dialog::Cleanup(Box::new(cleanup::CleanupDialog::new(
            candidates, checking, error,
        )))
    }

    pub fn credentials(url: &str, refused: bool) -> Self {
        Dialog::Credentials {
            url: url.to_owned(),
            endpoints: vec![url.to_owned()],
            refused,
            username: String::new(),
            password: String::new(),
            focus: true,
        }
    }

    /// Stash everything (or the files picked in the changes, when there are some).
    pub fn stash() -> Self {
        Self::stash_as(StashChoice::Everything)
    }

    /// The stash dialog with `choice` chosen.
    pub fn stash_as(choice: StashChoice) -> Self {
        Dialog::Stash(stash::StashDialog::new(choice, Vec::new()))
    }

    /// The stash dialog for just `files`.
    pub fn stash_files(files: Vec<String>) -> Self {
        Dialog::Stash(stash::StashDialog::new(StashChoice::Files, files))
    }

    pub fn tag(at: &str, at_label: &str, remote: Option<String>) -> Self {
        Dialog::Tag {
            at: at.to_owned(),
            at_label: at_label.to_owned(),
            name: String::new(),
            message: String::new(),
            remote,
            push: false,
            release: false,
            focus: true,
        }
    }

    pub fn rename_branch(old: &str) -> Self {
        Dialog::RenameBranch {
            old: old.to_owned(),
            new: old.to_owned(),
            focus: true,
        }
    }

    pub fn remote(original: Option<(String, String)>) -> Self {
        let (name, url) = original.clone().unwrap_or_default();
        Dialog::Remote {
            original,
            name,
            url,
            focus: true,
        }
    }

    pub fn set_upstream(branch: &str, remote: String, name: &str, undo: Vec<Vec<String>>) -> Self {
        Dialog::SetUpstream {
            branch: branch.to_owned(),
            remote,
            name: name.to_owned(),
            undo,
            focus: true,
        }
    }

    pub fn create_pr(noun: &str, branch: &str, base: &str, title: &str, body: &str) -> Self {
        Dialog::CreatePr {
            noun: noun.to_owned(),
            branch: branch.to_owned(),
            base: base.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            draft: false,
            reviewers: Vec::new(),
            focus: true,
        }
    }

    /// Add a worktree, checking out `branch` (or a new branch from `base` when `None`).
    pub fn add_worktree(repo: &Path, branch: Option<&str>, base: &str) -> Self {
        let mut dialog = Dialog::AddWorktree {
            repo: repo.to_path_buf(),
            root: crate::remote::git_path(&crate::git::default_worktree_root(repo)),
            path: String::new(),
            path_edited: false,
            new_branch: branch.is_none(),
            branch: branch.unwrap_or_default().to_owned(),
            new_name: String::new(),
            base: base.to_owned(),
            base_label: format!("HEAD ({})", base.get(..8).unwrap_or(base)),
            focus: true,
        };
        dialog.suggest_path();
        dialog
    }

    /// A worktree on a new branch starting at the commit `hash`.
    pub fn worktree_at(repo: &Path, hash: &str) -> Self {
        let mut dialog = Self::add_worktree(repo, None, hash);
        if let Dialog::AddWorktree { base_label, .. } = &mut dialog {
            *base_label = hash.get(..8).unwrap_or(hash).to_owned();
        }
        dialog
    }

    /// Starts a bisect from `hash`: good with HEAD as the bad end, or bad with a good end to
    /// pick.
    pub fn start_bisect(hash: &str, this_good: bool) -> Self {
        Dialog::StartBisect {
            hash: hash.to_owned(),
            this_good,
            other: if this_good { "HEAD" } else { "" }.to_owned(),
            focus: true,
        }
    }

    /// Runs a command for the bisect, `last` (the one run before) offered.
    pub fn bisect_run(last: &str) -> Self {
        Dialog::BisectRun {
            command: last.to_owned(),
            focus: true,
        }
    }

    pub fn refresh(mut self, what: Refresh) -> Self {
        if let Dialog::Confirm { refresh, .. } = &mut self {
            *refresh = what;
        }
        self
    }
}
