//! Which remote a link goes to: the one the branch tracks, else `origin`, else the person
//! picks among those on a forge.

use crate::forge::{self, Forge, Hosting};
use crate::git::Remote;

/// Whether a link can go to `remote`: its URL names a host (a folder on disk, or `C:/…`, has
/// no pages).
pub fn linkable(remote: &Remote) -> bool {
    forge::location(&remote.url).is_some_and(|(host, _)| host.len() > 1)
}

/// The remotes a link may go to, by name: just the one `tracked` names when it's linkable
/// (the remote of the branch's upstream), else `origin`, else every linkable remote, to pick
/// from when there are several. Empty when none is on a forge.
pub fn candidates(remotes: &[Remote], tracked: Option<&str>) -> Vec<String> {
    let usable: Vec<&Remote> = remotes.iter().filter(|r| linkable(r)).collect();
    let named = |name: &str| {
        usable
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.name.clone())
    };
    if let Some(one) = tracked.and_then(named).or_else(|| named("origin")) {
        return vec![one];
    }
    usable.iter().map(|r| r.name.clone()).collect()
}

/// The remote and the branch on it an upstream (`origin/main`, `my/fork/topic`) names: the
/// longest remote name that fits, as a remote's name can have a `/` in it.
pub fn upstream_remote(upstream: &str, remotes: &[Remote]) -> Option<(String, String)> {
    remotes
        .iter()
        .filter_map(|r| {
            let branch = upstream.strip_prefix(r.name.as_str())?.strip_prefix('/')?;
            (!branch.is_empty()).then(|| (r.name.clone(), branch.to_owned()))
        })
        .max_by_key(|(name, _)| name.len())
}

/// The name of the forge `remote` is on, for its menu entries: as `origin`'s detection
/// (`hosting`) found it, else as the remote's host says; "the forge" when neither tells.
pub fn forge_name(remote: &Remote, hosting: Option<&Hosting>) -> &'static str {
    forge_of(remote, hosting).map_or(crate::i18n::text("backend-the-forge-6550d0"), Forge::name)
}

fn forge_of(remote: &Remote, hosting: Option<&Hosting>) -> Option<Forge> {
    let (host, _) = forge::location(&remote.url)?;
    if let Some(h) = hosting.filter(|h| h.matches_git_remote(&remote.url)) {
        return Some(h.forge);
    }
    Forge::ALL
        .into_iter()
        .find(|forge| forge.provider().claims(&host))
}
