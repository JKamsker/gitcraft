//! The pills of the refs column: which branches and tags point at a commit.

use std::collections::{HashMap, HashSet};

use crate::git::Refs;

/// One selectable full ref in the refs column, or a detached HEAD.
#[derive(Debug)]
pub(super) struct Badge {
    /// Raw branch path; never a display caption.
    pub(super) label: String,
    pub(super) caption: String,
    pub(super) full_ref: String,
    pub(super) kind: BadgeKind,
    /// The branch exists locally.
    pub(super) local: bool,
    /// The owning remote for a remote-tracking ref (empty for a local ref).
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
    /// The immutable full Git ref; captions never participate in action routing.
    pub(super) fn git_ref(&self) -> String {
        self.full_ref.clone()
    }

    /// The pill as a screen reader says it, in a row's name: "branch dev, checked out".
    pub(super) fn spoken(&self) -> String {
        match self.kind {
            BadgeKind::Tag => crate::i18n::message(
                "repo-core-2591",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&self.label).into(),
                )],
            ),
            BadgeKind::DetachedHead => crate::i18n::text("repo-core-2592").to_owned(),
            BadgeKind::PullRequest => crate::i18n::message(
                "repo-core-2593",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&self.label).into(),
                )],
            ),
            BadgeKind::Branch if self.head => crate::i18n::message(
                "repo-core-2594",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&self.label).into(),
                )],
            ),
            BadgeKind::Branch if !self.local => crate::i18n::message(
                "repo-core-2595",
                &[("git-ref", self.git_ref().to_string().into())],
            ),
            BadgeKind::Branch => crate::i18n::message(
                "repo-core-2596",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&self.label).into(),
                )],
            ),
        }
    }

    /// The full names of the refs the pill stands for.
    pub(super) fn full_refs(&self) -> Vec<String> {
        if self.kind == BadgeKind::PullRequest {
            Vec::new()
        } else {
            vec![self.full_ref.clone()]
        }
    }

    pub(super) fn describe(&self) -> String {
        match self.kind {
            BadgeKind::Tag => crate::i18n::message(
                "repo-core-2591",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&self.label).into(),
                )],
            ),
            BadgeKind::DetachedHead => crate::i18n::text("repo-core-2598").to_owned(),
            BadgeKind::PullRequest => crate::i18n::message(
                "repo-core-2599",
                &[(
                    "label",
                    crate::repo::messages::display_operation(&self.label).into(),
                )],
            ),
            BadgeKind::Branch => {
                let mut places = Vec::new();
                if self.local {
                    places.push(if self.head {
                        crate::i18n::text("repo-core-2600").to_owned()
                    } else {
                        crate::i18n::text("repo-core-2601").to_owned()
                    });
                }
                places.extend(self.remotes.iter().cloned());
                format!("{} — {}", self.label, places.join(", "))
            }
        }
    }
}

/// Turns `git log --format=%D` decorations into pills: `x` and `origin/x` at the same commit
/// remain separate selectable pills.
#[cfg(test)]
pub(super) fn badges(decorations: &[String], refs: &Refs) -> Vec<Badge> {
    if decorations.is_empty() {
        return Vec::new();
    }
    BadgeIndex::new(refs).badges(decorations)
}

/// Reuse the repository-wide lookup tables across all decorated commits in a rebuild.
#[derive(Default)]
pub(in crate::repo) struct BadgeIndex {
    local: HashSet<String>,
    heads: HashSet<String>,
    current: Option<String>,
    names: Vec<String>,
    remote_branches: HashMap<String, String>,
    pub(in crate::repo) captions: std::collections::BTreeMap<String, crate::ref_labels::Label>,
}

impl BadgeIndex {
    pub(in crate::repo) fn new(refs: &Refs) -> Self {
        let local = refs.local.iter().map(|b| b.name.clone()).collect();
        let names = refs.remotes.iter().map(|(r, _)| r.clone()).collect();
        let mut remote_branches = HashMap::new();
        for (remote, branches) in &refs.remotes {
            for branch in branches {
                remote_branches
                    .entry(branch.name.clone())
                    .or_insert_with(|| remote.clone());
            }
        }
        let heads = refs
            .local
            .iter()
            .filter(|b| b.is_head)
            .map(|b| b.name.clone())
            .collect();
        Self {
            heads,
            current: refs.current.clone(),
            local,
            names,
            remote_branches,
            captions: Default::default(),
        }
    }

    pub(super) fn badges(&self, decorations: &[String]) -> Vec<Badge> {
        let Self {
            heads,
            current,
            local,
            names,
            remote_branches,
            ..
        } = self;
        // The remote whose branch list has it, else the longest remote it is under (`my/fork/main`
        // is `my/fork`'s even when there is a remote `my`), unless it is a local branch's name.
        // `tracking`: it is known to be a remote-tracking branch (a full `refs/remotes/` name),
        // then of its first part when no remote fits (one since removed from the config).
        let remote_of = |d: &str, tracking: bool| {
            if !tracking && local.contains(d) {
                return None;
            }
            let remote = remote_branches
                .get(d)
                .map(|remote| (*remote).to_owned())
                .or_else(|| {
                    if !tracking && local.contains(d) {
                        return None;
                    }
                    match remote_of(d, &names.iter().map(String::as_str).collect::<Vec<_>>()) {
                        Some(remote) => Some(remote.to_owned()),
                        None if tracking => d.split_once('/').map(|(remote, _)| remote.to_owned()),
                        None => None,
                    }
                })?;
            let branch = d.strip_prefix(&remote)?.strip_prefix('/')?;
            Some((branch.to_owned(), remote))
        };
        fn branch(
            out: &mut Vec<Badge>,
            indices: &mut HashMap<String, usize>,
            name: &str,
            remote: Option<&str>,
        ) -> usize {
            let full = match remote {
                Some(r) => format!("refs/remotes/{r}/{name}"),
                None => format!("refs/heads/{name}"),
            };
            if let Some(&i) = indices.get(&full) {
                return i;
            }
            let i = out.len();
            out.push(Badge {
                label: name.to_owned(),
                caption: name.to_owned(),
                full_ref: full.clone(),
                kind: BadgeKind::Branch,
                local: false,
                remotes: Vec::new(),
                head: false,
            });
            indices.insert(full, i);
            i
        }
        let mut indices = HashMap::new();
        let mut out: Vec<Badge> = Vec::new();
        for d in decorations {
            // What gitcraft keeps of earlier versions (`refs/gitcraft/`) is no branch: an older
            // git, or `log.initialDecorationSet=all`, lists it among the decorations.
            if d.starts_with("refs/gitcraft/") || d.starts_with("tag: refs/gitcraft/") {
                continue;
            }
            // With `log.decorate=full` in the user's config, `%D` gives full ref names.
            if let Some(name) = d.strip_prefix("HEAD -> ") {
                let name = name
                    .strip_prefix("refs/heads/")
                    .or_else(|| name.strip_prefix("heads/"))
                    .unwrap_or(name);
                let i = branch(&mut out, &mut indices, name, None);
                if let Some(b) = out.get_mut(i) {
                    b.local = true;
                    b.head = true;
                }
            } else if d == "HEAD" {
                out.push(Badge {
                    label: "HEAD".to_owned(),
                    caption: "HEAD".to_owned(),
                    full_ref: "HEAD".into(),
                    kind: BadgeKind::DetachedHead,
                    local: false,
                    remotes: Vec::new(),
                    head: true,
                });
            } else if let Some(tag) = d.strip_prefix("tag: ") {
                let tag = tag
                    .strip_prefix("refs/tags/")
                    .or_else(|| tag.strip_prefix("tags/"))
                    .unwrap_or(tag);
                out.push(Badge {
                    label: tag.to_owned(),
                    caption: tag.to_owned(),
                    full_ref: format!("refs/tags/{tag}"),
                    kind: BadgeKind::Tag,
                    local: false,
                    remotes: Vec::new(),
                    head: false,
                });
            } else if let Some(name) = d
                .strip_prefix("refs/heads/")
                .or_else(|| d.strip_prefix("heads/"))
            {
                let i = branch(&mut out, &mut indices, name, None);
                if let Some(b) = out.get_mut(i) {
                    b.local = true;
                }
            } else if let Some((name, remote)) = match d
                .strip_prefix("refs/remotes/")
                .or_else(|| d.strip_prefix("remotes/"))
            {
                Some(rest) => remote_of(rest, true),
                None => remote_of(d, false),
            } {
                // `origin/HEAD` only says which branch the remote defaults to.
                if name == "HEAD" && !remote_branches.contains_key(&format!("{remote}/{name}")) {
                    continue;
                }
                let i = branch(&mut out, &mut indices, &name, Some(&remote));
                if let Some(b) = out.get_mut(i) {
                    b.remotes.push(remote);
                }
            } else {
                let i = branch(&mut out, &mut indices, d, None);
                if let Some(b) = out.get_mut(i) {
                    b.local = true;
                }
            }
        }
        for badge in &mut out {
            // A log's decorations needn't include HEAD (for example a filtered graph). The refs
            // read alongside it still identify the checked-out local branch.
            if badge.kind == BadgeKind::Branch && badge.local {
                badge.head = match current.as_deref() {
                    Some(current) => current == badge.label,
                    None => badge.head || heads.contains(badge.label.as_str()),
                };
            }
            badge.remotes.sort();
            badge.remotes.dedup();
        }
        for badge in &mut out {
            if let Some(full) = badge.full_refs().first()
                && let Some(label) = self.captions.get(full)
            {
                badge.caption = label.caption.clone();
            }
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
}

#[cfg(test)]
#[path = "badge_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "badge_index_tests.rs"]
mod index_tests;

// Resolve nested aliases without depending on sidebar presentation.
fn remote_of<'a>(name: &str, names: &[&'a str]) -> Option<&'a str> {
    names
        .iter()
        .copied()
        .filter(|r| name.strip_prefix(r).is_some_and(|s| s.starts_with('/')))
        .max_by_key(|r| r.len())
}
