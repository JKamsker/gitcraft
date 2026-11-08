use super::*;
use crate::{
    pr_post::{LineComment, MergeMethod, Suggestion, Target, Verdict},
    pull_request::{DiffSide, PrDetails},
};
pub(super) struct AzureTarget {
    api: Api,
    shown: PrDetails,
}
impl AzureTarget {
    pub fn new(api: Api, d: &PrDetails) -> Option<Self> {
        if d.forge != Some(Forge::Azure)
            || d.url != pr_url(&api.hosting, d.number)
            || d.extras.native_account.as_deref() != Some(&api.viewer)
            || d.extras.connection_binding != Some(crate::pr_threads::connection_binding())
        {
            return None;
        }
        Some(Self {
            api,
            shown: d.clone(),
        })
    }
    fn guard(&self) -> Result<details::Snapshot, String> {
        self.api.guard_context()?;
        let s = details::snapshot(&self.api, self.shown.number)?;
        if text(&s.pr, "/status") != "active"
            || Some(s.revision()) != self.shown.extras.review_revision
        {
            return Err(
                "Azure source, target or displayed iteration changed; reload before writing".into(),
            );
        }
        Ok(s)
    }
    fn thread(&self, id: &str) -> Result<String, String> {
        if !id.bytes().all(|b| b.is_ascii_digit())
            || !self.shown.extras.threads.iter().any(|t| t.id == id)
        {
            return Err("Discussion was not in this displayed Azure PR".into());
        }
        let s = self.guard()?;
        let path = format!("{}/threads/{id}", s.endpoint(self.shown.number));
        let v = self.api.get(&path)?;
        if v.get("id")
            .and_then(Value::as_u64)
            .map(|v| v.to_string())
            .as_deref()
            != Some(id)
            || v.get("isDeleted").and_then(Value::as_bool) == Some(true)
        {
            return Err("Azure discussion is no longer available".into());
        }
        Ok(path)
    }
    fn note(&self, body: &str) -> Result<(), String> {
        body_ok(body)?;
        let s = self.guard()?;
        self.api.send("POST",&format!("{}/threads",s.endpoint(self.shown.number)),Some(&json!({"comments":[{"parentCommentId":0,"content":body,"commentType":1}],"status":1}))).map(|_|())
    }
}
fn body_ok(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 1024 * 1024 {
        Err("Write a comment of at most 1 MiB".into())
    } else {
        Ok(())
    }
}
impl Target for AzureTarget {
    fn comment(&self, b: &str) -> Result<(), String> {
        self.note(b)
    }
    fn review(&self, v: Verdict, b: &str) -> Result<(), String> {
        if v == Verdict::Comment {
            return self.note(b);
        }
        let s = self.guard()?;
        if text(&s.pr, "/createdBy/id").eq_ignore_ascii_case(&self.api.viewer) {
            return Err("Azure does not accept your review vote on your own PR here".into());
        }
        if !b.trim().is_empty() {
            self.note(b)?;
        }
        let vote = (|| {
            let s = self.guard()?;
            self.api
                .send(
                    "PUT",
                    &format!(
                        "{}/reviewers/{}",
                        s.endpoint(self.shown.number),
                        segment(&self.api.viewer)
                    ),
                    Some(&json!({"id":self.api.viewer,"vote":if v==Verdict::Approve{10}else{-5}})),
                )
                .map(|_| ())
        })();
        vote.map_err(|e| {
            if b.trim().is_empty() {
                e
            } else {
                format!(
                    "Comment was published; review vote failed: {e}. Refresh before another attempt"
                )
            }
        })
    }
    fn comment_now(&self, c: &LineComment) -> Result<(), String> {
        body_ok(&c.body)?;
        crate::pr_post::check_path(&c.path)?;
        if c.line == 0 {
            return Err("Choose a displayed line".into());
        }
        let s = self.guard()?;
        let n = self.shown.number;
        let iteration = s.iteration_id();
        let mut changes = vec![];
        let mut skip = 0;
        loop {
            let v = self.api.get(&format!(
                "{}/iterations/{iteration}/changes?$compareTo=0&$top=2000&$skip={skip}",
                s.endpoint(n)
            ))?;
            changes.extend(
                v.get("changeEntries")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or("Azure did not return iteration changes")?,
            );
            let next = v.get("nextSkip").and_then(Value::as_u64).unwrap_or(0);
            if next == 0 {
                break;
            }
            if next <= skip || changes.len() > 10000 {
                return Err("Azure iteration changes exceed the supported bound".into());
            }
            skip = next;
        }
        let matching = changes
            .iter()
            .filter(|v| text(v, "/item/path").trim_start_matches('/') == c.path)
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err("File is not unambiguously in the displayed Azure iteration".into());
        }
        let change = matching[0];
        let tracking = change
            .get("changeTrackingId")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or("Azure did not return a changeTrackingId")?;
        let file = if c.side == DiffSide::Left && !text(change, "/originalPath").is_empty() {
            text(change, "/originalPath")
        } else {
            text(change, "/item/path")
        };
        let oid = if c.side == DiffSide::Left {
            &self.shown.base_oid
        } else {
            &self.shown.head_oid
        };
        let content=self.api.get(&format!("{}/items?path={}&versionDescriptor.versionType=commit&versionDescriptor.version={oid}&includeContent=true",s.repo.endpoint,segment(&file)))?;
        if c.line > required(&content, "/content")?.lines().count() as u64 {
            return Err("Line is not in the displayed commit's file".into());
        }
        let mut context = json!({"filePath":text(change,"/item/path")});
        let side = if c.side == DiffSide::Left {
            "leftFile"
        } else {
            "rightFile"
        };
        context[format!("{side}Start")] = json!({"line":c.line,"offset":1});
        context[format!("{side}End")] = json!({"line":c.line,"offset":1});
        self.guard()?;
        self.api.send("POST",&format!("{}/threads",s.endpoint(n)),Some(&json!({"comments":[{"parentCommentId":0,"content":c.body,"commentType":1}],"status":1,"threadContext":context,"pullRequestThreadContext":{"changeTrackingId":tracking,"iterationContext":{"firstComparingIteration":iteration,"secondComparingIteration":iteration}}}))).map(|_|())
    }
    fn reply(&self, id: &str, b: &str) -> Result<(), String> {
        body_ok(b)?;
        let p = self.thread(id)?;
        self.guard()?;
        self.api
            .send(
                "POST",
                &format!("{p}/comments"),
                Some(&json!({"content":b,"commentType":1,"parentCommentId":0})),
            )
            .map(|_| ())
    }
    fn set_resolved(&self, id: &str, resolved: bool) -> Result<(), String> {
        let p = self.thread(id)?;
        self.guard()?;
        self.api
            .send("PATCH", &p, Some(&json!({"status":if resolved{2}else{1}})))
            .map(|_| ())
    }
    fn add_pending(&self, _: &LineComment) -> Result<(), String> {
        Err("Azure comments publish explicitly; no pending review".into())
    }
    fn remove_pending(&self, _: &str, _: bool) -> Result<(), String> {
        Err("Azure has no pending review here".into())
    }
    fn discard_pending(&self) -> Result<(), String> {
        Err("Azure has no pending review here".into())
    }
    fn dismiss(&self, _: &str, _: &str) -> Result<(), String> {
        Err("Manage review votes on Azure DevOps".into())
    }
    fn merge(&self, _: MergeMethod, _: bool) -> Result<(), String> {
        Err("Complete this PR on Azure DevOps; in-app completion is unavailable".into())
    }
    fn apply_suggestion(&self, _: &Suggestion) -> Result<(), String> {
        Err("Azure suggestion application is unavailable here".into())
    }
}
