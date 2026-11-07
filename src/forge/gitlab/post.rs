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
}
impl GitLabTarget {
    pub fn new(api: Api, details: &PrDetails) -> Option<Self> {
        let (host, path) = crate::pull_request::repo_of_url(&details.url)?;
        if host != api.hosting.host
            || path != api.hosting.path
            || details.forge != Some(crate::forge::Forge::GitLab)
            || details.extras.connection_binding != Some(crate::pr_threads::connection_binding())
        {
            return None;
        }
        Some(Self {
            api,
            details: details.clone(),
        })
    }
    fn endpoint(&self) -> String {
        self.api.mr(self.details.number)
    }
    fn guard(&self) -> Result<Value, String> {
        if self.api.context != crate::connections::current() {
            return Err("Profile connections changed. Reload the merge request.".into());
        }
        let me = self.api.get("user")?;
        if Some(at(&me, "/username")) != self.details.extras.viewer.as_deref() {
            return Err("The GitLab account changed. Reload before sending.".into());
        }
        let mr = self.api.get(&self.endpoint())?;
        if at(&mr, "/state") != "opened"
            || at(&mr, "/sha") != self.details.head_oid
            || at(&mr, "/diff_refs/head_sha") != self.details.head_oid
            || at(&mr, "/diff_refs/base_sha") != self.details.base_oid
            || mr.get("diff_refs").map(Value::to_string) != self.details.extras.review_revision
        {
            return Err("The merge request changed. Reload before sending.".into());
        }
        Ok(mr)
    }
    fn body(body: &str) -> Result<(), String> {
        if body.trim().is_empty() || body.len() > 1024 * 1024 {
            Err("Write a comment of at most 1 MiB".into())
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
        self.api
            .all(&format!("{}/discussions", self.endpoint()))?
            .into_iter()
            .find(|d| at(d, "/id") == id)
            .ok_or("This discussion is no longer in the merge request. Reload it.".into())
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
                    return Err("GitLab has not granted this account approval rights.".into());
                }
                if !body.trim().is_empty() {
                    self.note(body)?;
                }
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
                            format!("Your comment was published, but approval failed: {e}")
                        }
                    })
            }
            Verdict::RequestChanges => Err(
                "GitLab has no request-changes verdict here; send a discussion comment instead."
                    .into(),
            ),
        }
    }
    fn comment_now(&self, comment: &LineComment) -> Result<(), String> {
        Self::body(&comment.body)?;
        check_path(&comment.path)?;
        let mr = self.guard()?;
        let diffs = self.api.all(&format!("{}/diffs", self.endpoint()))?;
        let position = position(&mr, &diffs, comment)?;
        self.api
            .send(
                "POST",
                &format!("{}/discussions", self.endpoint()),
                Some(&json!({"body":comment.body,"position":position})),
            )
            .map(|_| ())
    }
    fn add_pending(&self, _: &LineComment) -> Result<(), String> {
        Err("Use Comment now: this GitLab page publishes each line comment explicitly.".into())
    }
    fn remove_pending(&self, _: &str, _: bool) -> Result<(), String> {
        Err("There is no pending GitLab review on this page.".into())
    }
    fn discard_pending(&self) -> Result<(), String> {
        Err("There is no pending GitLab review on this page.".into())
    }
    fn reply(&self, thread: &str, body: &str) -> Result<(), String> {
        Self::body(body)?;
        self.guard()?;
        self.discussion(thread)?;
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
            return Err("This GitLab discussion cannot be resolved.".into());
        }
        self.api
            .send(
                "PUT",
                &format!("{}/discussions/{}", self.endpoint(), segment(thread)),
                Some(&json!({"resolved":resolved})),
            )
            .map(|_| ())
    }
    fn dismiss(&self, _: &str, _: &str) -> Result<(), String> {
        Err("GitLab approvals cannot be dismissed with GitHub review semantics.".into())
    }
    fn merge(&self, method: MergeMethod, delete_branch: bool) -> Result<(), String> {
        let mr = self.guard()?;
        let project = self.api.get(&self.api.project())?;
        let approvals = self.api.get(&format!("{}/approvals", self.endpoint()));
        if let Some(blocker) = details::blocker(&mr, &project, &approvals) {
            return Err(blocker);
        }
        let squash = match method {
            MergeMethod::Merge if at(&project, "/squash_option") != "always" => false,
            MergeMethod::Squash if at(&project, "/squash_option") != "never" => true,
            _ => return Err("That merge method is not allowed by GitLab's current policy.".into()),
        };
        if delete_branch && self.details.extras.head_repo.as_deref() != Some(&self.api.hosting.path)
        {
            return Err("Deleting a fork's branch is not offered from this repository.".into());
        }
        self.api.send("PUT",&format!("{}/merge",self.endpoint()),Some(&json!({"sha":self.details.head_oid,"squash":squash,"should_remove_source_branch":delete_branch}))).map(|_|())
    }
    fn apply_suggestion(&self, _: &Suggestion) -> Result<(), String> {
        Err("Apply GitLab suggestions on GitLab; no direct branch writes are offered here.".into())
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
        return Err("The file is not uniquely present in GitLab's current diff. Reload it.".into());
    }
    let diff = matching[0];
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
            old =
                coordinate(parts.next(), '-').ok_or("GitLab returned invalid diff coordinates")?;
            new =
                coordinate(parts.next(), '+').ok_or("GitLab returned invalid diff coordinates")?;
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
            _ => return Err("GitLab returned an invalid text diff".into()),
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
            old = old
                .checked_add(1)
                .ok_or("GitLab diff coordinate overflow")?;
        }
        if right {
            new = new
                .checked_add(1)
                .ok_or("GitLab diff coordinate overflow")?;
        }
    }
    Err("That line was not included in GitLab's current text diff; use a conversation comment or reload.".into())
}
