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
        return Err(crate::i18n::text("backend-invalid-merge-request-number-f1cfff").into());
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
        let viewer_after = api.get("user")?;
        if same_revision(&mr, &after) && viewer == viewer_after {
            let mut result = assemble(
                &mr,
                &project,
                &viewer,
                &approvals,
                &discussions,
                &commits,
                number,
            )?;
            if !super::api::source_matches(&api.hosting, &result.url) {
                return Err(crate::i18n::text(
                    "backend-gitlab-returned-a-merge-request-for-a-different-repository-7ae447",
                )
                .into());
            }
            result.changed_files = diffs.len() as u64;
            for diff in &diffs {
                if diff.get("too_large").and_then(Value::as_bool) == Some(true)
                    || diff.get("collapsed").and_then(Value::as_bool) == Some(true)
                {
                    result.inline_error=Some(crate::i18n::text("backend-gitlab-omitted-a-large-or-collapsed-diff-line-comments-on-omitted-67ae9d").into());
                }
                let mut in_hunk = false;
                for line in at(diff, "/diff").lines() {
                    if line.starts_with("@@ ") {
                        in_hunk = true;
                        continue;
                    }
                    if !in_hunk {
                        continue;
                    }
                    if line.starts_with('+') {
                        result.additions += 1;
                    }
                    if line.starts_with('-') {
                        result.deletions += 1;
                    }
                }
            }
            return Ok(result);
        }
    }
    Err(crate::i18n::text(
        "backend-the-merge-request-changed-while-it-was-read-reload-it-before-revi-592b0c",
    )
    .into())
}
pub(super) fn same_revision(a: &Value, b: &Value) -> bool {
    revision(a) == revision(b) && a.get("state") == b.get("state")
}
pub(super) fn revision(mr: &Value) -> String {
    serde_json::json!({"diff_refs":mr["diff_refs"], "id":mr["id"], "iid":mr["iid"], "sha":mr["sha"], "source_project_id":mr["source_project_id"], "target_project_id":mr["target_project_id"], "source_branch":mr["source_branch"], "target_branch":mr["target_branch"]}).to_string()
}

fn flag(v: &Value, p: &str) -> bool {
    v.pointer(p).and_then(Value::as_bool) == Some(true)
}
fn id(v: &Value, p: &str) -> u64 {
    v.pointer(p).and_then(Value::as_u64).unwrap_or(0)
}
/// Community Edition reports `approved` without Enterprise approval-count fields.
fn approval_state(value: &Value) -> Option<bool> {
    value.get("approved").and_then(Value::as_bool).or_else(|| {
        value
            .get("approvals_left")
            .and_then(Value::as_u64)
            .map(|n| n == 0)
    })
}
pub(super) fn blocker(
    mr: &Value,
    project: &Value,
    approvals: &Result<Value, String>,
) -> Option<String> {
    if at(mr, "/state") != "opened" {
        return Some(
            crate::i18n::text("backend-this-merge-request-is-no-longer-open-5a106c").into(),
        );
    }
    if !flag(mr, "/user/can_merge") {
        return Some(
            crate::i18n::text(
                "backend-gitlab-has-not-granted-this-account-permission-to-merge-991e9f",
            )
            .into(),
        );
    }
    if flag(mr, "/draft") || flag(mr, "/work_in_progress") {
        return Some(crate::i18n::text("backend-it-is-still-a-draft-ce75ce").into());
    }
    if !matches!(
        at(project, "/merge_method"),
        "merge" | "rebase_merge" | "ff"
    ) || !matches!(
        at(project, "/squash_option"),
        "always" | "never" | "default_on" | "default_off"
    ) {
        return Some(
            crate::i18n::text(
                "backend-gitlab-s-merge-policy-could-not-be-read-reload-before-merging-100f05",
            )
            .into(),
        );
    }
    match approvals.as_ref().ok().and_then(approval_state) {
        None => {
            return Some(
                crate::i18n::text("backend-gitlab-approval-requirements-could-not-be-read-reload-before-merg-74d3ea").into(),
            );
        }
        Some(false) => {
            return Some(
                crate::i18n::text("backend-required-gitlab-approvals-are-missing-6c2652").into(),
            );
        }
        Some(true) => {}
    }
    let status = at(mr, "/detailed_merge_status");
    let message = match status {
        "mergeable" => return None,
        "conflict" => crate::i18n::text("backend-resolve-the-merge-conflicts-first-849762"),
        "not_approved" => crate::i18n::text("backend-required-gitlab-approvals-are-missing-6c2652"),
        "discussions_not_resolved" => {
            crate::i18n::text("backend-resolve-the-required-discussions-first-5acaeb")
        }
        "ci_must_pass" | "ci_still_running" => {
            crate::i18n::text("backend-gitlab-requires-the-pipeline-to-pass-first-21e000")
        }
        "need_rebase" => crate::i18n::text(
            "backend-gitlab-requires-the-source-branch-to-be-rebased-first-cd6394",
        ),
        "checking" | "unchecked" | "approvals_syncing" => crate::i18n::text(
            "backend-gitlab-is-still-checking-merge-requirements-reload-in-a-moment-b5d0ce",
        ),
        "requested_changes" => crate::i18n::text(
            "backend-gitlab-reports-requested-changes-which-must-be-addressed-first-6dace1",
        ),
        "" => crate::i18n::text(
            "backend-gitlab-did-not-report-whether-its-merge-requirements-are-met-9a2c0c",
        ),
        _ => {
            return Some(crate::i18n::message(
                "backend-gitlab-merge-blocker-fd282e",
                &[("status", (status.replace('_', " ")).to_string().into())],
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
    if id(project, "/id") == 0 || id(mr, "/target_project_id") != id(project, "/id") {
        return Err(
            crate::i18n::text("backend-gitlab-returned-a-different-target-project-3be59d").into(),
        );
    }
    if mr.get("iid").and_then(Value::as_u64) != Some(number)
        || id(mr, "/id") == 0
        || !oid(head)
        || !oid(at(mr, "/diff_refs/base_sha"))
        || head != at(mr, "/diff_refs/head_sha")
        || !oid(at(mr, "/diff_refs/start_sha"))
    {
        return Err(crate::i18n::text(
            "backend-gitlab-did-not-return-a-complete-merge-request-revision-reload-it-4f6731",
        )
        .into());
    }
    let who = at(viewer, "/username");
    if who.is_empty() || id(viewer, "/id") == 0 {
        return Err(crate::i18n::text(
            "backend-gitlab-did-not-identify-the-signed-in-account-2b2179",
        )
        .into());
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
        native_policy: true,
        merge_blocker: blocker(mr, project, approvals),
        merge_handoff: false,
        connection_binding: Some(crate::pr_threads::connection_binding()),
        review_revision: Some(revision(mr)),
        native_account: Some(id(viewer, "/id").to_string()),
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
                if let Some(t) = thread(d, mr, who, can_resolve, at(mr, "/state") == "opened") {
                    extras.threads.push(t);
                }
            } else {
                if !flag(d, "/individual_note")
                    && let Some(t) = thread(d, mr, who, can_resolve, at(mr, "/state") == "opened")
                {
                    extras.threads.push(t);
                }
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
                name: crate::i18n::message(
                    "backend-gitlab-pipeline-26d62e",
                    &[("id", (id(p, "/id")).to_string().into())],
                ),
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
        review_decision: match approvals.as_ref().ok().and_then(approval_state) {
            Some(true)
                if approvals
                    .as_ref()
                    .is_ok_and(|a| !items(a, "/approved_by").is_empty()) =>
            {
                "APPROVED"
            }
            Some(false) => "REVIEW_REQUIRED",
            _ => "",
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
fn thread(d: &Value, mr: &Value, viewer: &str, can_resolve: bool, open: bool) -> Option<Thread> {
    let notes = items(d, "/notes");
    let first = notes.first()?;
    let p = first.get("position").unwrap_or(&Value::Null);
    let new = p.get("new_line").and_then(Value::as_u64);
    let old = p.get("old_line").and_then(Value::as_u64);
    let side = if new.is_some() {
        DiffSide::Right
    } else {
        DiffSide::Left
    };
    // The shared diff uses the post-rename path for both sides; the old coordinate
    // still identifies a removed line in that same diff.
    let path = if at(p, "/new_path").is_empty() {
        at(p, "/old_path")
    } else {
        at(p, "/new_path")
    };
    let resolved = flag(first, "/resolved");
    let resolvable = flag(first, "/resolvable");
    Some(Thread {
        id: at(d, "/id").into(),
        path: path.into(),
        line: new.or(old),
        start_line: None,
        side,
        outdated: p.is_object()
            && ["head_sha", "base_sha", "start_sha"]
                .iter()
                .any(|k| at(p, &format!("/{k}")) != at(mr, &format!("/diff_refs/{k}"))),
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
