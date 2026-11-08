//! Each write rechecks the exact MR, signed-in viewer and revision; merge/approval also send SHA.
use super::{
    api::{Api, segment},
    details,
};
use crate::{
    pr_post::{LineComment, MergeMethod, Suggestion, Target, Verdict, check_path},
    pull_request::{DiffSide, PrDetails, at},
};
use serde_json::{Value, json};

pub(super) struct GitLabTarget {
    api: Api,
    details: PrDetails,
    context: crate::connections::Context,
}
impl GitLabTarget {
    pub fn new(mut api: Api, details: &PrDetails) -> Option<Self> {
        if !super::api::source_matches(&api.hosting, &details.url)
            || details.forge != Some(crate::forge::Forge::GitLab)
            || details.extras.connection_binding != Some(crate::pr_threads::connection_binding())
        {
            return None;
        }
        let context = api.context.clone();
        if !api.context.choices.gitlab.selected() {
            // A click authorizes the viewer shown on the page, never a later CLI default.
            api.context.choices.gitlab = crate::connections::Account {
                host: super::api_authority(&api.hosting),
                user: details.extras.viewer.clone()?,
                config_dir: String::new(),
            };
            api.context.choices.gitlab.validate().ok()?;
        }
        Some(Self {
            context,
            api,
            details: details.clone(),
        })
    }
    fn endpoint(&self) -> String {
        self.api.mr(self.details.number)
    }
    fn guard(&self) -> Result<Value, String> {
        if self.context != crate::connections::current() {
            return Err(crate::i18n::text(
                "backend-profile-connections-changed-reload-the-merge-request-6f0b5b",
            )
            .into());
        }
        let me = self.api.get("user")?;
        if Some(at(&me, "/username")) != self.details.extras.viewer.as_deref()
            || me
                .get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                != self.details.extras.native_account
        {
            return Err(crate::i18n::text(
                "backend-the-gitlab-account-changed-reload-before-sending-ba87f5",
            )
            .into());
        }
        let mr = self.api.get(&self.endpoint())?;
        if !super::api::source_matches(&self.api.hosting, at(&mr, "/web_url"))
            || mr
                .get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .as_deref()
                != Some(self.details.extras.pr_id.as_str())
            || mr.get("iid").and_then(Value::as_u64) != Some(self.details.number)
            || at(&mr, "/state") != "opened"
            || at(&mr, "/sha") != self.details.head_oid
            || at(&mr, "/diff_refs/head_sha") != self.details.head_oid
            || at(&mr, "/diff_refs/base_sha") != self.details.base_oid
            || Some(details::revision(&mr)) != self.details.extras.review_revision
        {
            return Err(crate::i18n::text(
                "backend-the-merge-request-changed-reload-before-sending-608007",
            )
            .into());
        }
        Ok(mr)
    }
    fn body(body: &str) -> Result<(), String> {
        if body.trim().is_empty() || body.len() > 1024 * 1024 {
            Err(crate::i18n::text("backend-write-a-comment-of-at-most-1-mib-b09b42").into())
        } else if body.lines().any(|line| line.trim_start().starts_with('/')) {
            Err(crate::i18n::text(
                "backend-gitlab-quick-actions-are-not-sent-from-this-revision-bound-review-2552ca",
            )
            .into())
        } else {
            Ok(())
        }
    }
    fn note(&self, body: &str) -> Result<(), String> {
        Self::body(body)?;
        self.api
            .send(
                "POST",
                &format!("{}/notes", self.endpoint()),
                Some(&json!({"body":body})),
            )
            .map(|_| ())
    }
    fn discussion(&self, id: &str) -> Result<Value, String> {
        if !self
            .details
            .extras
            .threads
            .iter()
            .any(|thread| thread.id == id)
        {
            return Err(crate::i18n::text(
                "backend-this-discussion-was-not-present-in-the-displayed-review-reload-it-6e75ef",
            )
            .into());
        }
        self.api
            .all(&format!("{}/discussions", self.endpoint()))?
            .into_iter()
            .find(|d| at(d, "/id") == id)
            .ok_or(
                crate::i18n::text(
                    "backend-this-discussion-is-no-longer-in-the-merge-request-reload-it-f52143",
                )
                .into(),
            )
    }
}
impl Target for GitLabTarget {
    fn comment(&self, body: &str) -> Result<(), String> {
        Self::body(body)?;
        self.guard()?;
        self.note(body)
    }
    fn review(&self, verdict: Verdict, body: &str) -> Result<(), String> {
        self.guard()?;
        match verdict {
            Verdict::Comment => self.note(body),
            Verdict::Approve => {
                let approvals = self.api.get(&format!("{}/approvals", self.endpoint()))?;
                if approvals.get("user_can_approve").and_then(Value::as_bool) != Some(true) {
                    return Err(crate::i18n::text(
                        "backend-gitlab-has-not-granted-this-account-approval-rights-d2c890",
                    )
                    .into());
                }
                if !body.trim().is_empty() {
                    self.note(body)?;
                }
                self.guard()?;
                self.api
                    .send(
                        "POST",
                        &format!("{}/approve", self.endpoint()),
                        Some(&json!({"sha":self.details.head_oid})),
                    )
                    .map(|_| ())
                    .map_err(|e| {
                        if body.trim().is_empty() {
                            e
                        } else {
                            crate::i18n::message(
                                "backend-your-comment-was-published-but-approval-failed-e-2e4e79",
                                &[("detail", (e).to_string().into())],
                            )
                        }
                    })
            }
            Verdict::RequestChanges => Err(crate::i18n::text(
                "backend-gitlab-has-no-request-changes-verdict-here-send-a-discussion-comm-7d88cd",
            )
            .into()),
        }
    }
    fn comment_now(&self, comment: &LineComment) -> Result<(), String> {
        Self::body(&comment.body)?;
        check_path(&comment.path)?;
        let mr = self.guard()?;
        let diffs = self.api.all(&format!("{}/diffs", self.endpoint()))?;
        let position = position(&mr, &diffs, comment)?;
        self.guard()?;
        self.api
            .send(
                "POST",
                &format!("{}/discussions", self.endpoint()),
                Some(&json!({"body":comment.body,"position":position})),
            )
            .map(|_| ())
    }
    fn add_pending(&self, _: &LineComment) -> Result<(), String> {
        Err(crate::i18n::text(
            "backend-use-comment-now-this-gitlab-page-publishes-each-line-comment-expl-d0c051",
        )
        .into())
    }
    fn remove_pending(&self, _: &str, _: bool) -> Result<(), String> {
        Err(
            crate::i18n::text("backend-there-is-no-pending-gitlab-review-on-this-page-8a69df")
                .into(),
        )
    }
    fn discard_pending(&self) -> Result<(), String> {
        Err(
            crate::i18n::text("backend-there-is-no-pending-gitlab-review-on-this-page-8a69df")
                .into(),
        )
    }
    fn reply(&self, thread: &str, body: &str) -> Result<(), String> {
        Self::body(body)?;
        self.guard()?;
        self.discussion(thread)?;
        self.guard()?;
        self.api
            .send(
                "POST",
                &format!("{}/discussions/{}/notes", self.endpoint(), segment(thread)),
                Some(&json!({"body":body})),
            )
            .map(|_| ())
    }
    fn set_resolved(&self, thread: &str, resolved: bool) -> Result<(), String> {
        self.guard()?;
        let discussion = self.discussion(thread)?;
        if !crate::pull_request::items(&discussion, "/notes")
            .iter()
            .any(|n| n.get("resolvable").and_then(Value::as_bool) == Some(true))
        {
            return Err(crate::i18n::text(
                "backend-this-gitlab-discussion-cannot-be-resolved-e50df6",
            )
            .into());
        }
        self.guard()?;
        self.api
            .send(
                "PUT",
                &format!("{}/discussions/{}", self.endpoint(), segment(thread)),
                Some(&json!({"resolved":resolved})),
            )
            .map(|_| ())
    }
    fn dismiss(&self, _: &str, _: &str) -> Result<(), String> {
        Err(crate::i18n::text(
            "backend-gitlab-approvals-cannot-be-dismissed-with-github-review-semantics-3cd69f",
        )
        .into())
    }
    fn merge(&self, method: MergeMethod, delete_branch: bool) -> Result<(), String> {
        let mr = self.guard()?;
        let project = self.api.get(&self.api.project())?;
        let approvals = self.api.get(&format!("{}/approvals", self.endpoint()));
        if let Some(blocker) = details::blocker(&mr, &project, &approvals) {
            return Err(blocker);
        }
        let squash =
            match method {
                MergeMethod::Merge if at(&project, "/squash_option") != "always" => false,
                MergeMethod::Squash if at(&project, "/squash_option") != "never" => true,
                _ => return Err(crate::i18n::text(
                    "backend-that-merge-method-is-not-allowed-by-gitlab-s-current-policy-d79825",
                )
                .into()),
            };
        if delete_branch && self.details.extras.head_repo.as_deref() != Some(&self.api.hosting.path)
        {
            return Err(crate::i18n::text(
                "backend-deleting-a-fork-s-branch-is-not-offered-from-this-repository-665374",
            )
            .into());
        }
        self.guard()?;
        let result = self.api.send("PUT", &format!("{}/merge", self.endpoint()), Some(&json!({"sha":self.details.head_oid,"squash":squash,"should_remove_source_branch":delete_branch})))?;
        if at(&result, "/state") != "merged" {
            return Err(crate::i18n::text(
                "backend-gitlab-has-not-confirmed-the-merge-refresh-its-state-before-anoth-025a28",
            )
            .into());
        }
        Ok(())
    }
    fn apply_suggestion(&self, _: &Suggestion) -> Result<(), String> {
        Err(crate::i18n::text(
            "backend-apply-gitlab-suggestions-on-gitlab-no-direct-branch-writes-are-of-b44f97",
        )
        .into())
    }
}
/// Derive both old/new coordinates for context lines, preserving rename paths and all diff SHAs.
pub(super) fn position(
    mr: &Value,
    diffs: &[Value],
    comment: &LineComment,
) -> Result<Value, String> {
    let direct = diffs.iter().any(|d| at(d, "/new_path") == comment.path);
    let matching: Vec<_> = diffs
        .iter()
        .filter(|d| at(d, if direct { "/new_path" } else { "/old_path" }) == comment.path)
        .collect();
    if matching.len() != 1 {
        return Err(crate::i18n::text(
            "backend-the-file-is-not-uniquely-present-in-gitlab-s-current-diff-reload-3f091a",
        )
        .into());
    }
    let diff = matching[0];
    if ["too_large", "collapsed"]
        .iter()
        .any(|key| diff.get(key).and_then(Value::as_bool) == Some(true))
    {
        return Err(crate::i18n::text(
            "backend-gitlab-omitted-this-diff-reload-or-use-a-conversation-comment-43d7af",
        )
        .into());
    }
    let mut old = 0u64;
    let mut new = 0u64;
    let mut hunk = false;
    for line in at(diff, "/diff").lines() {
        if line.starts_with("@@ ") {
            let mut parts = line.split_whitespace();
            let _ = parts.next();
            let coordinate = |p: Option<&str>, sign: char| {
                p.and_then(|p| p.strip_prefix(sign))
                    .and_then(|p| p.split(',').next())
                    .and_then(|n| n.parse::<u64>().ok())
            };
            old = coordinate(parts.next(), '-').ok_or(crate::i18n::text(
                "backend-gitlab-returned-invalid-diff-coordinates-8e6a15",
            ))?;
            new = coordinate(parts.next(), '+').ok_or(crate::i18n::text(
                "backend-gitlab-returned-invalid-diff-coordinates-8e6a15",
            ))?;
            hunk = true;
            continue;
        }
        if !hunk || line.starts_with('\\') {
            continue;
        }
        let (left, right) = match line.as_bytes().first() {
            Some(b' ') => (true, true),
            Some(b'-') => (true, false),
            Some(b'+') => (false, true),
            _ => {
                return Err(crate::i18n::text(
                    "backend-gitlab-returned-an-invalid-text-diff-273858",
                )
                .into());
            }
        };
        if (comment.side == DiffSide::Left && left && old == comment.line)
            || (comment.side == DiffSide::Right && right && new == comment.line)
        {
            let mut p = json!({"position_type":"text","base_sha":at(mr,"/diff_refs/base_sha"),"start_sha":at(mr,"/diff_refs/start_sha"),"head_sha":at(mr,"/diff_refs/head_sha"),"old_path":at(diff,"/old_path"),"new_path":at(diff,"/new_path")});
            if left {
                p["old_line"] = json!(old);
            }
            if right {
                p["new_line"] = json!(new);
            }
            return Ok(p);
        }
        if left {
            old = old.checked_add(1).ok_or(crate::i18n::text(
                "backend-gitlab-diff-coordinate-overflow-b7bda1",
            ))?;
        }
        if right {
            new = new.checked_add(1).ok_or(crate::i18n::text(
                "backend-gitlab-diff-coordinate-overflow-b7bda1",
            ))?;
        }
    }
    Err(crate::i18n::text(
        "backend-that-line-was-not-included-in-gitlab-s-current-text-diff-use-a-co-945d8f",
    )
    .into())
}
