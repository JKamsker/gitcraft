//! The pills of the refs column: which branches and tags point at a commit.

use crate::git::Refs;
use crate::repo::sidebar;

/// One pill in the refs column: a branch (local, remote or both), a tag or a detached HEAD.
#[derive(Debug)]
pub(super) struct Badge {
    pub(super) label: String,
    pub(super) kind: BadgeKind,
    /// The branch exists locally.
    pub(super) local: bool,
    /// Remotes that have the branch at this commit.
    pub(super) remotes: Vec<String>,
    /// The checked-out branch (or a detached HEAD).
    pub(super) head: bool,
}

#[derive(Debug, PartialEq)]
pub(super) enum BadgeKind {
    Branch,
    Tag,
    DetachedHead,
    /// A PR tip fetched as an object, with no corresponding branch ref here.
    PullRequest,
}

impl Badge {
    /// The name git knows a branch pill by: the local branch, else `remote/branch`.
    pub(super) fn git_ref(&self) -> String {
        match self.remotes.first() {
            Some(remote) if !self.local => format!("{remote}/{}", self.label),
            _ => self.label.clone(),
        }
    }

    /// The pill as a screen reader says it, in a row's name: "branch dev, checked out".
    pub(super) fn spoken(&self) -> String {
        match self.kind {
            BadgeKind::Tag => format!("tag {}", self.label),
            BadgeKind::DetachedHead => "detached HEAD".to_owned(),
            BadgeKind::PullRequest => format!("pull request branch {}", self.label),
            BadgeKind::Branch if self.head => format!("branch {}, checked out", self.label),
            BadgeKind::Branch if !self.local => format!("remote branch {}", self.git_ref()),
            BadgeKind::Branch => format!("branch {}", self.label),
        }
    }

    /// The full names of the refs the pill stands for.
    pub(super) fn full_refs(&self) -> Vec<String> {
        match self.kind {
            BadgeKind::Tag => vec![format!("refs/tags/{}", self.label)],
            BadgeKind::DetachedHead => vec!["HEAD".to_owned()],
            BadgeKind::PullRequest => Vec::new(),
            BadgeKind::Branch => {
                let mut refs: Vec<String> = self
                    .remotes
                    .iter()
                    .map(|r| format!("refs/remotes/{r}/{}", self.label))
                    .collect();
                if self.local {
                    refs.insert(0, format!("refs/heads/{}", self.label));
                }
                refs
            }
        }
    }

    pub(super) fn describe(&self) -> String {
        match self.kind {
            BadgeKind::Tag => format!("tag {}", self.label),
            BadgeKind::DetachedHead => "HEAD (detached)".to_owned(),
            BadgeKind::PullRequest => format!(
                "{} — pull request branch (not fetched as a branch)",
                self.label
            ),
            BadgeKind::Branch => {
                let mut places = Vec::new();
                if self.local {
                    places.push(if self.head {
                        "local, checked out".to_owned()
                    } else {
                        "local".to_owned()
                    });
                }
                places.extend(self.remotes.iter().cloned());
                format!("{} — {}", self.label, places.join(", "))
            }
        }
    }
}

/// Turns `git log --format=%D` decorations into pills: `x` and `origin/x` at the same commit
/// become one pill marked local and remote.
pub(super) fn badges(decorations: &[String], refs: &Refs) -> Vec<Badge> {
    // The remote whose branch list has it, else the longest remote it is under (`my/fork/main`
    // is `my/fork`'s even when there is a remote `my`), unless it is a local branch's name.
    // `tracking`: it is known to be a remote-tracking branch (a full `refs/remotes/` name),
    // then of its first part when no remote fits (one since removed from the config).
    let remote_of = |d: &str, tracking: bool| {
        let remote = refs
            .remotes
            .iter()
            .find(|(_, list)| list.iter().any(|b| b.name == d))
            .map(|(remote, _)| remote.to_owned())
            .or_else(|| {
                if !tracking && refs.local.iter().any(|b| b.name == d) {
                    return None;
                }
                let names: Vec<&str> = refs.remotes.iter().map(|(r, _)| r.as_str()).collect();
                match sidebar::remote_of(d, &names) {
                    Some(remote) => Some(remote.to_owned()),
                    None if tracking => d.split_once('/').map(|(remote, _)| remote.to_owned()),
                    None => None,
                }
            })?;
        let branch = sidebar::remote_short(d, &remote)?;
        Some((branch.to_owned(), remote))
    };
    fn branch(out: &mut Vec<Badge>, name: &str) -> usize {
        match out
            .iter()
            .position(|b| b.kind == BadgeKind::Branch && b.label == name)
        {
            Some(i) => i,
            None => {
                out.push(Badge {
                    label: name.to_owned(),
                    kind: BadgeKind::Branch,
                    local: false,
                    remotes: Vec::new(),
                    head: false,
                });
                out.len() - 1
            }
        }
    }
    let mut out: Vec<Badge> = Vec::new();
    for d in decorations {
        // What gitcraft keeps of earlier versions (`refs/gitcraft/`) is no branch: an older
        // git, or `log.initialDecorationSet=all`, lists it among the decorations.
        if d.starts_with("refs/gitcraft/") || d.starts_with("tag: refs/gitcraft/") {
            continue;
        }
        // With `log.decorate=full` in the user's config, `%D` gives full ref names.
        if let Some(name) = d.strip_prefix("HEAD -> ") {
            let name = name.strip_prefix("refs/heads/").unwrap_or(name);
            let i = branch(&mut out, name);
            if let Some(b) = out.get_mut(i) {
                b.local = true;
                b.head = true;
            }
        } else if d == "HEAD" {
            out.push(Badge {
                label: "HEAD".to_owned(),
                kind: BadgeKind::DetachedHead,
                local: false,
                remotes: Vec::new(),
                head: true,
            });
        } else if let Some(tag) = d.strip_prefix("tag: ") {
            let tag = tag.strip_prefix("refs/tags/").unwrap_or(tag);
            out.push(Badge {
                label: tag.to_owned(),
                kind: BadgeKind::Tag,
                local: false,
                remotes: Vec::new(),
                head: false,
            });
        } else if let Some(name) = d.strip_prefix("refs/heads/") {
            let i = branch(&mut out, name);
            if let Some(b) = out.get_mut(i) {
                b.local = true;
            }
        } else if let Some((name, remote)) = match d.strip_prefix("refs/remotes/") {
            Some(rest) => remote_of(rest, true),
            None => remote_of(d, false),
        } {
            // `origin/HEAD` only says which branch the remote defaults to.
            if name == "HEAD" {
                continue;
            }
            let i = branch(&mut out, &name);
            if let Some(b) = out.get_mut(i) {
                b.remotes.push(remote);
            }
        } else {
            let i = branch(&mut out, d);
            if let Some(b) = out.get_mut(i) {
                b.local = true;
            }
        }
    }
    for badge in &mut out {
        // A log's decorations needn't include HEAD (for example a filtered graph). The refs
        // read alongside it still identify the checked-out local branch.
        if badge.kind == BadgeKind::Branch && badge.local {
            badge.head = match refs.current.as_deref() {
                Some(current) => current == badge.label,
                None => {
                    badge.head
                        || refs
                            .local
                            .iter()
                            .any(|b| b.name == badge.label && b.is_head)
                }
            };
        }
        badge.remotes.sort();
        badge.remotes.dedup();
    }
    // These pills all point at the same commit, so their commit distance to the default
    // branch ties. Show the checked-out branch first, then the usual default/integration
    // branches, before other local branches, remote-only branches and tags. Name ties are
    // explicit: git's decoration order mustn't decide which branch hides under +N.
    out.sort_by(|a, b| {
        let rank = |badge: &Badge| match badge.kind {
            _ if badge.head => 0,
            BadgeKind::Branch => match badge.label.as_str() {
                "main" => 1,
                "master" => 2,
                "dev" => 3,
                "develop" => 4,
                _ if badge.local => 5,
                _ => 6,
            },
            BadgeKind::Tag => 7,
            BadgeKind::DetachedHead => 8,
            BadgeKind::PullRequest => 9,
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.remotes.cmp(&b.remotes))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;

    fn refs_with_origin() -> Refs {
        Refs {
            remotes: vec![("origin".to_owned(), Vec::new())],
            ..Refs::default()
        }
    }

    #[test]
    fn remote_twin_folds_into_local_badge() {
        let decos = [
            "origin/main",
            "HEAD -> main",
            "origin/HEAD",
            "origin/dev",
            "tag: v1",
        ]
        .map(String::from);
        let b = badges(&decos, &refs_with_origin());
        let labels: Vec<&str> = b.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["main", "dev", "v1"]);
        assert!(b[0].head && b[0].local && b[0].remotes == ["origin"]);
        assert!(!b[1].local && b[1].remotes == ["origin"]);
        assert_eq!(b[2].kind, BadgeKind::Tag);
    }

    /// The refs gitcraft keeps earlier versions in make no pill, however git lists them.
    #[test]
    fn kept_versions_are_no_branches() {
        let decos = [
            "HEAD -> main",
            "refs/gitcraft/rewrites/main/12/new-tip",
            "refs/gitcraft/reviewed/github.com/a/b/3/head",
        ]
        .map(String::from);
        let labels: Vec<String> = badges(&decos, &refs_with_origin())
            .into_iter()
            .map(|b| b.label)
            .collect();
        assert_eq!(labels, ["main"]);
    }

    fn branch(name: &str) -> git::Branch {
        git::Branch {
            name: name.to_owned(),
            target: String::new(),
            is_head: false,
            upstream: String::new(),
            track: String::new(),
        }
    }

    #[test]
    fn a_remote_named_like_the_start_of_another_does_not_take_its_branches() {
        let refs = Refs {
            remotes: vec![
                ("my".to_owned(), vec![branch("my/dev")]),
                ("my/fork".to_owned(), vec![branch("my/fork/main")]),
            ],
            ..Refs::default()
        };
        let b = badges(&["my/fork/main".to_owned()], &refs);
        assert_eq!(b[0].label, "main");
        assert_eq!(b[0].remotes, ["my/fork"]);
        assert_eq!(b[0].git_ref(), "my/fork/main");
        // Not in the lists yet (refs not reloaded): still the longest remote.
        let b = badges(&["my/fork/topic".to_owned()], &refs);
        assert_eq!(
            (b[0].label.as_str(), b[0].remotes.as_slice()),
            ("topic", &["my/fork".to_owned()][..])
        );
    }

    #[test]
    fn a_local_branch_named_under_a_remote_is_local() {
        let refs = Refs {
            local: vec![branch("origin/wip")],
            remotes: vec![("origin".to_owned(), vec![branch("origin/main")])],
            ..Refs::default()
        };
        let b = badges(&["origin/wip".to_owned()], &refs);
        assert_eq!(b[0].label, "origin/wip");
        assert!(b[0].local && b[0].remotes.is_empty());
        assert_eq!(b[0].git_ref(), "origin/wip");
    }

    #[test]
    fn full_ref_names_make_the_same_pills() {
        // `%D` with `log.decorate=full` in the user's config.
        let refs = Refs {
            local: vec![branch("main"), branch("x")],
            remotes: vec![("origin".to_owned(), vec![branch("origin/main")])],
            ..Refs::default()
        };
        let decos = [
            "HEAD -> refs/heads/main",
            "refs/remotes/origin/main",
            "refs/remotes/origin/HEAD",
            "tag: refs/tags/x",
            "refs/heads/x",
            "refs/remotes/gone/topic",
        ]
        .map(String::from);
        let b = badges(&decos, &refs);
        let pills: Vec<(&str, bool, &[String])> = b
            .iter()
            .map(|b| (b.label.as_str(), b.local, b.remotes.as_slice()))
            .collect();
        assert_eq!(
            pills,
            vec![
                ("main", true, &["origin".to_owned()][..]),
                ("x", true, &[][..]),
                ("topic", false, &["gone".to_owned()][..]),
                ("x", false, &[][..]),
            ]
        );
        assert!(b[0].head);
        assert_eq!(b[1].git_ref(), "x");
        assert_eq!(b[2].git_ref(), "gone/topic");
        assert_eq!(b[3].kind, BadgeKind::Tag);
    }

    #[test]
    fn checked_out_branch_wins_even_without_a_head_decoration() {
        let refs = Refs {
            current: Some("topic".to_owned()),
            local: vec![branch("main"), branch("topic")],
            ..refs_with_origin()
        };
        let b = badges(&["main", "origin/main", "topic"].map(String::from), &refs);
        assert_eq!(b[0].label, "topic");
        assert!(b[0].head);
        assert_eq!(b[1].label, "main");
        // Live checkout metadata also takes precedence over decorations read earlier.
        let b = badges(&["HEAD -> main", "topic"].map(String::from), &refs);
        assert_eq!(b[0].label, "topic");
        assert!(!b[1].head);
    }

    #[test]
    fn default_branch_names_win_over_other_branches_at_the_same_commit() {
        for (names, first) in [
            (vec!["topic", "dev", "master", "main"], "main"),
            (vec!["topic", "dev", "master"], "master"),
            (vec!["topic", "dev", "develop"], "dev"),
            (vec!["topic", "develop"], "develop"),
        ] {
            let decos: Vec<String> = names.into_iter().map(str::to_owned).collect();
            assert_eq!(badges(&decos, &refs_with_origin())[0].label, first);
        }
        // A default branch on the remote still represents this commit ahead of a local
        // feature branch; it needn't have been checked out locally.
        let decos = ["topic", "origin/dev"].map(String::from);
        let b = badges(&decos, &refs_with_origin());
        assert_eq!(b[0].label, "dev");
        assert!(!b[0].local);
    }

    #[test]
    fn branch_ties_are_independent_of_git_decoration_order() {
        let refs = Refs {
            remotes: vec![("z".to_owned(), vec![]), ("a".to_owned(), vec![])],
            ..Refs::default()
        };
        let names = ["beta", "a/topic", "alpha", "z/topic", "tag: release"];
        let forward = badges(&names.map(String::from), &refs);
        let backward = badges(
            &names
                .into_iter()
                .rev()
                .map(String::from)
                .collect::<Vec<_>>(),
            &refs,
        );
        let describe =
            |badges: Vec<Badge>| badges.into_iter().map(|b| b.describe()).collect::<Vec<_>>();
        assert_eq!(describe(forward), describe(backward));
    }

    #[test]
    fn local_branch_with_slash_is_not_remote() {
        let decos = ["feature/x".to_owned()];
        let b = badges(&decos, &refs_with_origin());
        assert!(b[0].kind == BadgeKind::Branch && b[0].local && b[0].remotes.is_empty());
    }
}
