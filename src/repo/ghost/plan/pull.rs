//! Pull preview policy, merge simulation and upstream-base selection.

use super::*;

impl Ctx<'_> {
    pub(super) fn pull(&self, mode: PullMode) -> Result<Outcome, PreviewFailure> {
        let branch = self.facts.current()?;
        let up = self.facts.upstream(branch).ok_or_else(|| {
            format!(
                "{} tracks no branch on a remote: nothing to pull",
                branch.name
            )
        })?;
        let mut out = Outcome::default();
        // Never the network from a hover: asking the remote can hang, ask for a passphrase or
        // a key's touch, or download a lot, for a pointer passing by. What the last fetch
        // brought (the background fetch keeps it recent) stands for what the pull fetches.
        out.notes.push(format!(
            "As of the last fetch of {}: what was pushed there since isn't shown.",
            up.remote
        ));
        let head = branch.target.clone();
        let tracking = up.tracking.target.clone();
        let new = tracking.clone();
        self.check()?;
        if dry_run::is_ancestor(self.repo, &new, &head) {
            out.notes
                .push("Already up to date: the branch has everything its remote has.".to_owned());
            return Ok(out);
        }
        if dry_run::is_ancestor(self.repo, &head, &new) {
            out.moves.push(RefMove {
                full: local_ref(&branch.name),
                from: Some(head.clone()),
                to: Some(new.clone()),
            });
            out.notes
                .push("Fast-forward: the branch moves to its remote's commit.".to_owned());
            self.check()?;
            let moved = [
                local_ref(&branch.name),
                "HEAD".to_owned(),
                tracking_ref(&up),
            ];
            self.lose(
                &mut out,
                &[head.clone(), tracking.clone()],
                std::slice::from_ref(&new),
                &moved,
            )?;
            out.files = Some(git::diff_files(self.repo, &head, Some(&new))?);
            return Ok(out);
        }
        match mode {
            PullMode::FastForwardOnly => {
                let fetched = if new != tracking {
                    format!(" (only {} moves, with what it fetches)", up.tracking.name)
                } else {
                    String::new()
                };
                out.refused = Some(format!(
                    "{} and {} have diverged, so a fast-forward-only pull refuses{fetched}",
                    branch.name, up.tracking.name
                ));
                Ok(out)
            }
            PullMode::Merge => self.pull_merge(branch, &up, &new, out),
            PullMode::Rebase => {
                if self.facts.dirty {
                    out.notes.push(
                        "Uncommitted changes are stashed first and put back after.".to_owned(),
                    );
                }
                let base = self.pull_rebase_base(&up, &new, &head);
                self.check()?;
                let picks = dry_run::rebase_picks(self.repo, &base, &local_ref(&branch.name))?;
                self.rewrite(
                    &Rewrite {
                        branch,
                        onto: &new,
                        base: &base,
                        picks,
                        interactive: false,
                        along: vec![(tracking_ref(&up), tracking.clone(), new.clone())],
                    },
                    &mut out,
                )?;
                Ok(out)
            }
        }
    }

    /// The merge a diverged pull makes: its commit drawn, or where it conflicts.
    pub(super) fn pull_merge(
        &self,
        branch: &Branch,
        up: &Upstream<'_>,
        new: &str,
        mut out: Outcome,
    ) -> Result<Outcome, PreviewFailure> {
        let head = branch.target.as_str();
        let merged = dry_run::merge_with_witness(self.repo, head, new, self.version)?;
        out.engine = Some(merged.engine);
        out.tree = Some(merged.tree.clone()).filter(|t| !t.is_empty());
        if !merged.conflicts.is_empty() {
            out.conflict = Some(Conflict {
                step: 1,
                of: 1,
                commit: None,
                files: merged.conflicts,
            });
            return Ok(out);
        }
        let you = git::identity(self.repo);
        out.created.insert(
            0,
            Commit {
                hash: GHOST_MERGE.to_owned(),
                parents: vec![head.to_owned(), new.to_owned()],
                summary: format!("Merge branch '{}' of {}", up.name, up.remote),
                author: you.name,
                email: you.email,
                time: git::now(),
                ..Commit::default()
            },
        );
        out.copies = 1;
        out.moves.push(RefMove {
            full: local_ref(&branch.name),
            from: Some(head.to_owned()),
            to: Some(GHOST_MERGE.to_owned()),
        });
        if merged.engine != Engine::Scratch {
            self.check()?;
            out.files = Some(git::diff_files(self.repo, head, Some(&merged.tree))?);
        }
        Ok(out)
    }

    pub(super) fn has_commit(&self, oid: &str) -> bool {
        git::run(
            Some(self.repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    }

    /// The upstream `git pull --rebase` rebases from (`git rebase --onto new <it>`), as
    /// git's pull works it out: the fork point of HEAD in the remote-tracking branch's history
    /// (read before the fetch, as git does), unless that is just where HEAD and `new` meet
    /// anyway, or there is none: then `new` itself, whose commits are compared with the
    /// branch's to leave out those already upstream.
    pub(super) fn pull_rebase_base(&self, up: &Upstream<'_>, new: &str, head: &str) -> String {
        let fork = git::run(
            Some(self.repo),
            &["merge-base", "--fork-point", &tracking_ref(up), head],
        )
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty());
        let Some(fork) = fork else {
            return new.to_owned();
        };
        let meet = git::run(
            Some(self.repo),
            &["merge-base", "--octopus", head, new, &fork],
        )
        .map(|s| s.trim().to_owned());
        if meet.as_deref() == Ok(fork.as_str()) {
            new.to_owned()
        } else {
            fork
        }
    }

    /// Whether the git config `key` is set to anything but false.
    pub(super) fn config_set(&self, key: &str) -> bool {
        git::run(Some(self.repo), &["config", "--get", key]).is_ok_and(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "" | "false" | "no" | "off" | "0"
            )
        })
    }
}
