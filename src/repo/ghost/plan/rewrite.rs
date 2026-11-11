//! Replay simulation, conflict accounting and stacked-ref updates.

use super::*;

impl Ctx<'_> {
    /// The commits of `w` replayed: the copies drawn, where it stops, the branch moved (and
    /// with `rebase.updateRefs` the branches on its commits), what it leaves behind and the
    /// files that change.
    pub(super) fn rewrite(&self, w: &Rewrite<'_>, out: &mut Outcome) -> Result<(), PreviewFailure> {
        let tip = local_ref(&w.branch.name);
        if w.picks.len() > PICKS_LIMIT {
            return Err((format!(
                "{} commits to replay, too many to preview (at most {PICKS_LIMIT})",
                w.picks.len()
            ))
            .into());
        }
        if self.config_set("rebase.rebaseMerges") && self.has_merges(w.base, &tip)? {
            return Err(
                ("rebase.rebaseMerges is set and merges are among the commits: git keeps them, which the preview can't replay yet"
                    .to_owned()).into(),
            );
        }
        let replay = dry_run::replay_with_witness(
            self.repo,
            w.onto,
            (w.base, &tip),
            &w.picks,
            self.version,
            self.cancel,
        )?;
        out.engine = Some(replay.engine);
        self.check()?;
        let originals = git::commits_of(self.repo, &w.picks)?;
        let find = |hash: &str| originals.iter().find(|c| c.hash == hash);
        // An interactive rebase stops on a commit that becomes empty, asking what to do.
        let emptied = w
            .interactive
            .then(|| replay.steps.iter().position(|s| s.copy.is_none()))
            .flatten();
        let steps = &replay.steps[..emptied.unwrap_or(replay.steps.len())];
        out.tree = steps.last().map(|s| s.tree.clone());
        let mut parent = replay.onto.clone();
        let mut copies = Vec::new();
        for step in steps {
            let (Some(copy), Some(original)) = (&step.copy, find(&step.original)) else {
                continue;
            };
            copies.push(Commit {
                hash: copy.clone(),
                parents: vec![parent.clone()],
                refs: Vec::new(),
                ..original.clone()
            });
            parent.clone_from(copy);
        }
        out.copies = copies.len();
        copies.reverse();
        copies.append(&mut out.created);
        out.created = copies;
        let dropped = steps.iter().filter(|s| s.copy.is_none()).count();
        if dropped > 0 {
            out.notes.push(format!(
                "{dropped} commit{} dropped: {} already there.",
                if dropped == 1 { " is" } else { "s are" },
                if dropped == 1 {
                    "its change is"
                } else {
                    "their changes are"
                }
            ));
        }
        let stop = match (&replay.stop, emptied) {
            (_, Some(i)) => Some((i, replay.steps[i].original.clone(), Vec::new())),
            (Some(stop), None) => Some((stop.step, stop.commit.clone(), stop.files.clone())),
            (None, None) => None,
        };
        if let Some((step, commit, files)) = stop {
            let why = if files.is_empty() {
                "The rebase stops there, asking whether to keep the commit that became empty; the branch doesn't move yet."
            } else {
                "The rebase stops there until the conflicts are resolved; the branch doesn't move yet."
            };
            out.conflict = Some(Conflict {
                step: step + 1,
                of: w.picks.len(),
                commit: find(&commit).cloned(),
                files,
            });
            out.notes.push(why.to_owned());
            let waits = out.created.iter().take(out.copies).next().map_or_else(
                || short(&replay.onto).to_owned(),
                |c| short(&c.hash).to_owned(),
            );
            out.notes.push(format!(
                "Meanwhile HEAD is detached at {waits}, mid-rebase."
            ));
            return Ok(());
        }
        let head = w.branch.target.clone();
        let new_tip = replay.tip().to_owned();
        out.moves.push(RefMove {
            full: tip.clone(),
            from: Some(head.clone()),
            to: Some(new_tip.clone()),
        });
        if self.config_set("rebase.updateRefs") {
            out.moves.extend(self.updated_refs(w, &replay));
        }
        let mut old = vec![head.clone()];
        let mut new = vec![replay.onto.clone()];
        let mut moved = vec!["HEAD".to_owned()];
        for m in &out.moves {
            moved.push(m.full.clone());
            old.extend(m.from.clone());
            new.extend(m.to.clone().filter(|t| t.as_str() != new_tip.as_str()));
        }
        for (full, from, to) in &w.along {
            moved.push(full.clone());
            old.push(from.clone());
            new.push(to.clone());
        }
        // Copies aren't in the repository when made in a scratch clone: their originals'
        // parents (where they go) stand in, as `onto` does for the branch.
        let known: Vec<String> = new.into_iter().filter(|h| self.has_commit(h)).collect();
        old.retain(|h| self.has_commit(h));
        old.dedup();
        self.check()?;
        self.lose(out, &old, &known, &moved)?;
        let rewritten: HashSet<&str> = steps
            .iter()
            .filter(|s| s.copy.is_some())
            .map(|s| s.original.as_str())
            .collect();
        out.replaced = out
            .lost
            .iter()
            .filter(|c| rewritten.contains(c.hash.as_str()))
            .count();
        if replay.engine == Engine::Scratch {
            out.notes.push(format!(
                "The files that change aren't listed: git {} replays in a scratch clone.",
                self.version
            ));
        } else {
            self.check()?;
            out.files = Some(git::diff_files(self.repo, &head, Some(&new_tip))?);
        }
        Ok(())
    }

    /// Whether the commits from `base` to `tip` include a merge.
    pub(super) fn has_merges(&self, base: &str, tip: &str) -> Result<bool, String> {
        let range = format!("{base}..{tip}");
        let count = git::run(
            Some(self.repo),
            &[
                "rev-list",
                "--merges",
                "--count",
                "--end-of-options",
                &range,
            ],
        )?;
        Ok(count.trim() != "0")
    }

    /// With `rebase.updateRefs`: the other local branches on the rebased commits move to the
    /// copies, as git moves them (one on a dropped commit to the copy before it).
    pub(super) fn updated_refs(&self, w: &Rewrite<'_>, replay: &dry_run::Replay) -> Vec<RefMove> {
        let mut moves = Vec::new();
        for b in &self.facts.refs.local {
            if b.name == w.branch.name {
                continue;
            }
            let Some(i) = replay.steps.iter().position(|s| s.original == b.target) else {
                continue;
            };
            let to = replay.steps[..=i]
                .iter()
                .rev()
                .find_map(|s| s.copy.clone())
                .unwrap_or_else(|| replay.onto.clone());
            moves.push(RefMove {
                full: local_ref(&b.name),
                from: Some(b.target.clone()),
                to: Some(to),
            });
        }
        moves
    }
}
