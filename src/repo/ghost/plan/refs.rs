//! Force-push, branch deletion and commit-drop previews.

use super::*;

impl Ctx<'_> {
    pub(super) fn force_push(&self, name: &str) -> Result<Outcome, PreviewFailure> {
        let branch = self
            .facts
            .local(name)
            .ok_or_else(|| format!("there is no branch {name} here"))?;
        let up = self
            .facts
            .upstream(branch)
            .ok_or_else(|| format!("{name} tracks no branch on a remote"))?;
        let (old, new) = (up.tracking.target.clone(), branch.target.clone());
        let tracking = format!("refs/remotes/{}", up.tracking.name);
        let mut out = Outcome {
            moves: vec![RefMove {
                full: tracking.clone(),
                from: Some(old.clone()),
                to: Some(new.clone()),
            }],
            from_remote: true,
            ..Outcome::default()
        };
        let leaving = dry_run::leaving_with_witness(self.repo, &old, &new, LOST_LIMIT + 1)?;
        out.lost_more = leaving.len() > LOST_LIMIT;
        out.lost = git::commits_of(self.repo, &leaving[..leaving.len().min(LOST_LIMIT)])?;
        self.check()?;
        out.vanish =
            dry_run::unreachable_with_witness(self.repo, &[old], &[new], &[tracking], LOST_LIMIT)?
                .into_iter()
                .collect();
        if out.lost.is_empty() {
            out.notes.push(format!(
                "{} loses nothing: a plain push would do.",
                up.tracking.name
            ));
        }
        out.notes.push(format!(
            "As of the last fetch of {}: with the lease, the push is refused if someone pushed since.",
            up.remote
        ));
        Ok(out)
    }

    pub(super) fn delete_branch(
        &self,
        name: &str,
        remote: bool,
    ) -> Result<Outcome, PreviewFailure> {
        let branch = self
            .facts
            .local(name)
            .ok_or_else(|| format!("there is no branch {name} here"))?;
        let up = self.facts.upstream(branch);
        let mut out = Outcome {
            moves: vec![RefMove {
                full: local_ref(name),
                from: Some(branch.target.clone()),
                to: None,
            }],
            ..Outcome::default()
        };
        let mut old = vec![branch.target.clone()];
        if remote && let Some(up) = &up {
            out.moves.push(RefMove {
                full: format!("refs/remotes/{}", up.tracking.name),
                from: Some(up.tracking.target.clone()),
                to: None,
            });
            old.push(up.tracking.target.clone());
        }
        let moved: Vec<String> = out.moves.iter().map(|m| m.full.clone()).collect();
        self.lose(&mut out, &old, &[], &moved)?;
        self.check()?;
        let head = self.facts.refs.head.as_str();
        // As `git branch -d` decides: merged into its upstream when it has one, else HEAD.
        let into = up.as_ref().map_or(head, |u| u.tracking.target.as_str());
        let merged = dry_run::is_ancestor(self.repo, &branch.target, into);
        if !merged {
            let what = if up.is_some() {
                "its upstream"
            } else {
                "the checked-out branch"
            };
            out.warnings.push(format!(
                "{name} isn't merged into {what}: Delete refuses unless Force (-D) is ticked, which leaves its own commits on no branch."
            ));
        }
        Ok(out)
    }

    pub(super) fn drop_commit(&self, hash: &str) -> Result<Outcome, PreviewFailure> {
        let branch = self.facts.current()?;
        let hash = git::resolve(self.repo, hash)?;
        let parent = git::resolve(self.repo, &format!("{hash}^"))
            .map_err(|_| "the first commit has no parent to rebase from".to_owned())?;
        let tip = local_ref(&branch.name);
        let range = format!("{hash}..{tip}");
        let picks: Vec<String> = git::run(
            Some(self.repo),
            &[
                "rev-list",
                "--reverse",
                "--topo-order",
                "--end-of-options",
                &range,
            ],
        )?
        .lines()
        .map(str::to_owned)
        .collect();
        let merges = git::run(
            Some(self.repo),
            &[
                "rev-list",
                "--merges",
                "--count",
                "--end-of-options",
                &range,
            ],
        )?;
        if merges.trim() != "0" {
            return Err(
                ("merges came after the commit, which this would flatten".to_owned()).into(),
            );
        }
        let mut out = Outcome {
            dropped: git::commits_of(self.repo, std::slice::from_ref(&hash))?
                .into_iter()
                .next(),
            ..Outcome::default()
        };
        if self.facts.dirty {
            out.notes
                .push("Uncommitted changes are stashed first and put back after.".to_owned());
        }
        self.rewrite(
            &Rewrite {
                branch,
                onto: &parent,
                base: &hash,
                picks,
                interactive: true,
                along: Vec::new(),
            },
            &mut out,
        )?;
        Ok(out)
    }
}
