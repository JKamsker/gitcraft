//! The branches and tags a commit is in, and the first tag after it.

use std::path::Path;

use super::{END_OPTS, run};

/// How many names of each kind are kept; the rest are only counted.
pub const CONTAINED_CAP: usize = 50;

/// Names of one kind of ref (local branches, remote branches, tags) that contain a commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefList {
    /// The first [`CONTAINED_CAP`] of them, in git's order (by name).
    pub names: Vec<String>,
    /// How many there are in all.
    pub total: usize,
}

impl RefList {
    /// How many are left out of `names`.
    pub fn hidden(&self) -> usize {
        self.total.saturating_sub(self.names.len())
    }

    fn push(&mut self, name: &str) {
        self.total += 1;
        if self.names.len() < CONTAINED_CAP {
            self.names.push(name.to_owned());
        }
    }
}

/// Where a commit is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Containment {
    pub local: RefList,
    /// Remote branches, as `origin/main`.
    pub remote: RefList,
    pub tags: RefList,
    /// The first tag that contains it, as `describe --contains` names it (`v1.2~3`): `None`
    /// when no tag does.
    pub describe: Option<String>,
}

impl Containment {
    /// The tag of [`Self::describe`] alone (`v1.2` of `v1.2~3^2`).
    pub fn first_tag(&self) -> Option<&str> {
        self.describe.as_deref().map(tag_of_describe)
    }
}

/// The branches and tags that contain `hash`, and the first tag after it.
pub fn containment(repo: &Path, hash: &str) -> Result<Containment, String> {
    let out = run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("--contains={hash}"),
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-list-the-branches-and-tags-containing-it-e-20d486",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let mut found = sort_refs(&out);
    found.describe = describe_contains(repo, hash)?;
    Ok(found)
}

/// The full ref names `out` lists, one a line, sorted by kind. A remote's `HEAD` (which
/// points to one of its branches) isn't a branch of its own.
pub(super) fn sort_refs(out: &str) -> Containment {
    let mut found = Containment::default();
    for name in out.lines().filter(|l| !l.is_empty()) {
        if let Some(branch) = name.strip_prefix("refs/heads/") {
            found.local.push(branch);
        } else if let Some(branch) = name.strip_prefix("refs/remotes/") {
            if !branch.ends_with("/HEAD") {
                found.remote.push(branch);
            }
        } else if let Some(tag) = name.strip_prefix("refs/tags/") {
            found.tags.push(tag);
        }
    }
    found
}

/// `git describe --contains`: the first tag after `hash`, or `None` when no tag has it.
fn describe_contains(repo: &Path, hash: &str) -> Result<Option<String>, String> {
    match run(Some(repo), &["describe", "--contains", END_OPTS, hash]) {
        Ok(out) => Ok(Some(out.trim().to_owned()).filter(|s| !s.is_empty())),
        // No tag contains it (or there are no tags at all).
        Err(e) if e.contains("cannot describe") || e.contains("No tags can describe") => Ok(None),
        Err(e) => Err(crate::i18n::message(
            "backend-could-not-find-the-first-tag-after-it-e-61ec1b",
            &[("detail", (e).to_string().into())],
        )),
    }
}

/// The tag a `describe --contains` name starts from: `v1.2` of `v1.2~3^2~1`.
pub(super) fn tag_of_describe(name: &str) -> &str {
    let end = name.find(['~', '^']).unwrap_or(name.len());
    &name[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_are_sorted_by_kind_and_a_remote_head_left_out() {
        let found = sort_refs(
            "refs/heads/main\nrefs/heads/feat/ünï code\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/main\nrefs/tags/v1.0\n\n",
        );
        assert_eq!(found.local.names, ["main", "feat/ünï code"]);
        assert_eq!(found.remote.names, ["origin/main"]);
        assert_eq!(found.tags.names, ["v1.0"]);
        assert_eq!(found.local.total, 2);
        assert_eq!(sort_refs(""), Containment::default());
    }

    #[test]
    fn long_lists_are_capped_and_counted() {
        let many: String = (0..CONTAINED_CAP + 7)
            .map(|n| format!("refs/tags/t{n}\n"))
            .collect();
        let found = sort_refs(&many);
        assert_eq!(found.tags.names.len(), CONTAINED_CAP);
        assert_eq!(found.tags.total, CONTAINED_CAP + 7);
        assert_eq!(found.tags.hidden(), 7);
        assert_eq!(found.local.hidden(), 0);
    }

    #[test]
    fn the_tag_of_a_describe_name() {
        assert_eq!(tag_of_describe("v1.2~3^2~1"), "v1.2");
        assert_eq!(tag_of_describe("v1.2^0"), "v1.2");
        assert_eq!(tag_of_describe("release/2024"), "release/2024");
        assert_eq!(tag_of_describe(""), "");
    }
}
