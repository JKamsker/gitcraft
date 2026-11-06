//! What the guide of a new repository offers: make the first commit, add a remote, publish.
//! Decided from what was read of the repository alone, so that it can be tested without a
//! window.

use crate::git::{Identity, Refs, Remote, WorkStatus};

/// A repository with at most this many commits, nothing of it published yet, still shows the
/// first steps (below its commits); past that, it is not new any more.
pub(crate) const NEW_REPO_COMMITS: usize = 1;

/// What the first steps are decided from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    /// Commits in the history read.
    pub commits: usize,
    /// The working tree has changes (untracked files included).
    pub has_changes: bool,
    pub remotes: usize,
    /// A branch with commits is checked out (not a detached HEAD, not an unborn branch).
    pub on_branch: bool,
    /// The checked-out branch tracks a remote branch, or some remote branch is known already.
    pub published: bool,
    /// git knows who commits: a name and an email.
    pub has_identity: bool,
}

impl Facts {
    /// The facts of a repository as the tab read it.
    pub(crate) fn read(
        commits: usize,
        refs: &Refs,
        work: &WorkStatus,
        remotes: &[Remote],
        identity: &Identity,
    ) -> Self {
        let upstream = refs
            .current
            .as_ref()
            .and_then(|c| refs.local.iter().find(|b| &b.name == c))
            .is_some_and(|b| !b.upstream.is_empty() && !b.upstream_gone());
        let remote_branches = refs.remotes.iter().any(|(_, list)| !list.is_empty());
        Self {
            commits,
            has_changes: !work.is_clean(),
            remotes: remotes.len(),
            on_branch: refs.current.is_some(),
            published: upstream || remote_branches,
            has_identity: !identity.name.trim().is_empty() && !identity.email.trim().is_empty(),
        }
    }
}

/// Why Publish waits for the first commit.
pub(crate) const NEEDS_COMMIT: &str = "Needs the first commit";
/// Why the first commit waits for an identity.
pub(crate) const NEEDS_IDENTITY: &str =
    "Set your name and email first (Settings → Profile, or git config user.name / user.email)";

/// Where a step stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepState {
    /// To do, and it can be done now.
    Todo,
    Done,
    /// It can't be done yet, for this reason.
    Blocked(&'static str),
}

/// The three first steps and what the first commit would be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FirstSteps {
    /// No commit yet: the history is empty.
    pub empty: bool,
    pub first_commit: StepState,
    /// No commit and nothing to commit: the first commit could only be an empty one.
    pub nothing_to_commit: bool,
    pub add_remote: StepState,
    pub publish: StepState,
}

/// The first steps to show, or `None` when the repository is not new or they are all done.
pub(crate) fn first_steps(f: &Facts) -> Option<FirstSteps> {
    if f.commits > NEW_REPO_COMMITS || (f.commits > 0 && f.published) {
        return None;
    }
    let done_if = |done: bool| {
        if done {
            StepState::Done
        } else {
            StepState::Todo
        }
    };
    let publish = if f.commits == 0 {
        StepState::Blocked(NEEDS_COMMIT)
    } else if f.remotes == 0 {
        StepState::Blocked("Add a remote first")
    } else if !f.on_branch {
        StepState::Blocked("Check out a branch first")
    } else {
        StepState::Todo
    };
    Some(FirstSteps {
        empty: f.commits == 0,
        first_commit: if f.commits == 0 && !f.has_identity {
            StepState::Blocked(NEEDS_IDENTITY)
        } else {
            done_if(f.commits > 0)
        },
        nothing_to_commit: f.commits == 0 && !f.has_changes,
        add_remote: done_if(f.remotes > 0),
        publish,
    })
}

/// What the tests take for steps that should be there but are not: nothing in it passes.
#[cfg(test)]
pub(crate) const MISSING: FirstSteps = FirstSteps {
    empty: false,
    first_commit: StepState::Blocked("missing"),
    nothing_to_commit: false,
    add_remote: StepState::Blocked("missing"),
    publish: StepState::Blocked("missing"),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(commits: usize, has_changes: bool, remotes: usize, published: bool) -> Facts {
        Facts {
            commits,
            has_changes,
            remotes,
            on_branch: commits > 0,
            published,
            has_identity: true,
        }
    }

    #[test]
    fn without_an_identity_the_first_commit_waits_for_one() {
        let nobody = Facts {
            has_identity: false,
            ..facts(0, true, 0, false)
        };
        let steps = first_steps(&nobody).unwrap_or(MISSING);
        assert_eq!(steps.first_commit, StepState::Blocked(NEEDS_IDENTITY));
        // Once committed (in the terminal, say), it is done whoever committed.
        let committed = Facts {
            has_identity: false,
            ..facts(1, false, 0, false)
        };
        let steps = first_steps(&committed).unwrap_or(MISSING);
        assert_eq!(steps.first_commit, StepState::Done);
    }

    #[test]
    fn an_empty_repository_shows_every_step_to_do_and_publish_waits_for_a_commit() {
        let steps = first_steps(&facts(0, false, 0, false)).unwrap_or(MISSING);
        assert!(steps.empty);
        assert_eq!(steps.first_commit, StepState::Todo);
        assert!(steps.nothing_to_commit);
        assert_eq!(steps.add_remote, StepState::Todo);
        assert_eq!(steps.publish, StepState::Blocked(NEEDS_COMMIT));
    }

    #[test]
    fn changes_to_commit_are_not_nothing_to_commit() {
        let steps = first_steps(&facts(0, true, 0, false)).unwrap_or(MISSING);
        assert!(!steps.nothing_to_commit);
        assert_eq!(steps.first_commit, StepState::Todo);
    }

    #[test]
    fn a_remote_marks_its_step_done_and_publish_still_waits_for_a_commit() {
        let steps = first_steps(&facts(0, false, 2, false)).unwrap_or(MISSING);
        assert_eq!(steps.add_remote, StepState::Done);
        assert_eq!(steps.publish, StepState::Blocked(NEEDS_COMMIT));
    }

    #[test]
    fn after_the_first_commit_publish_waits_for_a_remote_then_can_go() {
        let no_remote = first_steps(&facts(1, false, 0, false)).unwrap_or(MISSING);
        assert!(!no_remote.empty);
        assert_eq!(no_remote.first_commit, StepState::Done);
        assert!(!no_remote.nothing_to_commit);
        assert_eq!(no_remote.publish, StepState::Blocked("Add a remote first"));
        let remote = first_steps(&facts(1, true, 1, false)).unwrap_or(MISSING);
        assert_eq!(remote.publish, StepState::Todo);
    }

    #[test]
    fn a_detached_head_cannot_be_published() {
        let detached = Facts {
            on_branch: false,
            ..facts(1, false, 1, false)
        };
        let steps = first_steps(&detached).unwrap_or(MISSING);
        assert_eq!(
            steps.publish,
            StepState::Blocked("Check out a branch first")
        );
    }

    #[test]
    fn a_published_or_older_repository_shows_no_steps() {
        assert_eq!(first_steps(&facts(1, false, 1, true)), None);
        assert_eq!(
            first_steps(&facts(NEW_REPO_COMMITS + 1, false, 0, false)),
            None
        );
        assert_eq!(first_steps(&facts(5_000, true, 3, false)), None);
        assert_eq!(first_steps(&facts(usize::MAX, false, 0, false)), None);
    }
}
