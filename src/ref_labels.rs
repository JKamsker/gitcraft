//! Deterministic, collision-safe captions. Git ref names remain the only identity.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Local,
    Remote,
    Tag,
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub full_ref: String,
    pub kind: Kind,
    pub path: String,
    pub remote: Option<String>,
    pub owner: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Label {
    pub caption: String,
    pub qualifier: Option<String>,
}

/// Resolve the complete surface catalog, before filtering or truncation. Reserve every
/// full-ref fallback up front, so literal branch names cannot steal another ref's identity.
pub(crate) fn labels(entries: &[Entry], preferred: Option<&str>) -> BTreeMap<String, Label> {
    let remotes: BTreeSet<_> = entries.iter().filter_map(|e| e.remote.as_deref()).collect();
    let preferred = preferred.or_else(|| {
        if remotes.contains("origin") {
            Some("origin")
        } else {
            remotes.first().copied()
        }
    });
    let non_remote: BTreeSet<_> = entries
        .iter()
        .filter(|e| e.kind != Kind::Remote)
        .map(|e| e.path.as_str())
        .collect();
    let mut paths = BTreeMap::<&str, usize>::new();
    let mut owners = BTreeMap::<&str, BTreeSet<&str>>::new();
    for e in entries {
        *paths.entry(&e.path).or_default() += 1;
        if let (Some(owner), Some(remote)) = (e.owner.as_deref(), e.remote.as_deref()) {
            owners.entry(owner).or_default().insert(remote);
        }
    }
    let mut ordered: Vec<_> = entries.iter().collect();
    ordered.sort_by_key(|e| {
        (
            match e.kind {
                Kind::Local => 0,
                Kind::Tag => 1,
                Kind::Remote => 2,
            },
            &e.full_ref,
        )
    });
    let reserved: BTreeSet<_> = entries.iter().map(|e| e.full_ref.as_str()).collect();
    let mut used = BTreeSet::new();
    let mut out = BTreeMap::new();
    // Claim literal captions first, so generated owner/alias prefixes cannot steal them.
    for e in &ordered {
        let bare = e.kind != Kind::Remote
            || paths.get(e.path.as_str()) == Some(&1)
            || (e.remote.as_deref() == preferred && !non_remote.contains(e.path.as_str()));
        if bare && !reserved.contains(e.path.as_str()) && used.insert(e.path.clone()) {
            out.insert(
                e.full_ref.clone(),
                Label {
                    caption: e.path.clone(),
                    qualifier: e.remote.clone(),
                },
            );
        }
    }
    for e in ordered {
        if out.contains_key(&e.full_ref) {
            continue;
        }
        let owner = e
            .owner
            .as_deref()
            .filter(|owner| owners.get(owner).is_some_and(|rs| rs.len() == 1));
        let candidates = owner
            .map(|o| format!("{o}/{}", e.path))
            .into_iter()
            .chain(e.remote.as_ref().map(|r| format!("{r}/{}", e.path)))
            .chain(std::iter::once(e.full_ref.clone()));
        for caption in candidates {
            if (caption == e.full_ref || !reserved.contains(caption.as_str()))
                && used.insert(caption.clone())
            {
                out.insert(
                    e.full_ref.clone(),
                    Label {
                        caption,
                        qualifier: e.remote.clone(),
                    },
                );
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
