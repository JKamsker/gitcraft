//! Adapts the repository catalog to shared, display-only ref captions.
use std::collections::BTreeMap;

use super::RepoTab;
use crate::ref_labels::{Entry, Kind, Label};

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Preference {
    pub name: String,
    pub url: String,
}

impl Preference {
    pub fn reconcile(&mut self, remotes: &[crate::git::Remote]) {
        if self.name.is_empty()
            || remotes
                .iter()
                .any(|r| r.name == self.name && r.url == self.url)
        {
            return;
        }
        let matches: Vec<_> = remotes.iter().filter(|r| r.url == self.url).collect();
        *self = match matches.as_slice() {
            [remote] => Self {
                name: remote.name.clone(),
                url: remote.url.clone(),
            },
            _ => Self::default(),
        };
    }
}

impl RepoTab {
    pub(super) fn sync_ref_labels(&mut self, ctx: &egui::Context) {
        let preference =
            super::branch_visibility::Visibility::load(ctx, self.repo_key()).label_remote;
        if preference != self.visibility.label_remote {
            self.visibility.label_remote = preference;
            self.rebuild_graph_labels();
        }
    }

    pub(super) fn ref_labels(&self) -> BTreeMap<String, Label> {
        let mut entries: Vec<_> = self
            .refs
            .local
            .iter()
            .map(|b| Entry {
                full_ref: format!("refs/heads/{}", b.name),
                kind: Kind::Local,
                path: b.name.clone(),
                remote: None,
                owner: None,
            })
            .chain(self.refs.tags.iter().map(|b| Entry {
                full_ref: format!("refs/tags/{}", b.name),
                kind: Kind::Tag,
                path: b.name.clone(),
                remote: None,
                owner: None,
            }))
            .collect();
        for (remote, branches) in &self.refs.remotes {
            let owner = self
                .remotes
                .iter()
                .find(|r| r.name == *remote)
                .and_then(|r| crate::forge::location(&r.url))
                .and_then(|(_, path)| path.rsplit_once('/').map(|(owner, _)| owner.to_owned()));
            for b in branches {
                entries.push(Entry {
                    full_ref: format!("refs/remotes/{}", b.name),
                    kind: Kind::Remote,
                    path: b
                        .name
                        .strip_prefix(remote)
                        .and_then(|s| s.strip_prefix('/'))
                        .unwrap_or(&b.name)
                        .to_owned(),
                    remote: Some(remote.clone()),
                    owner: owner.clone(),
                });
            }
        }
        crate::ref_labels::labels(
            &entries,
            self.remotes
                .iter()
                .find(|r| r.name == self.visibility.label_remote.name)
                .or_else(|| self.remotes.iter().find(|r| r.name == "origin"))
                .or_else(|| self.remotes.iter().min_by_key(|r| &r.name))
                .map(|r| r.name.as_str()),
        )
    }

    pub(super) fn ref_description(&self, full: &str) -> String {
        let remote = self.refs.remotes.iter().find(|(_, bs)| {
            bs.iter()
                .any(|b| format!("refs/remotes/{}", b.name) == full)
        });
        match remote.and_then(|(name, _)| self.remotes.iter().find(|r| r.name == *name)) {
            Some(r) => format!("{}\n{full}\n{}", r.name, r.url),
            None => full.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn remote(name: &str, url: &str) -> crate::git::Remote {
        crate::git::Remote {
            name: name.into(),
            url: url.into(),
        }
    }
    #[test]
    fn preference_follows_only_unambiguous_url_renames() {
        let original = Preference {
            name: "old".into(),
            url: "url".into(),
        };
        let mut preference = original.clone();
        preference.reconcile(&[remote("renamed", "url")]);
        assert_eq!(preference.name, "renamed");
        preference = original.clone();
        preference.reconcile(&[remote("a", "url"), remote("b", "url")]);
        assert_eq!(preference, Preference::default());
        preference = original;
        preference.reconcile(&[]);
        assert_eq!(preference, Preference::default());
    }
}

#[cfg(test)]
#[path = "ref_presentation_tests.rs"]
mod surface_tests;
