//! Shared project, branch and routing-plan fixtures.

use super::*;
use crate::forge::Forge;
use crate::git::Branch;

pub(super) fn project(path: &str) -> Project {
    Project {
        forge: Forge::GitHub,
        host: "github.com".to_owned(),
        path: path.to_owned(),
    }
}

pub(super) fn remote(name: &str, url: &str) -> Remote {
    Remote {
        name: name.to_owned(),
        url: url.to_owned(),
    }
}

pub(super) fn branch(name: &str, target: &str, upstream: &str) -> Branch {
    Branch {
        name: name.to_owned(),
        target: target.to_owned(),
        is_head: false,
        upstream: upstream.to_owned(),
        track: String::new(),
    }
}

pub(super) fn fork_and_upstream() -> Vec<Remote> {
    vec![
        remote("origin", "https://github.com/up/repo.git"),
        remote("fork", "git@github.com:me/repo.git"),
    ]
}

pub(super) fn plan_of(remotes: &[Remote], refs: &Refs, prs: Vec<&str>, recent: Vec<&str>) -> Plan {
    let main = project("up/repo");
    plan(
        &Sources {
            main: &main,
            remotes,
            refs,
            prs,
            recent,
        },
        120,
    )
}

pub(super) fn shas_of<'a>(plan: &'a Plan, path: &str) -> Vec<&'a str> {
    plan.iter()
        .find(|(p, _)| p.path == path)
        .map(|(_, shas)| shas.iter().map(String::as_str).collect())
        .unwrap_or_default()
}
