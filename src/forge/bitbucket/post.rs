use super::*;
use crate::{
    pr_post::{LineComment, MergeMethod, Suggestion, Target, Verdict},
    pull_request::{DiffSide, PrDetails},
};
pub(super) struct BitbucketTarget {
    api: Api,
    shown: PrDetails,
}
impl BitbucketTarget {
    pub fn new(api: Api, d: &PrDetails) -> Option<Self> {
        (d.forge == Some(Forge::Bitbucket)
            && d.url == url(&api.hosting, d.number)
            && d.extras.native_account.as_deref() == Some(&api.viewer)
            && d.extras.connection_binding == Some(crate::pr_threads::connection_binding()))
        .then(|| Self {
            api,
            shown: d.clone(),
        })
    }
    fn guard(&self) -> Result<details::Snapshot, String> {
        self.api.guard_context()?;
        let s = details::snapshot(&self.api, self.shown.number)?;
        if text(&s.pr, "/state") != "OPEN"
            || Some(s.revision()) != self.shown.extras.review_revision
        {
            return Err(crate::i18n::text(
                "backend-bitbucket-source-target-or-repository-changed-reload-before-writi-29589e",
            )
            .into());
        }
        Ok(s)
    }
    fn end(&self) -> Result<String, String> {
        self.guard()?.end(&self.api)
    }
    fn thread(&self, id: &str) -> Result<String, String> {
        if id.is_empty()
            || !id.bytes().all(|c| c.is_ascii_digit())
            || !self.shown.extras.threads.iter().any(|t| t.id == id)
        {
            return Err(crate::i18n::text(
                "backend-comment-thread-was-not-displayed-in-this-bitbucket-pr-3d13df",
            )
            .into());
        }
        let p = format!("{}/comments/{id}", self.end()?);
        let c = self.api.get(&p)?;
        if c.get("id")
            .and_then(Value::as_u64)
            .map(|n| n.to_string())
            .as_deref()
            != Some(id)
            || c.get("deleted").and_then(Value::as_bool) == Some(true)
        {
            return Err(crate::i18n::text(
                "backend-bitbucket-comment-is-no-longer-available-7544d1",
            )
            .into());
        }
        Ok(p)
    }
}
fn body_ok(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 1024 * 1024 {
        Err(crate::i18n::text("backend-write-a-comment-of-at-most-1-mib-b09b42").into())
    } else {
        Ok(())
    }
}
impl Target for BitbucketTarget {
    fn comment(&self, b: &str) -> Result<(), String> {
        body_ok(b)?;
        let e = self.end()?;
        self.api
            .send(
                "POST",
                &format!("{e}/comments"),
                Some(&json!({"content":{"raw":b}})),
            )
            .map(|_| ())
    }
    fn review(&self, v: Verdict, b: &str) -> Result<(), String> {
        if v == Verdict::Comment {
            return self.comment(b);
        }
        let s = self.guard()?;
        if text(&s.pr, "/author/uuid").eq_ignore_ascii_case(&self.api.viewer) {
            return Err(crate::i18n::text("backend-review-your-own-pr-on-bitbucket-878675").into());
        }
        if !b.trim().is_empty() {
            self.comment(b)?;
        }
        (||{let e=self.end()?;self.api.send("POST",&format!("{e}/{}",if v==Verdict::Approve{"approve"}else{"request-changes"}),None).map(|_|())})().map_err(|e|if b.trim().is_empty(){e}else{crate::i18n::message("backend-comment-was-published-review-verdict-failed-e-refresh-before-anot-33a726", &[("detail", (e).to_string().into())])})
    }
    fn comment_now(&self, c: &LineComment) -> Result<(), String> {
        body_ok(&c.body)?;
        crate::pr_post::check_path(&c.path)?;
        if c.line == 0 {
            return Err(crate::i18n::text("backend-choose-a-displayed-line-b45ef3").into());
        }
        let end = self.end()?;
        let changes = diffstat(&self.api, &self.shown.base_oid, &self.shown.head_oid)?;
        let matching = changes
            .iter()
            .filter(|v| text(v, "/new/path") == c.path || text(v, "/old/path") == c.path)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(crate::i18n::text(
                "backend-file-is-not-unambiguously-in-the-displayed-bitbucket-changes-881ac3",
            )
            .into());
        }
        let side = if c.side == DiffSide::Left {
            "old"
        } else {
            "new"
        };
        let file = required(matching[0], &format!("/{side}/path"))?;
        crate::pr_post::check_path(file)?;
        let sha = if c.side == DiffSide::Left {
            &self.shown.base_oid
        } else {
            &self.shown.head_oid
        };
        let bytes = self.api.bytes(
            "GET",
            &format!(
                "{}/src/{sha}/{}",
                endpoint(&self.api.hosting)?,
                crate::pr_post::encode_path(file)
            ),
            None,
            "text/plain",
        )?;
        let content = std::str::from_utf8(&bytes)
            .map_err(|_| crate::i18n::text("backend-cannot-comment-on-a-binary-file-61a4c9"))?;
        if c.line > content.lines().count() as u64 {
            return Err(crate::i18n::text(
                "backend-line-is-not-in-the-displayed-file-revision-4d162a",
            )
            .into());
        }
        let mut inline = json!({"path":c.path});
        inline[if c.side == DiffSide::Left {
            "from"
        } else {
            "to"
        }] = json!(c.line);
        self.guard()?;
        self.api
            .send(
                "POST",
                &format!("{end}/comments"),
                Some(&json!({"content":{"raw":c.body},"inline":inline})),
            )
            .map(|_| ())
    }
    fn reply(&self, id: &str, b: &str) -> Result<(), String> {
        body_ok(b)?;
        self.thread(id)?;
        let end = self.end()?;
        let id = id
            .parse::<u64>()
            .map_err(|_| crate::i18n::text("backend-invalid-comment-id-042678"))?;
        self.api
            .send(
                "POST",
                &format!("{end}/comments"),
                Some(&json!({"content":{"raw":b},"parent":{"id":id}})),
            )
            .map(|_| ())
    }
    fn set_resolved(&self, id: &str, resolved: bool) -> Result<(), String> {
        let p = self.thread(id)?;
        self.guard()?;
        self.api
            .send(
                if resolved { "POST" } else { "DELETE" },
                &format!("{p}/resolve"),
                None,
            )
            .map(|_| ())
    }
    fn add_pending(&self, _: &LineComment) -> Result<(), String> {
        Err(crate::i18n::text(
            "backend-bitbucket-comments-publish-explicitly-no-pending-review-226dee",
        )
        .into())
    }
    fn remove_pending(&self, _: &str, _: bool) -> Result<(), String> {
        Err(crate::i18n::text("backend-no-bitbucket-pending-review-here-4de8d2").into())
    }
    fn discard_pending(&self) -> Result<(), String> {
        Err(crate::i18n::text("backend-no-bitbucket-pending-review-here-4de8d2").into())
    }
    fn dismiss(&self, _: &str, _: &str) -> Result<(), String> {
        Err(crate::i18n::text("backend-manage-review-verdicts-on-bitbucket-a804c5").into())
    }
    fn merge(&self, _: MergeMethod, _: bool) -> Result<(), String> {
        Err(crate::i18n::text(
            "backend-bitbucket-s-documented-merge-api-has-no-expected-head-condition-c-d2faaf",
        )
        .into())
    }
    fn apply_suggestion(&self, _: &Suggestion) -> Result<(), String> {
        Err(crate::i18n::text("backend-apply-suggestions-on-bitbucket-0925b0").into())
    }
}
