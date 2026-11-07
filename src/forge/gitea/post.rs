//! Saying something on a pull request through the REST API: comments, reviews with their comments
//! on lines (pending ones too), replies, dismissals, suggested changes, merging.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

use super::details::parse_thread_id;
use super::{Api, Hosting};
use crate::forge::http::Failure;
use crate::pr_post::{
    LineComment, MergeMethod, Suggestion, Target, Verdict, apply_lines, check_path, decode_file,
    encode_path, may_delete_branch,
};
use crate::pull_request::{DiffSide, PrDetails, at};

/// A pull request on a Forgejo or Gitea to say something on.
pub(super) struct GiteaTarget {
    api: Api,
    name: &'static str,
    /// `owner/name`.
    path: String,
    number: u64,
    head_oid: String,
    head_branch: String,
    head_repo: Option<String>,
    pr_id: String,
    pending_review: Option<String>,
}

impl GiteaTarget {
    pub(super) fn of(
        api: Api,
        name: &'static str,
        hosting: &Hosting,
        details: &PrDetails,
    ) -> Option<GiteaTarget> {
        if !hosting.matches_pull_request(&details.url) {
            return None;
        }
        Some(GiteaTarget {
            api,
            name,
            path: hosting.path.clone(),
            number: details.number,
            head_oid: details.head_oid.clone(),
            head_branch: details.head_branch.clone(),
            head_repo: details.extras.head_repo.clone(),
            pr_id: details.extras.pr_id.clone(),
            pending_review: details.extras.pending_review.clone(),
        })
        .filter(|t| t.number > 0)
    }

    fn pulls(&self, rest: &str) -> String {
        format!("repos/{}/pulls/{}{rest}", self.path, self.number)
    }

    fn post(&self, endpoint: &str, payload: &Value) -> Result<Value, String> {
        Ok(self.api.send("POST", endpoint, Some(payload))?)
    }

    /// Whether what the forge holds for the viewer is known: both forges fold a pending review
    /// into any review sent, so sending without knowing of one would publish it.
    fn known(&self) -> Result<(), String> {
        if self.pr_id.is_empty() {
            Err("Couldn't read all of this pull request. Load it again.".to_owned())
        } else {
            Ok(())
        }
    }

    fn post_review(&self, payload: &Value) -> Result<(), String> {
        self.post(&self.pulls("/reviews"), payload).map(|_| ())
    }

    /// Comments on lines go into the pending review, which is started for them when there's none.
    ///
    /// The two forges differ here. Forgejo adds a comment to a review through the review's own
    /// endpoint, and won't start a pending review without words of its own (which sending the
    /// review replaces). Gitea has no such endpoint; posting another pending review adds to the
    /// one there is.
    fn add_to_pending(&self, comment: &LineComment) -> Result<(), String> {
        self.known()?;
        let comments = std::slice::from_ref(comment);
        if let Some(review) = &self.pending_review {
            // Forgejo adds to any review it's given, a sent one too: one sent from elsewhere since
            // the page was read would publish the comment.
            let state = self.api.get(&self.pulls(&format!("/reviews/{review}")))?;
            if at(&state, "/state") != "PENDING" {
                return Err(
                    "Your pending review was sent or discarded meanwhile. Load it again."
                        .to_owned(),
                );
            }
            let endpoint = self.pulls(&format!("/reviews/{review}/comments"));
            match self
                .api
                .send("POST", &endpoint, Some(&comment_payload(comment)))
            {
                Ok(_) => return Ok(()),
                // Gitea has no such endpoint; any other refusal is the forge's answer.
                Err(failure) if failure.status == Some(405) => {}
                Err(failure) => return Err(failure.message),
            }
        }
        let payload = review_payload(&self.head_oid, "PENDING", "", comments);
        match self
            .api
            .send("POST", &self.pulls("/reviews"), Some(&payload))
        {
            Ok(_) => Ok(()),
            Err(failure) if needs_words(&failure.message) => {
                let payload = review_payload(&self.head_oid, "PENDING", PENDING_WORDS, comments);
                self.post_review(&payload)
            }
            Err(failure) => Err(failure.message),
        }
    }
}

impl Target for GiteaTarget {
    fn comment(&self, body: &str) -> Result<(), String> {
        let endpoint = format!("repos/{}/issues/{}/comments", self.path, self.number);
        self.post(&endpoint, &json!({ "body": body })).map(|_| ())
    }

    fn review(&self, verdict: Verdict, body: &str) -> Result<(), String> {
        self.known()?;
        let event = event(verdict);
        match &self.pending_review {
            Some(review) => {
                let endpoint = self.pulls(&format!("/reviews/{review}"));
                let payload = json!({ "event": event, "body": body });
                match self.api.send("POST", &endpoint, Some(&payload)) {
                    Ok(_) => Ok(()),
                    // Forgejo counts only the words, not the comments on lines.
                    Err(failure) if body.trim().is_empty() && needs_words(&failure.message) => {
                        Err(format!(
                            "{} sends a review of comments only with a few words: write \
                             something, or approve",
                            self.name
                        ))
                    }
                    Err(failure) => Err(failure.message),
                }
            }
            None => self.post_review(&review_payload(&self.head_oid, event, body, &[])),
        }
    }

    fn comment_now(&self, comment: &LineComment) -> Result<(), String> {
        self.known()?;
        let payload = review_payload(
            &self.head_oid,
            event(Verdict::Comment),
            "",
            std::slice::from_ref(comment),
        );
        self.post_review(&payload)
    }

    fn add_pending(&self, comment: &LineComment) -> Result<(), String> {
        self.add_to_pending(comment)
    }

    fn remove_pending(&self, comment: &str, last: bool) -> Result<(), String> {
        if last {
            return self.discard_pending();
        }
        let review = self
            .pending_review
            .as_ref()
            .ok_or("There is no pending review")?;
        let endpoint = self.pulls(&format!("/reviews/{review}/comments/{comment}"));
        match self.api.send("DELETE", &endpoint, None) {
            Ok(_) => Ok(()),
            Err(failure) if matches!(failure.status, Some(404 | 405)) => Err(format!(
                "This {} can't take one comment out of a pending review: discard the review, or \
                 send it ({})",
                self.name, failure.message
            )),
            Err(failure) => Err(failure.message),
        }
    }

    fn discard_pending(&self) -> Result<(), String> {
        let review = self
            .pending_review
            .as_ref()
            .ok_or("There is no pending review")?;
        self.api
            .send("DELETE", &self.pulls(&format!("/reviews/{review}")), None)?;
        Ok(())
    }

    /// A comment on the same line of the same file is a reply: that's what makes a conversation
    /// here. With a review pending, it goes into it, as it would on the web page.
    fn reply(&self, thread: &str, body: &str) -> Result<(), String> {
        let (side, line, path) =
            parse_thread_id(thread).ok_or("This thread can't be answered from here")?;
        let comment = LineComment {
            path: path.to_owned(),
            line,
            side,
            body: body.to_owned(),
        };
        match self.pending_review {
            Some(_) => self.add_to_pending(&comment),
            None => self.comment_now(&comment),
        }
    }

    fn set_resolved(&self, _: &str, _: bool) -> Result<(), String> {
        Err(format!(
            "{}'s API can't resolve conversations: do it on the pull request's web page",
            self.name
        ))
    }

    fn dismiss(&self, review: &str, message: &str) -> Result<(), String> {
        self.post(
            &self.pulls(&format!("/reviews/{review}/dismissals")),
            &json!({ "message": message }),
        )
        .map(|_| ())
    }

    fn merge(&self, method: MergeMethod, delete_branch: bool) -> Result<(), String> {
        let payload = merge_payload(&self.head_oid, method);
        // While the server is still testing whether it merges (it does again after a push, a
        // review…), it answers "Please try again later": so it is, a few times. The merge names
        // its commit, so trying again never merges anything else.
        let mut tries = 0;
        loop {
            match self.api.send("POST", &self.pulls("/merge"), Some(&payload)) {
                Ok(_) => break,
                Err(failure) if still_checking(&failure) && tries < MERGE_TRIES => {
                    tries += 1;
                    std::thread::sleep(MERGE_RETRY_AFTER);
                }
                Err(failure) => return Err(failure.into()),
            }
        }
        if delete_branch {
            let endpoint = format!(
                "repos/{}/branches/{}",
                self.path,
                encode_path(&self.head_branch)
            );
            // Which commit it's on is checked first; a push can still come in between, just in
            // a much smaller window.
            let tip = match self.api.get(&endpoint) {
                Ok(branch) => branch_tip(&branch).map(str::to_owned),
                // Gone already: that's what deleting it wanted.
                Err(failure) if failure.status == Some(404) => return Ok(()),
                Err(_) => None,
            };
            may_delete_branch(tip.as_deref(), &self.head_oid)?;
            match self.api.send("DELETE", &endpoint, None) {
                // Gone already: that's what deleting it wanted.
                Ok(_) => {}
                Err(failure) if failure.status == Some(404) => {}
                Err(failure) => {
                    return Err(format!(
                        "It's merged, but the branch wasn't deleted: {}",
                        failure.message
                    ));
                }
            }
        }
        Ok(())
    }

    fn apply_suggestion(&self, suggestion: &Suggestion) -> Result<(), String> {
        check_path(&suggestion.path)?;
        let head = self
            .head_repo
            .as_ref()
            .ok_or("The repository of this branch is gone")?;
        let endpoint = format!("repos/{head}/contents/{}", encode_path(&suggestion.path));
        let file = self.api.get(&format!("{endpoint}?ref={}", self.head_oid))?;
        let content = decode_file(&file, self.name)?;
        let changed = apply_lines(&content, suggestion.start, suggestion.end, &suggestion.text)?;
        let payload = json!({
            "message": format!("Apply suggestion from @{}", suggestion.author),
            "content": BASE64.encode(changed),
            "sha": file.get("sha"),
            "branch": self.head_branch,
        });
        self.api.send("PUT", &endpoint, Some(&payload))?;
        Ok(())
    }
}

/// What a pending review says until it's sent, where the forge wants it to say something: sending
/// it replaces this with what the review says.
pub(super) const PENDING_WORDS: &str = "Review in progress";

/// Whether the forge refused something for having no words of its own.
fn needs_words(error: &str) -> bool {
    error.contains("requires a body")
}

/// A comment on a line: a removed line is on the old position, an added or unchanged one on the
/// new.
fn comment_payload(comment: &LineComment) -> Value {
    let position = match comment.side {
        DiffSide::Left => "old_position",
        DiffSide::Right => "new_position",
    };
    json!({ "path": comment.path, "body": comment.body, position: comment.line })
}

/// How the API names a verdict.
fn event(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Comment => "COMMENT",
        Verdict::Approve => "APPROVED",
        Verdict::RequestChanges => "REQUEST_CHANGES",
    }
}

/// What `POST …/pulls/N/reviews` is sent.
pub(super) fn review_payload(
    head_oid: &str,
    event: &str,
    body: &str,
    comments: &[LineComment],
) -> Value {
    let mut payload = json!({ "commit_id": head_oid, "event": event, "body": body });
    if !comments.is_empty() {
        payload["comments"] = Value::Array(comments.iter().map(comment_payload).collect());
    }
    payload
}

/// What `POST …/pulls/N/merge` is sent. The commit is named, so that nothing pushed since the page
/// was read gets merged unseen; the branch is deleted apart, once that's known to be safe.
/// How often a merge is tried again while the server is still testing it, and how long apart.
const MERGE_TRIES: u32 = 10;
const MERGE_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(1);

/// Whether the server refused a merge only because it is still testing whether it merges.
fn still_checking(failure: &Failure) -> bool {
    failure.status == Some(405) && failure.message.to_lowercase().contains("try again later")
}

pub(super) fn merge_payload(head_oid: &str, method: MergeMethod) -> Value {
    let method = match method {
        MergeMethod::Merge => "merge",
        MergeMethod::Squash => "squash",
        MergeMethod::Rebase => "rebase",
    };
    json!({ "Do": method, "head_commit_id": head_oid, "delete_branch_after_merge": false })
}

/// The commit a branch is on, from what the API answers when asked for it.
fn branch_tip(branch: &Value) -> Option<&str> {
    Some(at(branch, "/commit/id")).filter(|tip| !tip.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review13_gitea_targets_reject_another_web_instance() {
        for forge in [crate::forge::Forge::Gitea, crate::forge::Forge::Forgejo] {
            for prefix in ["", "/reviews"] {
                let h = Hosting {
                    forge,
                    host: "forge.example".into(),
                    path: "Team/Repo".into(),
                    web: format!("https://forge.example:4000{prefix}"),
                };
                for (base, path, expected) in [
                    ("https://forge.example:3000", "Team/Repo", false),
                    ("https://forge.example:4000", "Team/Repo", prefix.is_empty()),
                    (
                        "https://forge.example:4000/reviews",
                        "team/repo",
                        !prefix.is_empty(),
                    ),
                    ("https://forge.example:4000/REVIEWS", "Team/Repo", false),
                ] {
                    let details = PrDetails {
                        number: 7,
                        url: format!("{base}/{path}/pulls/7"),
                        ..Default::default()
                    };
                    let api = Api::new("Gitea", &h.web, &h.host, None);
                    let target = GiteaTarget::of(api, "Gitea", &h, &details);
                    assert_eq!(target.is_some(), expected, "{h:?} {}", details.url);
                    if let Some(target) = target {
                        assert!(
                            target.path.eq_ignore_ascii_case(&h.path),
                            "proxy prefix must stay in API base only"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn provider_boundary_stale_details_cannot_retarget_gitea_or_forgejo() {
        for forge in [crate::forge::Forge::Gitea, crate::forge::Forge::Forgejo] {
            let h = Hosting {
                forge,
                host: "forge.test".into(),
                path: "fork/project".into(),
                web: "https://forge.test".into(),
            };
            for (url, expected) in [
                ("https://forge.test/fork/project/pulls/7", true),
                ("https://forge.test/upstream/project/pulls/7", false),
                ("https://other.test/fork/project/pulls/7", false),
            ] {
                let details = PrDetails {
                    number: 7,
                    url: url.into(),
                    ..Default::default()
                };
                let api = Api::new("Gitea", &h.web, &h.host, None);
                assert_eq!(
                    GiteaTarget::of(api, "Gitea", &h, &details).is_some(),
                    expected
                );
            }
        }
    }

    /// Only "still testing it" is waited out; any other refusal of a merge is said at once.
    #[test]
    fn a_merge_is_tried_again_only_while_the_server_tests_it() {
        let failure = |status, message: &str| Failure {
            status,
            message: message.to_owned(),
        };
        assert!(still_checking(&failure(
            Some(405),
            "Gitea: Please try again later (HTTP 405)"
        )));
        assert!(!still_checking(&failure(Some(405), "Not allowed to merge")));
        assert!(!still_checking(&failure(
            Some(409),
            "Please try again later"
        )));
        assert!(!still_checking(&failure(None, "try again later")));
    }

    fn line(path: &str, line: u64, side: DiffSide, body: &str) -> LineComment {
        LineComment {
            path: path.to_owned(),
            line,
            side,
            body: body.to_owned(),
        }
    }

    #[test]
    fn a_review_names_its_verdict_and_commit() {
        for (verdict, name) in [
            (Verdict::Comment, "COMMENT"),
            (Verdict::Approve, "APPROVED"),
            (Verdict::RequestChanges, "REQUEST_CHANGES"),
        ] {
            let payload = review_payload("abc", event(verdict), "Text", &[]);
            assert_eq!(payload["event"], name);
            assert_eq!(payload["commit_id"], "abc");
            assert_eq!(payload["body"], "Text");
            assert!(payload.get("comments").is_none());
        }
    }

    #[test]
    fn comments_on_lines_go_on_their_side() {
        let comments = [
            line("src/a.rs", 12, DiffSide::Right, "New line"),
            line("src/b.rs", 3, DiffSide::Left, "Removed line"),
        ];
        let payload = review_payload("abc", "PENDING", "", &comments);
        assert_eq!(
            payload["comments"],
            json!([
                {"path": "src/a.rs", "new_position": 12, "body": "New line"},
                {"path": "src/b.rs", "old_position": 3, "body": "Removed line"},
            ])
        );
    }

    #[test]
    fn a_merge_names_its_method_and_commit() {
        for (method, name) in [
            (MergeMethod::Merge, "merge"),
            (MergeMethod::Squash, "squash"),
            (MergeMethod::Rebase, "rebase"),
        ] {
            let payload = merge_payload("abc", method);
            assert_eq!(payload["Do"], name);
            assert_eq!(payload["head_commit_id"], "abc");
            assert_eq!(payload["delete_branch_after_merge"], false);
        }
    }

    #[test]
    fn knows_a_refusal_for_want_of_words() {
        assert!(needs_words(
            "Forgejo: review event PENDING requires a body (HTTP 422)"
        ));
        assert!(needs_words(
            "Forgejo: review event COMMENT requires a body or a comment (HTTP 422)"
        ));
        assert!(!needs_words("Forgejo: not found (HTTP 404)"));
    }

    #[test]
    fn reads_which_commit_a_branch_is_on() {
        let branch = json!({"name": "b", "commit": {"id": "abc123"}});
        assert_eq!(branch_tip(&branch), Some("abc123"));
        assert_eq!(branch_tip(&json!({"message": "not found"})), None);
    }

    fn target(pending: Option<&str>) -> GiteaTarget {
        GiteaTarget {
            api: Api::new("Forgejo", "https://127.0.0.1:9", "127.0.0.1", None),
            name: "Forgejo",
            path: "o/r".to_owned(),
            number: 1,
            head_oid: "h".to_owned(),
            head_branch: "b".to_owned(),
            head_repo: None,
            pr_id: String::new(),
            pending_review: pending.map(str::to_owned),
        }
    }

    #[test]
    fn nothing_goes_out_without_knowing_the_pull_request() {
        let error = target(None).add_pending(&line("a", 1, DiffSide::Right, "x"));
        assert!(error.is_err_and(|e| e.contains("Load it again")));
    }

    #[test]
    fn nothing_folds_a_pending_review_in_unknowingly() {
        let t = target(None);
        assert!(t.comment_now(&line("a", 1, DiffSide::Right, "x")).is_err());
        assert!(t.review(Verdict::Comment, "x").is_err());
        assert!(t.reply("R1:a", "x").is_err());
    }

    #[test]
    fn conversations_cant_be_resolved_through_the_api() {
        assert!(target(None).set_resolved("R1:a", true).is_err());
    }

    #[test]
    fn a_thread_from_elsewhere_cant_be_answered() {
        let error = target(None).reply("PRRT_1", "x");
        assert!(error.is_err_and(|e| e.contains("can't be answered")));
    }

    #[test]
    fn nothing_is_pending_to_discard() {
        assert!(target(None).discard_pending().is_err());
        assert!(target(None).remove_pending("1", false).is_err());
    }
}
