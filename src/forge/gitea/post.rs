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
            Err(crate::i18n::text(
                "backend-couldn-t-read-all-of-this-pull-request-load-it-again-250862",
            )
            .to_owned())
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
                    crate::i18n::text("backend-your-pending-review-was-sent-or-discarded-meanwhile-load-it-again-9836ae")
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
                        Err(crate::i18n::message(
                            "backend-sends-a-review-of-comments-only-with-a-few-words-write-something-dcd801",
                            &[("name", (self.name).to_string().into())],
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
        let review = self.pending_review.as_ref().ok_or(crate::i18n::text(
            "backend-there-is-no-pending-review-1a6148",
        ))?;
        let endpoint = self.pulls(&format!("/reviews/{review}/comments/{comment}"));
        match self.api.send("DELETE", &endpoint, None) {
            Ok(_) => Ok(()),
            Err(failure) if matches!(failure.status, Some(404 | 405)) => Err(crate::i18n::message(
                "backend-this-can-t-take-one-comment-out-of-a-pending-review-discard-the-r-fe31dd",
                &[
                    ("name", (self.name).to_string().into()),
                    ("message", (failure.message).to_string().into()),
                ],
            )),
            Err(failure) => Err(failure.message),
        }
    }

    fn discard_pending(&self) -> Result<(), String> {
        let review = self.pending_review.as_ref().ok_or(crate::i18n::text(
            "backend-there-is-no-pending-review-1a6148",
        ))?;
        self.api
            .send("DELETE", &self.pulls(&format!("/reviews/{review}")), None)?;
        Ok(())
    }

    /// A comment on the same line of the same file is a reply: that's what makes a conversation
    /// here. With a review pending, it goes into it, as it would on the web page.
    fn reply(&self, thread: &str, body: &str) -> Result<(), String> {
        let (side, line, path) = parse_thread_id(thread).ok_or(crate::i18n::text(
            "backend-this-thread-can-t-be-answered-from-here-c6d211",
        ))?;
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
        Err(crate::i18n::message(
            "backend-s-api-can-t-resolve-conversations-do-it-on-the-pull-request-s-web-d6a191",
            &[("name", (self.name).to_string().into())],
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
                    return Err(crate::i18n::message(
                        "backend-it-s-merged-but-the-branch-wasn-t-deleted-7758b4",
                        &[("message", (failure.message).to_string().into())],
                    ));
                }
            }
        }
        Ok(())
    }

    fn apply_suggestion(&self, suggestion: &Suggestion) -> Result<(), String> {
        check_path(&suggestion.path)?;
        let head = self.head_repo.as_ref().ok_or(crate::i18n::text(
            "backend-the-repository-of-this-branch-is-gone-ce162e",
        ))?;
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
mod tests;
