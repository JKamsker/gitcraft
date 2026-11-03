//! Saying something on a pull request through `gh api`: comments and reviews, with their
//! comments on lines. The text goes over stdin, so its length and contents don't matter to the
//! command line.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

use super::gh_with_input;
use crate::pr_post::{
    LineComment, MergeMethod, Suggestion, Target, Verdict, apply_lines, decode_file, encode_path,
    may_delete_branch,
};
use crate::pull_request::{DiffSide, PrDetails, repo_of_url};

/// What `POST …/pulls/N/reviews` is sent.
fn review_payload(head_oid: &str, verdict: Verdict, body: &str, pending: &[LineComment]) -> Value {
    let event = match verdict {
        Verdict::Comment => "COMMENT",
        Verdict::Approve => "APPROVE",
        Verdict::RequestChanges => "REQUEST_CHANGES",
    };
    let mut payload = json!({ "commit_id": head_oid, "event": event, "body": body });
    if !pending.is_empty() {
        let comments: Vec<Value> = pending
            .iter()
            .map(|comment| {
                json!({
                    "path": comment.path,
                    "line": comment.line,
                    "side": side_name(comment.side),
                    "body": comment.body,
                })
            })
            .collect();
        payload["comments"] = Value::Array(comments);
    }
    payload
}

/// What `PUT …/pulls/N/merge` is sent. The commit is named, so that nothing pushed since the page
/// was read gets merged unseen.
fn merge_payload(head_oid: &str, method: MergeMethod) -> Value {
    let method = match method {
        MergeMethod::Merge => "merge",
        MergeMethod::Squash => "squash",
        MergeMethod::Rebase => "rebase",
    };
    json!({ "sha": head_oid, "merge_method": method })
}

/// Whether a failed deletion only says the branch is gone already.
fn branch_was_gone(error: &str) -> bool {
    error.contains("Reference does not exist")
}

/// Whether asking for a ref failed for want of it.
fn ref_not_found(error: &str) -> bool {
    error.contains("(HTTP 404)")
}

/// The commit a branch is on, from what GitHub answers when asked for its ref.
fn ref_tip(answer: &str) -> Option<String> {
    let v: Value = serde_json::from_str(answer).ok()?;
    v.pointer("/object/sha")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// A pull request on GitHub to say something on.
pub(super) struct GitHubTarget {
    repo: PathBuf,
    host: String,
    /// `owner/name`.
    path: String,
    number: u64,
    head_oid: String,
    head_branch: String,
    head_repo: Option<String>,
    pr_id: String,
    pending_review: Option<String>,
}

impl GitHubTarget {
    /// The pull request `details` of the repository at `repo`.
    pub(super) fn of(repo: &Path, details: &PrDetails) -> Option<GitHubTarget> {
        let (host, path) = repo_of_url(&details.url)?;
        Some(GitHubTarget {
            repo: repo.to_owned(),
            host,
            path,
            number: details.number,
            head_oid: details.head_oid.clone(),
            head_branch: details.head_branch.clone(),
            head_repo: details.extras.head_repo.clone(),
            pr_id: details.extras.pr_id.clone(),
            pending_review: details.extras.pending_review.clone(),
        })
    }

    fn api(&self, method: &str, endpoint: &str, payload: Option<&Value>) -> Result<String, String> {
        let mut args = vec!["api", "-X", method, endpoint];
        if payload.is_some() {
            args.extend(["--input", "-"]);
        }
        if self.host != "github.com" {
            args.extend(["--hostname", &self.host]);
        }
        let input = payload.map(Value::to_string);
        gh_with_input(&self.repo, &args, input.as_deref())
    }

    /// Runs a mutation taking one `input`, and returns what GitHub answers.
    fn mutate(&self, name: &str, input: Value, answer: &str) -> Result<Value, String> {
        let upper = format!("{}{}Input", name[..1].to_uppercase(), &name[1..]);
        let query =
            format!("mutation($input: {upper}!) {{ {name}(input: $input) {{ {answer} }} }}");
        let payload = json!({ "query": query, "variables": { "input": input } });
        let mut args = vec!["api", "graphql", "--input", "-"];
        if self.host != "github.com" {
            args.extend(["--hostname", &self.host]);
        }
        let out = gh_with_input(&self.repo, &args, Some(&payload.to_string()))?;
        serde_json::from_str(&out).map_err(|e| format!("unexpected output of gh: {e}"))
    }

    fn post_review(&self, payload: &Value) -> Result<(), String> {
        let endpoint = format!("repos/{}/pulls/{}/reviews", self.path, self.number);
        self.api("POST", &endpoint, Some(payload)).map(|_| ())
    }
}

impl Target for GitHubTarget {
    fn path(&self) -> &str {
        &self.path
    }

    /// Adds a comment to the conversation.
    fn comment(&self, body: &str) -> Result<(), String> {
        let endpoint = format!("repos/{}/issues/{}/comments", self.path, self.number);
        self.api("POST", &endpoint, Some(&json!({ "body": body })))
            .map(|_| ())
    }

    /// Sends the review: the pending one if there is one, with the comments in it.
    fn review(&self, verdict: Verdict, body: &str) -> Result<(), String> {
        if let Some(review) = &self.pending_review {
            let event = match verdict {
                Verdict::Comment => "COMMENT",
                Verdict::Approve => "APPROVE",
                Verdict::RequestChanges => "REQUEST_CHANGES",
            };
            let input = json!({ "pullRequestReviewId": review, "event": event, "body": body });
            return self
                .mutate("submitPullRequestReview", input, "clientMutationId")
                .map(|_| ());
        }
        let payload = review_payload(&self.head_oid, verdict, body, &[]);
        self.post_review(&payload)
    }

    /// Sends a comment on a line right away, as a review of its own.
    fn comment_now(&self, comment: &LineComment) -> Result<(), String> {
        let payload = review_payload(
            &self.head_oid,
            Verdict::Comment,
            "",
            std::slice::from_ref(comment),
        );
        self.post_review(&payload)
    }

    /// Adds a comment on a line to the pending review, starting one if there is none.
    fn add_pending(&self, comment: &LineComment) -> Result<(), String> {
        if self.pr_id.is_empty() {
            return Err("Couldn't read all of this pull request. Load it again.".to_owned());
        }
        let review = match &self.pending_review {
            Some(review) => review.clone(),
            None => {
                let answer = self.mutate(
                    "addPullRequestReview",
                    json!({ "pullRequestId": self.pr_id }),
                    "pullRequestReview { id }",
                )?;
                answer
                    .pointer("/data/addPullRequestReview/pullRequestReview/id")
                    .and_then(Value::as_str)
                    .ok_or("GitHub didn't start a review")?
                    .to_owned()
            }
        };
        let input = json!({
            "pullRequestReviewId": review,
            "path": comment.path,
            "line": comment.line,
            "side": side_name(comment.side),
            "body": comment.body,
        });
        let added = self
            .mutate("addPullRequestReviewThread", input, "clientMutationId")
            .map(|_| ());
        if added.is_err() && self.pending_review.is_none() {
            // A review started for this and left empty would only stand in the way of the next try.
            let _ = self.mutate(
                "deletePullRequestReview",
                json!({ "pullRequestReviewId": review }),
                "clientMutationId",
            );
        }
        added
    }

    /// Takes a comment out of the pending review; the review goes with its `last` comment.
    fn remove_pending(&self, comment: &str, last: bool) -> Result<(), String> {
        if last {
            return self.discard_pending();
        }
        self.mutate(
            "deletePullRequestReviewComment",
            json!({ "id": comment }),
            "clientMutationId",
        )
        .map(|_| ())
    }

    /// Throws the pending review away.
    fn discard_pending(&self) -> Result<(), String> {
        let review = self
            .pending_review
            .as_ref()
            .ok_or("There is no pending review")?;
        self.mutate(
            "deletePullRequestReview",
            json!({ "pullRequestReviewId": review }),
            "clientMutationId",
        )
        .map(|_| ())
    }

    fn reply(&self, thread: &str, body: &str) -> Result<(), String> {
        let mut input = json!({ "pullRequestReviewThreadId": thread, "body": body });
        if let Some(review) = &self.pending_review {
            input["pullRequestReviewId"] = json!(review);
        }
        self.mutate("addPullRequestReviewThreadReply", input, "clientMutationId")
            .map(|_| ())
    }

    fn set_resolved(&self, thread: &str, resolved: bool) -> Result<(), String> {
        let name = if resolved {
            "resolveReviewThread"
        } else {
            "unresolveReviewThread"
        };
        self.mutate(name, json!({ "threadId": thread }), "clientMutationId")
            .map(|_| ())
    }

    fn dismiss(&self, review: &str, message: &str) -> Result<(), String> {
        self.mutate(
            "dismissPullRequestReview",
            json!({ "pullRequestReviewId": review, "message": message }),
            "clientMutationId",
        )
        .map(|_| ())
    }

    /// Merges the pull request, and deletes its branch if asked to.
    fn merge(&self, method: MergeMethod, delete_branch: bool) -> Result<(), String> {
        let endpoint = format!("repos/{}/pulls/{}/merge", self.path, self.number);
        self.api(
            "PUT",
            &endpoint,
            Some(&merge_payload(&self.head_oid, method)),
        )?;
        if delete_branch {
            let branch = encode_path(&self.head_branch);
            // GitHub deletes without asking which commit it's on, so this is checked first; a
            // push can still come in between, just in a much smaller window.
            let tip = match self.api(
                "GET",
                &format!("repos/{}/git/ref/heads/{branch}", self.path),
                None,
            ) {
                Ok(answer) => ref_tip(&answer),
                // Gone already: that's what deleting it wanted.
                Err(e) if ref_not_found(&e) => return Ok(()),
                Err(_) => None,
            };
            may_delete_branch(tip.as_deref(), &self.head_oid)?;
            let endpoint = format!("repos/{}/git/refs/heads/{branch}", self.path);
            if let Err(e) = self.api("DELETE", &endpoint, None)
                && !branch_was_gone(&e)
            {
                return Err(format!("It's merged, but the branch wasn't deleted: {e}"));
            }
        }
        Ok(())
    }

    /// Commits a suggested change to the pull request's branch. The file is read as it is in the
    /// commit that was reviewed, so if it has changed since, GitHub refuses instead of the change
    /// landing on the wrong lines.
    fn apply_suggestion(&self, suggestion: &Suggestion) -> Result<(), String> {
        let head = self
            .head_repo
            .as_ref()
            .ok_or("The repository of this branch is gone")?;
        let endpoint = format!("repos/{head}/contents/{}", encode_path(&suggestion.path));
        let file = self.api("GET", &format!("{endpoint}?ref={}", self.head_oid), None)?;
        let file: Value =
            serde_json::from_str(&file).map_err(|e| format!("unexpected output of gh: {e}"))?;
        let content = decode_file(&file, "GitHub")?;
        let changed = apply_lines(&content, suggestion.start, suggestion.end, &suggestion.text)?;
        let payload = json!({
            "message": format!("Apply suggestion from @{}", suggestion.author),
            "content": BASE64.encode(changed),
            "sha": file.get("sha"),
            "branch": self.head_branch,
        });
        self.api("PUT", &endpoint, Some(&payload)).map(|_| ())
    }
}

fn side_name(side: DiffSide) -> &'static str {
    match side {
        DiffSide::Left => "LEFT",
        DiffSide::Right => "RIGHT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        for (verdict, event) in [
            (Verdict::Comment, "COMMENT"),
            (Verdict::Approve, "APPROVE"),
            (Verdict::RequestChanges, "REQUEST_CHANGES"),
        ] {
            let payload = review_payload("abc", verdict, "Text", &[]);
            assert_eq!(payload["event"], event);
            assert_eq!(payload["commit_id"], "abc");
            assert_eq!(payload["body"], "Text");
        }
    }

    #[test]
    fn a_review_without_comments_on_lines_sends_none() {
        let payload = review_payload("abc", Verdict::Approve, "", &[]);
        assert!(payload.get("comments").is_none());
    }

    #[test]
    fn comments_on_lines_go_along_with_their_side() {
        let pending = [
            line("src/a.rs", 12, DiffSide::Right, "New line"),
            line("src/b.rs", 3, DiffSide::Left, "Removed line"),
        ];
        let payload = review_payload("abc", Verdict::Comment, "", &pending);
        assert_eq!(
            payload["comments"],
            json!([
                {"path": "src/a.rs", "line": 12, "side": "RIGHT", "body": "New line"},
                {"path": "src/b.rs", "line": 3, "side": "LEFT", "body": "Removed line"},
            ])
        );
    }

    #[test]
    fn text_is_sent_as_it_is_whatever_it_contains() {
        let body = "\"quotes\" \\ and\nlines --flag $(x)";
        let payload = review_payload("abc", Verdict::Comment, body, &[]);
        let sent: Value = serde_json::from_str(&payload.to_string()).unwrap_or_default();
        assert_eq!(sent["body"], body);
    }

    #[test]
    fn a_merge_names_its_method_and_commit() {
        for (method, name) in [
            (MergeMethod::Merge, "merge"),
            (MergeMethod::Squash, "squash"),
            (MergeMethod::Rebase, "rebase"),
        ] {
            let payload = merge_payload("abc", method);
            assert_eq!(payload["merge_method"], name);
            assert_eq!(payload["sha"], "abc");
        }
    }

    #[test]
    fn a_branch_gone_already_is_what_deleting_it_wanted() {
        assert!(branch_was_gone("gh: Reference does not exist (HTTP 422)"));
        assert!(!branch_was_gone("gh: Must have admin rights (HTTP 403)"));
    }

    #[test]
    fn a_ref_that_isnt_there_is_told_from_other_failures() {
        assert!(ref_not_found("gh: Not Found (HTTP 404)"));
        assert!(!ref_not_found("gh: Bad credentials (HTTP 401)"));
        assert!(!ref_not_found("error connecting to api.github.com"));
    }

    #[test]
    fn reads_which_commit_a_branch_is_on() {
        let answer = r#"{"ref":"refs/heads/b","object":{"sha":"abc123","type":"commit"}}"#;
        assert_eq!(ref_tip(answer).as_deref(), Some("abc123"));
        assert_eq!(ref_tip(r#"{"message":"Not Found"}"#), None);
        assert_eq!(ref_tip("not json"), None);
    }

    fn target_without_extras() -> GitHubTarget {
        GitHubTarget {
            repo: PathBuf::new(),
            host: "github.com".to_owned(),
            path: "o/r".to_owned(),
            number: 1,
            head_oid: "h".to_owned(),
            head_branch: "b".to_owned(),
            head_repo: None,
            pr_id: String::new(),
            pending_review: None,
        }
    }

    #[test]
    fn nothing_goes_out_without_knowing_the_pull_request() {
        let error = target_without_extras().add_pending(&line("a", 1, DiffSide::Right, "x"));
        assert!(error.is_err_and(|e| e.contains("Load it again")));
    }
}
