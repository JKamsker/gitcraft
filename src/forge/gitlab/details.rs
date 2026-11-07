//! A coherent GitLab revision; native policy is kept separate from GitHub merge states.
use super::api::Api;
use crate::forge::Forge;
use crate::pr_threads::{PrExtras, ReviewFeatures, Thread, ThreadComment};
use crate::pull_request::{
    Check, CheckState, DiffSide, Entry, EntryKind, PrCommit, PrDetails, at, items, line_entries,
    parse_time,
};
use serde_json::Value;

pub(super) fn read(api: &Api, number: u64) -> Result<PrDetails, String> {
    if number == 0 {
        return Err("Invalid merge request number".into());
    }
    for _ in 0..2 {
        let mr = api.get(&api.mr(number))?;
        let project = api.get(&api.project())?;
        let viewer = api.get("user")?;
        let approvals = api.get(&format!("{}/approvals", api.mr(number)));
        let discussions = api.all(&format!("{}/discussions", api.mr(number)));
        let commits = api.all(&format!("{}/commits", api.mr(number)))?;
        let diffs = api.all(&format!("{}/diffs", api.mr(number)))?;
        let after = api.get(&api.mr(number))?;
        if same_revision(&mr, &after) {
            let mut result = assemble(
                &mr,
                &project,
                &viewer,
                &approvals,
                &discussions,
                &commits,
                number,
            )?;
            let identity = crate::pull_request::repo_of_url(&result.url);
            if identity != Some((api.hosting.host.clone(), api.hosting.path.clone())) {
                return Err("GitLab returned a merge request for a different repository".into());
            }
            result.changed_files = diffs.len() as u64;
            for diff in &diffs {
                if diff.get("too_large").and_then(Value::as_bool) == Some(true)
                    || diff.get("collapsed").and_then(Value::as_bool) == Some(true)
                {
                    result.inline_error=Some("GitLab omitted a large or collapsed diff; line comments on omitted lines are unavailable.".into());
                }
                for line in at(diff, "/diff").lines() {
                    if line.starts_with('+') && !line.starts_with("+++") {
                        result.additions += 1;
                    }
                    if line.starts_with('-') && !line.starts_with("---") {
                        result.deletions += 1;
                    }
                }
            }
            return Ok(result);
        }
    }
    Err("The merge request changed while it was read. Reload it before reviewing.".into())
}
pub(super) fn same_revision(a: &Value, b: &Value) -> bool {
    at(a, "/sha") == at(b, "/sha")
        && a.get("diff_refs") == b.get("diff_refs")
        && a.get("state") == b.get("state")
}
fn flag(v: &Value, p: &str) -> bool {
    v.pointer(p).and_then(Value::as_bool) == Some(true)
}
fn id(v: &Value, p: &str) -> u64 {
    v.pointer(p).and_then(Value::as_u64).unwrap_or(0)
}
pub(super) fn blocker(
    mr: &Value,
    project: &Value,
    approvals: &Result<Value, String>,
) -> Option<String> {
    if at(mr, "/state") != "opened" {
        return Some("This merge request is no longer open.".into());
    }
    if !flag(mr, "/user/can_merge") {
        return Some("GitLab has not granted this account permission to merge.".into());
    }
    if flag(mr, "/draft") || flag(mr, "/work_in_progress") {
        return Some("It is still a draft.".into());
    }
    if !matches!(
        at(project, "/merge_method"),
        "merge" | "rebase_merge" | "ff"
    ) || !matches!(
        at(project, "/squash_option"),
        "always" | "never" | "default_on" | "default_off"
    ) {
        return Some("GitLab's merge policy could not be read. Reload before merging.".into());
    }
    if approvals.is_err() {
        return Some(
            "GitLab approval requirements could not be read. Reload before merging.".into(),
        );
    }
    let status = at(mr, "/detailed_merge_status");
    let message = match status {
        "mergeable" => return None,
        "conflict" => "Resolve the merge conflicts first.",
        "not_approved" => "Required GitLab approvals are missing.",
        "discussions_not_resolved" => "Resolve the required discussions first.",
        "ci_must_pass" | "ci_still_running" => "GitLab requires the pipeline to pass first.",
        "need_rebase" => "GitLab requires the source branch to be rebased first.",
        "checking" | "unchecked" | "approvals_syncing" => {
            "GitLab is still checking merge requirements. Reload in a moment."
        }
        "requested_changes" => "GitLab reports requested changes which must be addressed first.",
        "" => "GitLab did not report whether its merge requirements are met.",
        _ => {
            return Some(format!(
                "GitLab merge blocker: {}",
                status.replace('_', " ")
            ));
        }
    };
    Some(message.into())
}
pub(super) fn assemble(
    mr: &Value,
    project: &Value,
    viewer: &Value,
    approvals: &Result<Value, String>,
    discussions: &Result<Vec<Value>, String>,
    commits: &[Value],
    number: u64,
) -> Result<PrDetails, String> {
    let head = at(mr, "/sha");
    if mr.get("iid").and_then(Value::as_u64) != Some(number)
        || id(mr, "/id") == 0
        || !oid(head)
        || !oid(at(mr, "/diff_refs/base_sha"))
        || head != at(mr, "/diff_refs/head_sha")
        || !oid(at(mr, "/diff_refs/start_sha"))
    {
        return Err("GitLab did not return a complete merge request revision. Reload it.".into());
    }
    let who = at(viewer, "/username");
    if who.is_empty() {
        return Err("GitLab did not identify the signed-in account".into());
    }
    let permission = if flag(mr, "/user/can_merge") {
        "WRITE"
    } else {
        "READ"
    };
    let same_project = mr.get("source_project_id") == mr.get("target_project_id")
        && id(mr, "/source_project_id") > 0;
    let policy_known = matches!(
        at(project, "/merge_method"),
        "merge" | "rebase_merge" | "ff"
    );
    let mut extras = PrExtras {
        pr_id: id(mr, "/id").to_string(),
        viewer: Some(who.into()),
        permission: permission.into(),
        review_features: ReviewFeatures {
            pending: false,
            request_changes: false,
            approve: approvals
                .as_ref()
                .is_ok_and(|a| flag(a, "/user_can_approve")),
            apply_suggestions: false,
        },
        merge_blocker: blocker(mr, project, approvals),
        connection_binding: Some(crate::pr_threads::connection_binding()),
        merge_allowed: policy_known && at(project, "/squash_option") != "always",
        squash_allowed: policy_known
            && matches!(
                at(project, "/squash_option"),
                "always" | "default_on" | "default_off"
            ),
        rebase_allowed: false,
        delete_branch_on_merge: flag(mr, "/force_remove_source_branch"),
        head_repo: same_project.then(|| at(project, "/path_with_namespace").to_owned()),
        pending_review: None,
        head_oid: head.into(),
        can_bypass_rules: false,
        threads: Vec::new(),
    };
    let can_resolve = flag(mr, "/user/can_merge")
        || who == at(mr, "/author/username")
        || [
            id(project, "/permissions/project_access/access_level"),
            id(project, "/permissions/group_access/access_level"),
        ]
        .into_iter()
        .any(|v| v >= 30);
    let mut timeline = Vec::new();
    if let Ok(ds) = discussions {
        for d in ds {
            let notes = items(d, "/notes");
            let Some(first) = notes.first() else {
                continue;
            };
            if first.get("position").is_some_and(Value::is_object) {
                if let Some(t) = thread(d, head, who, can_resolve, at(mr, "/state") == "opened") {
                    extras.threads.push(t);
                }
            } else {
                timeline.extend(notes.iter().map(|n| Entry {
                    kind: EntryKind::Comment,
                    author: at(n, "/author/username").into(),
                    at: parse_time(at(n, "/created_at")).unwrap_or(0),
                    body: at(n, "/body").into(),
                    url: format!("{}#note_{}", at(mr, "/web_url"), id(n, "/id")),
                    review_id: None,
                }));
            }
        }
    }
    if let Ok(a) = approvals {
        for a in items(a, "/approved_by") {
            timeline.push(Entry {
                kind: EntryKind::Review("approved"),
                author: at(a, "/user/username").into(),
                at: 0,
                body: String::new(),
                url: at(mr, "/web_url").into(),
                review_id: None,
            });
        }
    }
    timeline.extend(line_entries(&extras));
    timeline.sort_by_key(|e| e.at);
    let pipeline = mr.get("head_pipeline").filter(|p| p.is_object());
    let checks = pipeline
        .map(|p| {
            vec![Check {
                name: format!("GitLab pipeline #{}", id(p, "/id")),
                state: match at(p, "/status") {
                    "success" => CheckState::Passed,
                    "failed" => CheckState::Failed,
                    "running" | "pending" | "created" | "preparing" | "waiting_for_resource" => {
                        CheckState::Running
                    }
                    _ => CheckState::Skipped,
                },
                detail: at(p, "/status").into(),
                url: at(p, "/web_url").into(),
            }]
        })
        .unwrap_or_default();
    Ok(PrDetails {
        forge: Some(Forge::GitLab),
        number,
        title: at(mr, "/title").into(),
        body: at(mr, "/description").into(),
        state: match at(mr, "/state") {
            "opened" => "OPEN",
            "merged" => "MERGED",
            _ => "CLOSED",
        }
        .into(),
        draft: flag(mr, "/draft") || flag(mr, "/work_in_progress"),
        author: at(mr, "/author/username").into(),
        url: at(mr, "/web_url").into(),
        base_branch: at(mr, "/target_branch").into(),
        base_oid: at(mr, "/diff_refs/base_sha").into(),
        head_branch: at(mr, "/source_branch").into(),
        head_oid: head.into(),
        head_ref: format!("refs/merge-requests/{number}/head"),
        created_at: parse_time(at(mr, "/created_at")).unwrap_or(0),
        updated_at: parse_time(at(mr, "/updated_at")).unwrap_or(0),
        additions: 0,
        deletions: 0,
        changed_files: at(mr, "/changes_count").parse().unwrap_or(0),
        mergeable: if flag(mr, "/has_conflicts") {
            "CONFLICTING"
        } else if at(mr, "/detailed_merge_status") == "mergeable" {
            "MERGEABLE"
        } else {
            "UNKNOWN"
        }
        .into(),
        merge_state: if extras.merge_blocker.is_some() {
            "BLOCKED"
        } else {
            "CLEAN"
        }
        .into(),
        review_decision: if approvals
            .as_ref()
            .is_ok_and(|a| id(a, "/approvals_left") == 0)
        {
            "APPROVED"
        } else {
            "REVIEW_REQUIRED"
        }
        .into(),
        labels: items(mr, "/labels")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        checks,
        checks_error: None,
        commits: commits
            .iter()
            .map(|c| PrCommit {
                oid: at(c, "/id").into(),
                headline: at(c, "/title").into(),
                author: at(c, "/author_name").into(),
            })
            .collect(),
        timeline,
        inline_error: discussions.as_ref().err().cloned(),
        extras,
    })
}
pub(super) fn oid(v: &str) -> bool {
    matches!(v.len(), 40 | 64) && v.bytes().all(|b| b.is_ascii_hexdigit())
}
fn thread(d: &Value, head: &str, viewer: &str, can_resolve: bool, open: bool) -> Option<Thread> {
    let notes = items(d, "/notes");
    let first = notes.first()?;
    let p = first.get("position")?;
    let new = p.get("new_line").and_then(Value::as_u64);
    let old = p.get("old_line").and_then(Value::as_u64);
    let side = if new.is_some() {
        DiffSide::Right
    } else {
        DiffSide::Left
    };
    let path = at(
        p,
        if side == DiffSide::Right {
            "/new_path"
        } else {
            "/old_path"
        },
    );
    if path.is_empty() {
        return None;
    }
    let resolved = flag(first, "/resolved");
    let resolvable = flag(first, "/resolvable");
    Some(Thread {
        id: at(d, "/id").into(),
        path: path.into(),
        line: new.or(old),
        start_line: None,
        side,
        outdated: at(p, "/head_sha") != head,
        resolved,
        can_resolve: resolvable && can_resolve && !resolved,
        can_unresolve: resolvable && can_resolve && resolved,
        can_reply: open && !viewer.is_empty(),
        comments: notes
            .iter()
            .map(|n| ThreadComment {
                id: id(n, "/id").to_string(),
                author: at(n, "/author/username").into(),
                at: parse_time(at(n, "/created_at")).unwrap_or(0),
                body: at(n, "/body").into(),
                url: String::new(),
                pending: false,
            })
            .collect(),
        starts_left: side == DiffSide::Left,
        more_comments: false,
    })
}
