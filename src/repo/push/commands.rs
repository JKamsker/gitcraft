//! Explicit push, pull and force-with-lease command assembly.

use super::*;

/// Where a refused push to `to` (the ref the refused line names) went: only a branch can be
/// looked at and replaced (not Gerrit's `refs/for/main`: `None`). The remote is the one the push
/// named, if it did (`Some(None)` leaves it to git's config); the branch is the one the line
/// names, as a remote's push refspecs may have sent it under another name than the one asked.
pub(in crate::repo) fn refused_target(asked: Option<Target>, to: &str) -> Option<Option<Target>> {
    let name = to.strip_prefix("refs/heads/")?;
    Some(asked.map(|t| Target {
        remote: t.remote,
        branch: name.to_owned(),
    }))
}

/// A first push of `branch` to `remote`, tracking it: the full ref (a tag of the same name must
/// not be what is pushed), no `:dst` (which would bypass `remote.<name>.push`, Gerrit's
/// refs/for/*), and `simple` (with push.default=upstream, a source alone goes to the branch it
/// tracks).
pub(in crate::repo) fn publish_args(branch: &str, remote: &str) -> Vec<String> {
    [
        "-c",
        "push.default=simple",
        "push",
        "--porcelain",
        "-u",
        "--",
        remote,
        &format!("refs/heads/{branch}"),
    ]
    .map(str::to_owned)
    .to_vec()
}

/// A push of `branch` to `target`, named in full; `-u` for a first one.
pub(super) fn explicit_push(
    branch: &str,
    target: &Target,
    publish: bool,
    label: &str,
) -> (Attempt, Vec<Vec<String>>) {
    let mut args = vec!["push".to_owned(), "--porcelain".to_owned()];
    if publish {
        args.push("-u".to_owned());
    }
    args.push("--".to_owned());
    args.push(target.remote.clone());
    args.push(format!("refs/heads/{branch}:refs/heads/{}", target.branch));
    let attempt = Attempt {
        label: label.to_owned(),
        branch: branch.to_owned(),
        target: Some(target.clone()),
        commit: None,
        publish,
        done: None,
    };
    (attempt, vec![args])
}

/// A pull of the remote branch `target` (which need not be the upstream), the way `mode` pulls.
pub(in crate::repo) fn pull_from(mode: PullMode, target: &Target) -> Vec<String> {
    mode.args()
        .iter()
        .map(|a| (*a).to_owned())
        .chain([
            target.remote.clone(),
            format!("refs/heads/{}", target.branch),
        ])
        .collect()
}

/// A force push of what was checked: the branch's commit then (refused if the branch has
/// moved since), over the remote's commit then (refused by git if the remote has moved since:
/// the lease). `--force-if-includes` where git has it; with the lease's commit given, as here,
/// git does no more with it, the lease is what guards.
pub(in crate::repo) fn force_push_ops(r: &Rejection) -> Vec<Vec<String>> {
    let dst = format!("refs/heads/{}", r.target.branch);
    let mut push = vec![
        "push".to_owned(),
        "--porcelain".to_owned(),
        format!("--force-with-lease={dst}:{}", r.remote),
    ];
    if r.if_includes {
        push.push("--force-if-includes".to_owned());
    }
    push.push("--".to_owned());
    push.push(r.target.remote.clone());
    push.push(format!("{}:{dst}", r.local));
    let mut steps = vec![
        expect_ref_at(&format!("refs/heads/{}", r.branch), &r.local),
        push,
    ];
    // A first push: tracked as `-u` would have (which sets nothing for a commit pushed).
    if r.publish {
        let config = |key: &str, value: &str| {
            vec![
                "config".to_owned(),
                format!("branch.{}.{key}", r.branch),
                value.to_owned(),
            ]
        };
        steps.push(config("remote", &r.target.remote));
        steps.push(config("merge", &dst));
    }
    steps
}

/// The confirmation of a force push, naming what it takes off the remote, and the push it
/// runs as it will be taken in.
pub(super) fn force_dialog(r: &Rejection) -> (Attempt, Dialog) {
    let title = format!("Force push {} to {}", r.branch, r.target.label());
    let attempt = Attempt {
        label: title.clone(),
        branch: r.branch.clone(),
        target: Some(r.target.clone()),
        commit: Some(r.local.clone()),
        publish: r.publish,
        done: None,
    };
    let dialog = Dialog::confirm(
        &title,
        banner::force_body(r),
        crate::i18n::text("repo-workflows-push-force-dialog-1d502fc3"),
        force_push_ops(r),
        None,
    );
    (attempt, dialog)
}
