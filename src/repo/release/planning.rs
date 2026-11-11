//! Build release plans and undo commands; verify completed release artifacts.

use super::*;

/// The release the form asks for.
pub(super) fn plan_of(draft: &Draft, form: &Form) -> Plan {
    let tag = form.tag.trim().to_owned();
    let version = release::version::Version::parse(&tag)
        .map(|v| v.number())
        .unwrap_or_else(|| {
            tag.trim_start_matches(|c: char| !c.is_ascii_digit())
                .to_owned()
        });
    let bump = (form.bump || form.changelog)
        .then(|| draft.branch.clone())
        .flatten()
        .filter(|_| draft.manifest_note.is_none())
        .map(|branch| release::Bump {
            remote_ref: draft
                .remote_ref
                .clone()
                .unwrap_or_else(|| format!("refs/heads/{branch}")),
            branch,
            version,
            manifests: if form.bump {
                draft.manifests.clone()
            } else {
                Vec::new()
            },
            changelog: form.changelog && draft.changelog,
        });
    Plan {
        commit: draft.commit.clone(),
        tag,
        notes: form.notes.trim().to_owned(),
        bump,
        sign: form.sign,
        push: draft.remote.clone().filter(|_| form.push),
        publish: form.publish && form.push,
        prerelease: form.prerelease,
        assets: if form.publish {
            form.assets.clone()
        } else {
            Vec::new()
        },
    }
}

/// How the local steps done are undone (and redone): the tag deleted, the version commit taken
/// back off the branch, each only while it is still as the release left it. What was pushed and
/// published stays.
pub(crate) fn undo(plan: &Plan, done: &Done) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    use crate::repo::operations::{expect_ref_at, expect_symref};
    let s = |args: &[&str]| {
        args.iter()
            .map(|a| (*a).to_owned())
            .collect::<Vec<String>>()
    };
    let tag_ref = format!("refs/tags/{}", plan.tag);
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    // The tag checked by its commit (what the check reads), put back as its object.
    if let Some(commit) = &done.tag_commit {
        undo.push(expect_ref_at(&tag_ref, commit));
    }
    if let (Some(bump), Some(commit)) = (&plan.bump, &done.bump_commit) {
        let branch = format!("refs/heads/{}", bump.branch);
        undo.push(expect_symref("HEAD", &branch));
        undo.push(expect_ref_at(&branch, commit));
        redo.push(expect_symref("HEAD", &branch));
        redo.push(expect_ref_at(&branch, &plan.commit));
        redo.push(s(&["reset", "--keep", commit]));
    }
    if done.tag_object.is_some() {
        undo.push(s(&["tag", "-d", &plan.tag]));
    }
    if done.bump_commit.is_some() {
        undo.push(s(&["reset", "--keep", &plan.commit]));
    }
    if let Some(object) = &done.tag_object {
        redo.push(s(&["update-ref", &tag_ref, object, ""]));
    }
    (undo, redo)
}

/// Why what the steps done left isn't there any more (`None`: it is), before resuming.
pub(super) fn still_there(repo: &std::path::Path, plan: &Plan, done: &Done) -> Option<String> {
    use crate::git;
    if let Some(commit) = &done.tag_commit
        && git::resolve(repo, &format!("refs/tags/{}", plan.tag))
            .ok()
            .as_ref()
            != Some(commit)
    {
        return Some(crate::i18n::message(
            "repo-workflows-release-still-there-a632e5ba",
            &[("plan-tag-1", (plan.tag).clone().into())],
        ));
    }
    if let (Some(bump), Some(commit)) = (&plan.bump, &done.bump_commit)
        && git::run(
            Some(repo),
            &[
                "merge-base",
                "--is-ancestor",
                commit,
                &format!("refs/heads/{}", bump.branch),
            ],
        )
        .is_err()
    {
        return Some(crate::i18n::message(
            "repo-workflows-release-still-there-46993d84",
            &[("bump-branch-1", (bump.branch).clone().into())],
        ));
    }
    None
}
