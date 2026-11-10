//! Move-changes preparation, checks and mutation settlement.

use super::*;

impl RepoTab {
    /// Reads commit `hash` and opens the editor to move some of its changes elsewhere.
    pub(in crate::repo) fn open_move(&mut self, hash: String, ctx: &egui::Context) {
        if let Some(why) = self.surgery_refusal(&hash) {
            self.status = Some(crate::repo::Notice::plain((
                format!("Move changes: {why}"),
                true,
            )));
            return;
        }
        let line = quick_edit::first_parents(&self.history, &self.history_index, &self.refs.head);
        let Some(depth) = line.iter().position(|c| c.hash == hash) else {
            return;
        };
        let targets = targets(&line, depth);
        // Neither a merge nor the first commit ended the line: its history isn't all loaded.
        let truncated = line.iter().all(|c| c.parents.len() == 1);
        self.status = Some(crate::repo::Notice::plain((
            format!("Move changes: reading {}…", short(&hash)),
            false,
        )));
        let path = self.path.clone();
        self.then(
            ctx,
            move || crate::repo::surgery::split::read(&path, &hash),
            move |tab, result: Result<Read, String>, _| match result {
                Ok(_) if tab.rebase.is_some() || tab.conflict.is_some() => tab.status = None,
                Ok(read) => {
                    tab.status = None;
                    tab.diff = None;
                    tab.file_view = None;
                    let mut editor = SplitEditor::moving(read, targets, depth);
                    if let Mode::Move(m) = &mut editor.mode {
                        m.truncated = truncated;
                    }
                    tab.split = Some(Box::new(editor));
                }
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("Move changes: {e}"),
                        true,
                    )))
                }
            },
        );
    }

    /// Makes the trees of what stays and what moves, and shows both.
    pub(in crate::repo) fn check_move(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        if !matches!(editor.mode, Mode::Move(_)) || editor.problem().is_some() {
            return;
        }
        let stay = patchset::patch_of(&editor.files, |f, u| editor.group_of(f, u) == 0);
        let moved = patchset::patch_of(&editor.files, |f, u| editor.group_of(f, u) == 1);
        let (id, parent) = (editor.id, editor.parent.clone());
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.then(
            ctx,
            move || prepare(&path, &parent, &stay, &moved),
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

    /// Makes the commits of what stays and what moves and rewrites the branch from the older
    /// of the two commits; to the working changes, applies what moved once that is done.
    pub(in crate::repo) fn run_move(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        let (Mode::Move(m), Some(checked)) = (&editor.mode, editor.checked.clone()) else {
            return;
        };
        let target = m.target().cloned();
        // From below the older of the two.
        let onto = match &target {
            Some(t) if t.depth > m.depth => format!("{}^", t.hash),
            _ => editor.parent.clone(),
        };
        let (id, hash, parent, message) = (
            editor.id,
            editor.hash.clone(),
            editor.parent.clone(),
            editor.message.clone(),
        );
        let label = match &target {
            Some(t) => format!("Move changes from {} to {}", short(&hash), short(&t.hash)),
            None => format!("Move changes from {} to the working changes", short(&hash)),
        };
        let (after, undo) = if target.is_some() {
            (Vec::new(), Undo::HeadMove { mode: "--keep" })
        } else {
            (
                // Picked last by the rebase, which went through: out of the commits, staged.
                vec![["reset", "--soft", "HEAD^"].map(str::to_owned).to_vec()],
                // Back with the moved changes staged again in their old commit's place:
                // HEAD's tree before is HEAD's now with them.
                Undo::HeadMove { mode: "--soft" },
            )
        };
        let older = target.as_ref().is_some_and(|t| t.depth > m.depth);
        let onto_for_replay = onto.clone();
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.rewrite_with(
            label,
            onto,
            move |entries| {
                let Some(old) = entries.iter().find(|e| e.commit.hash == hash).cloned() else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-97891621").to_owned(),
                    );
                };
                let Some(head) = entries.last().map(|e| e.commit.hash.clone()) else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-145a2a94").to_owned(),
                    );
                };
                let author = git::author_of(&path, &hash)?;
                let [base, stay_tree, moved_tree] = &checked.trees[..] else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-d1994fc1").to_owned(),
                    );
                };
                let whole = git::tree_of(&path, &hash)?;
                let note = format!("fixup! moved from {}", short(&hash));
                let commit = |tree: &str, parent: &str, message: &str| {
                    git::commit_tree(&path, tree, parent, message, Some(&author))
                };
                // Each part made on the other, so that together they are the old commit and
                // neither repeats what the other does (a file both would create…).
                let (stay, moved) = if older {
                    // What moves goes first (into the older commit), what stays on top of it.
                    let moved = commit(moved_tree, &parent, &note)?;
                    let stay = (*moved_tree != whole)
                        .then(|| commit(&whole, &moved, &message))
                        .transpose()?;
                    (stay, moved)
                } else {
                    let stay = (stay_tree != base)
                        .then(|| commit(stay_tree, &parent, &message))
                        .transpose()?;
                    let moved = commit(&whole, stay.as_deref().unwrap_or(&parent), &note)?;
                    (stay, moved)
                };
                let todo = todo(
                    entries,
                    &hash,
                    stay.map(|s| prepared(s, &message, &old.commit)),
                    target.as_ref().map(|t| t.hash.as_str()),
                    prepared(moved, &note, &old.commit),
                )?;
                // Played through first: the branch must end with the files it has now. A commit
                // in between that undoes (or redoes) the moved lines would make them vanish.
                if let Some(end) =
                    crate::repo::surgery::replay::ends_as(&path, &onto_for_replay, &todo)?
                    && end != git::tree_of(&path, &head)?
                {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-9a1d6978").to_owned(),
                    );
                }
                Ok(todo)
            },
            (Vec::new(), after),
            undo,
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
