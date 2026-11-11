//! Hotfix recovery commands and current repository snapshot.

use super::*;

/// Sets `branch`'s upstream back as it was, as config (the remote's branch may be gone).
pub(super) fn upstream_back(branch: &At) -> Vec<Vec<String>> {
    let Some((remote, merge)) = &branch.upstream else {
        return Vec::new();
    };
    vec![
        strings(&["config", &format!("branch.{}.remote", branch.name), remote]),
        strings(&["config", &format!("branch.{}.merge", branch.name), merge]),
    ]
}

/// Steps that put back what a finish that stopped part-way did, from how things are now
/// (`now`: where each branch merged into is, whether a merge is under way, the tag's object,
/// whether the hotfix branch is still there); `None` when it changed nothing.
pub(crate) fn recover(f: &Finish, c: &Choices, now: &Now) -> Option<Vec<Vec<String>>> {
    let h = &f.hotfix;
    let hotfix_ref = format!("refs/heads/{}", h.name);
    let mut steps = Vec::new();
    let mut changed = false;
    if now.merging {
        steps.push(strings(&["merge", "--abort"]));
        changed = true;
    }
    if now.hotfix.is_none() {
        steps.push(expect_ref_at(&hotfix_ref, ""));
        steps.push(strings(&["branch", &h.name, &h.head]));
        steps.extend(upstream_back(h));
        changed = true;
    }
    if now.head != hotfix_ref {
        steps.push(expect_symref("HEAD", &now.head));
        steps.push(strings(&["checkout", &h.name, "--"]));
        changed = true;
    }
    for t in f.targets(c) {
        let Some((_, at)) = now.branches.iter().find(|(n, _)| *n == t.name) else {
            continue;
        };
        if *at != t.head {
            steps.push(strings(&[
                "update-ref",
                &format!("refs/heads/{}", t.name),
                &t.head,
                at,
            ]));
            changed = true;
        }
    }
    if let Some(tag) = &now.tag {
        steps.push(strings(&[
            "update-ref",
            "-d",
            &format!("refs/tags/{}", c.tag.trim()),
            tag,
        ]));
        changed = true;
    }
    changed.then_some(steps)
}

/// How things are after a finish stopped (see [`recover`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Now {
    /// What HEAD points to (`refs/heads/main`).
    pub head: String,
    pub merging: bool,
    pub hotfix: Option<String>,
    pub branches: Vec<(String, String)>,
    /// The tag's object, when finishing made it.
    pub tag: Option<String>,
}

/// Reads [`Now`] for `f`.
pub(crate) fn now(repo: &Path, f: &Finish, c: &Choices) -> Now {
    let raw = |r: &str| {
        git::run_ok_codes(Some(repo), &["rev-parse", "-q", "--verify", r], &[0, 1])
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    Now {
        head: git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])
            .map(|s| s.trim().to_owned())
            .unwrap_or_default(),
        merging: raw("MERGE_HEAD").is_some(),
        hotfix: raw(&format!("refs/heads/{}", f.hotfix.name)),
        branches: f
            .targets(c)
            .iter()
            .filter_map(|t| Some((t.name.clone(), raw(&format!("refs/heads/{}", t.name))?)))
            .collect(),
        // The plan refused a tag of that name, so one there now is finishing's.
        tag: raw(&format!("refs/tags/{}", c.tag.trim())),
    }
}
