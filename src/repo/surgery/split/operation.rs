//! Split preparation, commit assembly and tab mutation lifecycle.

use super::*;

/// The trees of a split of `hash` (on `parent`): the parent's, then each new commit's but the
/// last the parent with its `patches` applied, the last the old commit's tree. Fails, saying
/// which, when a patch doesn't apply or leaves a commit with no change.
pub(crate) fn prepare(
    repo: &Path,
    hash: &str,
    parent: &str,
    patches: &[String],
) -> Result<Checked, String> {
    let mut trees = vec![git::tree_of(repo, parent)?];
    for (g, patch) in patches.iter().enumerate() {
        let file = git::save_patch_for(repo, patch)?;
        let tree = git::tree_after(repo, parent, &[file]).map_err(|e| {
            crate::i18n::message(
                "repo-workflows-surgery-prepare-1a3632ec",
                &[("g-1-1", (g + 1).into()), ("e-2", (e).clone().into())],
            )
        })?;
        trees.push(tree);
    }
    trees.push(git::tree_of(repo, hash)?);
    let mut patches_out = Vec::new();
    for (g, pair) in trees.windows(2).enumerate() {
        let [before, after] = pair else { continue };
        if before == after {
            return Err(crate::i18n::message(
                "repo-workflows-surgery-prepare-9ee5bdbf",
                &[("g-1-1", (g + 1).into())],
            ));
        }
        patches_out.push((
            git::patch_between(repo, before, after)?,
            git::paths_between(repo, before, after)?,
        ));
    }
    Ok(Checked {
        trees,
        preview: Preview::new(&patches_out),
    })
}

/// Makes the new commits of `trees` (the parent's first) on `parent`, each by the old commit
/// `hash`'s author with its message.
pub(crate) fn make_commits(
    repo: &Path,
    hash: &str,
    parent: &str,
    trees: &[String],
    messages: &[String],
) -> Result<Vec<String>, String> {
    let author = git::author_of(repo, hash)?;
    let mut base = parent.to_owned();
    let mut commits = Vec::new();
    for (tree, message) in trees.iter().skip(1).zip(messages) {
        base = git::commit_tree(repo, tree, &base, message, Some(&author))?;
        commits.push(base.clone());
    }
    Ok(commits)
}

impl RepoTab {
    /// Reads commit `hash` and opens the split editor on it.
    pub(in crate::repo) fn open_split(&mut self, hash: String, ctx: &egui::Context) {
        if let Some(why) = self.surgery_refusal(&hash) {
            self.status = Some(crate::repo::Notice::plain((
                format!("Split commit: {why}"),
                true,
            )));
            return;
        }
        if self.absorb.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                "Split commit: close the staged fixes editor first".to_owned(),
                true,
            )));
            return;
        }
        self.status = Some(crate::repo::Notice::plain((
            format!("Split commit: reading {}…", short(&hash)),
            false,
        )));
        let path = self.path.clone();
        self.then(
            ctx,
            move || read(&path, &hash),
            |tab, result, _| match result {
                // Another editor opened meanwhile is left alone.
                Ok(_) if tab.rebase.is_some() || tab.conflict.is_some() => tab.status = None,
                Ok(read) => {
                    tab.status = None;
                    tab.diff = None;
                    tab.file_view = None;
                    tab.split = Some(Box::new(SplitEditor::new(read)));
                }
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("Split commit: {e}"),
                        true,
                    )))
                }
            },
        );
    }

    /// Makes the trees of the split being edited and shows what each new commit changes.
    pub(in crate::repo) fn check_split(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        if matches!(editor.mode, Mode::Move(_)) {
            self.check_move(ctx);
            return;
        }
        if editor.problem().is_some() {
            return;
        }
        editor.busy = true;
        editor.error = None;
        let (id, hash, parent) = (editor.id, editor.hash.clone(), editor.parent.clone());
        let patches = editor.cumulative_patches();
        let path = self.path.clone();
        self.then(
            ctx,
            move || prepare(&path, &hash, &parent, &patches),
            move |tab, result, _| {
                let Some(editor) = tab.split.as_mut().filter(|e| e.id == id) else {
                    return;
                };
                editor.busy = false;
                match result {
                    Ok(checked) => editor.checked = Some(checked),
                    Err(e) => editor.error = Some(e),
                }
            },
        );
    }

    /// Makes the checked commits, with the messages as they are now, and puts them in place of
    /// the one split, replaying those after it. The editor stays open until that has started.
    pub(in crate::repo) fn run_split(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        let Some(checked) = editor.checked.clone() else {
            return;
        };
        if editor.problem().is_some() {
            return;
        }
        if matches!(editor.mode, Mode::Move(_)) {
            self.run_move(ctx);
            return;
        }
        editor.busy = true;
        editor.error = None;
        let (id, hash, parent, messages) = (
            editor.id,
            editor.hash.clone(),
            editor.parent.clone(),
            editor.messages(),
        );
        let label = format!("Split {} into {}", short(&hash), messages.len());
        let path = self.path.clone();
        self.rewrite_with(
            label,
            parent.clone(),
            move |entries| {
                let Some((first, rest)) = entries.split_first() else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-split-609071bf").to_owned(),
                    );
                };
                if first.commit.hash != hash {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-split-6da7c69e").to_owned(),
                    );
                }
                let made = make_commits(&path, &hash, &parent, &checked.trees, &messages)?;
                let mut todo: Vec<_> = made
                    .into_iter()
                    .zip(&messages)
                    .map(|(made, message)| prepared(made, message, &first.commit))
                    .collect();
                todo.extend(rest.iter().cloned());
                Ok(todo)
            },
            (Vec::new(), Vec::new()),
            Undo::HeadMove { mode: "--keep" },
            move |tab, started| {
                // Cancelled meanwhile: nothing runs.
                let Some(editor) = tab.split.as_mut().filter(|e| e.id == id) else {
                    return false;
                };
                match started {
                    Ok(()) => {
                        tab.split = None;
                        true
                    }
                    Err(e) => {
                        editor.busy = false;
                        editor.error = Some(e);
                        false
                    }
                }
            },
            ctx,
        );
    }
}
