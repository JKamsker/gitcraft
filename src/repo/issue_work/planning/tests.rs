//! Pure planning fixtures: no RepoTab, Git commands, filesystem reads or background work.
use super::*;
use crate::git::{Branch, Refs, Worktree};
use std::collections::HashMap;

struct Evidence {
    refs: Refs,
    links: HashMap<String, super::super::IssueLink>,
    heads: HashMap<String, String>,
    worktrees: Vec<Worktree>,
    primary: Option<String>,
    eligible: Vec<String>,
    dirty: bool,
}

impl Default for Evidence {
    fn default() -> Self {
        Self {
            refs: Refs {
                head: "abcdef123456789".into(),
                current: Some("main".into()),
                ..Default::default()
            },
            links: HashMap::new(),
            heads: HashMap::new(),
            worktrees: Vec::new(),
            primary: Some("origin".into()),
            eligible: vec!["origin".into()],
            dirty: false,
        }
    }
}

impl Evidence {
    fn planner(&self) -> Planner<'_> {
        Planner::new(Inputs {
            refs: &self.refs,
            links: &self.links,
            remote_heads: &self.heads,
            worktrees: &self.worktrees,
            primary: self.primary.clone(),
            primary_branches: self
                .refs
                .remotes
                .iter()
                .filter(|(r, _)| Some(r) == self.primary.as_ref())
                .flat_map(|(_, branches)| branches.iter().map(|b| b.name.clone()))
                .collect(),
            eligible: self.eligible.iter().map(String::as_str).collect(),
            dirty: self.dirty,
        })
    }

    fn remote(&mut self, remote: &str, branches: &[&str]) {
        self.refs.remotes.push((
            remote.into(),
            branches
                .iter()
                .map(|b| branch(&format!("{remote}/{b}")))
                .collect(),
        ));
    }
}

fn branch(name: &str) -> Branch {
    Branch {
        name: name.into(),
        target: String::new(),
        is_head: false,
        upstream: String::new(),
        track: String::new(),
    }
}

#[allow(clippy::panic)] // Deliberate guard against unexpected allocation.
fn no_folder() -> PathBuf {
    panic!("planning must not allocate a folder on this path")
}

fn here(e: &Evidence, key: impl ToString, branch: &str) -> Plan {
    e.planner().plan(key, branch, false, no_folder)
}

#[test]
fn validation_precedes_allocation_and_returns_no_effects() -> Result<(), String> {
    let mut e = Evidence::default();
    for name in ["-bad", "main"] {
        let p = e.planner().plan(7, name, true, no_folder);
        assert!(p.start.is_err());
        assert!(p.open.is_none() && p.note.is_empty() && p.warnings.is_empty());
    }
    e.refs.head.clear();
    assert!(e.planner().plan(7, "7-fix", true, no_folder).start.is_err());
    Ok(())
}

#[test]
fn known_default_selects_fetch_and_fully_qualified_start() -> Result<(), String> {
    let mut e = Evidence::default();
    e.heads.insert("origin".into(), "dev".into());
    e.remote("origin", &["main", "dev"]);
    e.refs.local.push(branch("dev"));
    assert_eq!(
        e.planner().base(),
        Base {
            name: "dev".into(),
            remote: Some("origin".into()),
            known: true,
        }
    );
    let s = here(&e, 7, "7-fix").start?;
    assert_eq!(
        s.source,
        Source::New {
            from: "refs/remotes/origin/dev".into()
        }
    );
    assert_eq!(s.fetch, Some(("origin".into(), "dev".into())));
    Ok(())
}

#[test]
fn invalid_remote_heads_use_ordered_guesses() -> Result<(), String> {
    for head in ["", "HEAD"] {
        let mut e = Evidence::default();
        e.heads.insert("origin".into(), head.into());
        e.remote("origin", &["develop", "master"]);
        assert_eq!(e.planner().base().name, "master");
        assert!(!e.planner().base().known);
        let s = here(&e, 7, "7-fix").start?;
        assert_eq!(s.fetch, Some(("origin".into(), "master".into())));
    }
    Ok(())
}

#[test]
fn missing_tracking_falls_back_to_local_base_then_exact_head() -> Result<(), String> {
    let mut e = Evidence::default();
    e.refs.local.push(branch("main"));
    let s = here(&e, 7, "7-fix").start?;
    assert_eq!(
        s.source,
        Source::New {
            from: "refs/heads/main".into()
        }
    );
    assert!(s.fetch.is_none());
    e.refs.local.clear();
    let s = here(&e, 7, "7-fix").start?;
    assert_eq!(
        s.source,
        Source::New {
            from: e.refs.head.clone()
        }
    );
    assert!(s.fetch.is_none());
    Ok(())
}

#[test]
fn selected_forge_eligibility_and_primary_preference_are_independent() -> Result<(), String> {
    let mut e = Evidence::default();
    e.remote("other", &["7-fix"]);
    e.remote("mirror", &["7-fix"]);
    e.remote("origin", &["7-fix"]);
    e.eligible.push("mirror".into());
    assert_eq!(
        here(&e, 7, "7-fix").start?.source,
        Source::Track {
            from: "refs/remotes/origin/7-fix".into()
        }
    );
    e.primary = Some("missing".into());
    assert_eq!(
        here(&e, 7, "7-fix").start?.source,
        Source::Track {
            from: "refs/remotes/mirror/7-fix".into()
        }
    );
    e.eligible.clear();
    assert!(matches!(
        here(&e, 7, "7-fix").start?.source,
        Source::New { .. }
    ));
    Ok(())
}

#[test]
fn local_branch_wins_over_remote_and_carries_no_fetch() -> Result<(), String> {
    let mut e = Evidence::default();
    e.refs.local.push(branch("7-fix"));
    e.remote("origin", &["7-fix", "main"]);
    let s = here(&e, 7, "7-fix").start?;
    assert_eq!(s.source, Source::Exists);
    assert!(s.fetch.is_none());
    Ok(())
}

#[test]
fn warning_inputs_distinguish_checkout_current_and_new_worktree() -> Result<(), String> {
    let mut e = Evidence {
        dirty: true,
        ..Default::default()
    };
    e.refs.local.push(branch("7-fix"));
    e.links.insert(
        "7-fix".into(),
        super::super::IssueLink {
            number: 8,
            ..Default::default()
        },
    );
    assert_eq!(here(&e, 7, "7-fix").warnings.len(), 2);
    assert_eq!(here(&e, 8, "7-fix").warnings.len(), 1);
    assert_eq!(here(&e, "#8", "7-fix").warnings.len(), 1);
    assert_eq!(here(&e, "APP-8", "7-fix").warnings.len(), 2);
    e.refs.current = Some("7-fix".into());
    assert_eq!(here(&e, 7, "7-fix").warnings.len(), 1);
    let p = e
        .planner()
        .plan(7, "7-fix", true, || PathBuf::from("new-tree"));
    assert_eq!(p.warnings.len(), 1);
    assert_eq!(p.open, Some(PathBuf::from("new-tree")));
    assert_eq!(p.start?.place, Place::NewWorktree("new-tree".into()));
    Ok(())
}

#[test]
fn existing_worktrees_override_requested_placement_and_skip_prunable_entries() -> Result<(), String>
{
    let mut e = Evidence {
        worktrees: vec![
            Worktree {
                branch: Some("7-fix".into()),
                prunable: Some("gone".into()),
                path: "gone".into(),
                ..Default::default()
            },
            Worktree {
                branch: Some("7-fix".into()),
                path: "existing".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    for request in [false, true] {
        let p = e.planner().plan(7, "7-fix", request, no_folder);
        assert_eq!(p.start?.place, Place::Worktree);
        assert_eq!(p.open, Some(PathBuf::from("existing")));
    }
    e.worktrees[1].current = true;
    let p = e.planner().plan(7, "7-fix", true, no_folder);
    assert_eq!(p.start?.place, Place::Here);
    assert!(p.open.is_none());
    e.worktrees[1].prunable = Some("gone too".into());
    let p = e
        .planner()
        .plan(7, "7-fix", true, || PathBuf::from("allocated"));
    assert_eq!(p.start?.place, Place::NewWorktree("allocated".into()));
    Ok(())
}

#[test]
fn no_primary_uses_local_main_without_fetch_or_remote_default_guess() -> Result<(), String> {
    let mut e = Evidence {
        primary: None,
        ..Default::default()
    };
    e.remote("origin", &["develop"]);
    e.refs.local.push(branch("main"));
    let base = e.planner().base();
    assert_eq!(
        base,
        Base {
            name: "main".into(),
            remote: None,
            known: false
        }
    );
    assert_eq!(
        here(&e, 7, "7-fix").start?.source,
        Source::New {
            from: "refs/heads/main".into()
        }
    );
    Ok(())
}
