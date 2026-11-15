//! Fetch refspecs establish ownership, independent of destination namespaces.
use super::{Choice, Path};
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

fn source(spec: &str, full: &str) -> Option<String> {
    let (from, to) = spec.trim_start_matches('+').split_once(':')?;
    if let Some((prefix, suffix)) = to.split_once('*') {
        Some(from.replace('*', full.strip_prefix(prefix)?.strip_suffix(suffix)?))
    } else {
        (to == full).then(|| from.to_owned())
    }
}
pub(super) fn choices(repo: &Path, refs: &crate::git::Refs) -> Result<(Vec<Choice>, bool), String> {
    let fetched: BTreeSet<_> = refs
        .remotes
        .iter()
        .flat_map(|(_, branches)| branches.iter().map(|b| format!("refs/remotes/{}", b.name)))
        .collect();
    let mut owners = BTreeMap::new();
    for remote in crate::git::remotes(repo)? {
        let specs =
            super::super::remote_config::values(repo, &format!("remote.{}.fetch", remote.name))?;
        for full in &fetched {
            if let Some(source) = specs.iter().find_map(|spec| source(spec, full)) {
                match owners.entry(full.clone()) {
                    Entry::Vacant(entry) => {
                        entry.insert(Some(Choice {
                            full: full.clone(),
                            path: format!(
                                "{}/{}",
                                remote.name,
                                source.strip_prefix("refs/heads/").unwrap_or(&source)
                            ),
                            remote: remote.name.clone(),
                            url: remote.url.clone(),
                            source,
                        }));
                    }
                    Entry::Occupied(mut entry) => {
                        // Git refuses --set-upstream-to if several remotes claim the ref.
                        // No remote owns that choice, regardless of iteration order.
                        entry.insert(None);
                    }
                }
            }
        }
    }
    let ambiguous = owners.values().any(Option::is_none);
    let mut choices: Vec<Choice> = owners.into_values().flatten().collect();
    choices.sort_by(|a, b| (&a.remote, &a.path, &a.full).cmp(&(&b.remote, &b.path, &b.full)));
    Ok((choices, ambiguous))
}
