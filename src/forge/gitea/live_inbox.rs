//! The inbox against real servers (see `live.rs` and `ci/forge-server.sh`): a pull request by
//! the author asks the reviewer's review, its head fails a check and the reviewer comments on it.
//! The author's inbox has it as theirs with failing checks and one new comment; the reviewer's
//! has it as asking their review.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::live::{Server, run, scratch, write};
use super::{FORGEJO, GITEA, GiteaFamily, create, inbox};
use crate::forge::inbox::{Checks, Reason, needs_you, seen_now};
use crate::forge::{Hosting, NewPullRequest};

#[test]
#[ignore = "needs a Forgejo: see ci/forge-server.sh"]
fn live_inbox_forgejo() -> Result<(), String> {
    inbox_life(&FORGEJO)
}

#[test]
#[ignore = "needs a Gitea: see ci/forge-server.sh"]
fn live_inbox_gitea() -> Result<(), String> {
    inbox_life(&GITEA)
}

fn inbox_life(family: &'static GiteaFamily) -> Result<(), String> {
    let server = Server::from_env(family)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let name = format!("gitcraft-inbox-{stamp}");
    let path = format!("author/{name}");
    server.author().send(
        "POST",
        "user/repos",
        Some(&json!({ "name": name, "default_branch": "main", "private": false })),
    )?;
    server.author().send(
        "PUT",
        &format!("repos/{path}/collaborators/reviewer"),
        Some(&json!({ "permission": "write" })),
    )?;
    let dir = scratch(&format!(
        "gitcraft-live-inbox-{}-{stamp}",
        family.forge.key()
    ))?;
    let remote = server
        .url
        .replacen("://", &format!("://author:{}@", server.author_token), 1)
        + &format!("/{path}.git");
    run(&dir, &["init", "-q", "-b", "main"])?;
    for (k, v) in [
        ("user.name", "Author"),
        ("user.email", "author@example.org"),
        ("commit.gpgsign", "false"),
    ] {
        run(&dir, &["config", k, v])?;
    }
    write(&dir, "a.txt", "one\n")?;
    run(&dir, &["add", "."])?;
    run(&dir, &["commit", "-q", "-m", "first"])?;
    run(&dir, &["remote", "add", "origin", &remote])?;
    run(&dir, &["push", "-q", "-u", "origin", "main"])?;
    run(&dir, &["checkout", "-q", "-b", "feat"])?;
    write(&dir, "a.txt", "two ✓\n")?;
    run(&dir, &["commit", "-q", "-am", "feat: ünïcode"])?;
    run(&dir, &["push", "-q", "origin", "feat"])?;
    let head = run(&dir, &["rev-parse", "feat"])?.trim().to_owned();

    let hosting = Hosting {
        forge: family.forge,
        host: "127.0.0.1".to_owned(),
        path: path.clone(),
        web: server.url.clone(),
    };
    let new = NewPullRequest {
        branch: "feat",
        base: "main",
        title: "Change ünïcode",
        body: "",
        draft: false,
    };
    create(&server.author(), &hosting, &new)?;
    server.author().send(
        "POST",
        &format!("repos/{path}/pulls/1/requested_reviewers"),
        Some(&json!({ "reviewers": ["reviewer"] })),
    )?;
    server.author().send(
        "POST",
        &format!("repos/{path}/statuses/{head}"),
        Some(&json!({ "state": "failure", "context": "ci/test", "description": "red" })),
    )?;
    server.reviewer().send(
        "POST",
        &format!("repos/{path}/issues/1/comments"),
        Some(&json!({ "body": "Why?" })),
    )?;
    // The author's own comment isn't news to them.
    server.author().send(
        "POST",
        &format!("repos/{path}/issues/1/comments"),
        Some(&json!({ "body": "Because." })),
    )?;

    let theirs = inbox::inbox(&server.author(), &hosting)?;
    let pr = theirs
        .iter()
        .find(|p| p.number == 1)
        .ok_or_else(|| format!("the author's inbox lacks it: {theirs:?}"))?;
    if !(pr.mine && !pr.review_requested && pr.checks == Checks::Failing) {
        return Err(format!("the author's inbox read {pr:?}"));
    }
    let reasons = needs_you(pr, None);
    if reasons != [Reason::ChecksFailing, Reason::NewComments(1)] {
        return Err(format!("the author's reasons: {reasons:?}"));
    }
    // Opened from the inbox: the comment is seen; the failing checks still need them.
    let seen = seen_now(pr, crate::git::now() + 1);
    if needs_you(pr, Some(seen)) != [Reason::ChecksFailing] {
        return Err("a comment seen stays new".to_owned());
    }

    let asked = inbox::inbox(&server.reviewer(), &hosting)?;
    let pr = asked
        .iter()
        .find(|p| p.number == 1)
        .ok_or_else(|| format!("the reviewer's inbox lacks it: {asked:?}"))?;
    if needs_you(pr, None) != [Reason::ReviewRequested] {
        return Err(format!("the reviewer's inbox read {pr:?}"));
    }
    // Without a login, it says how to get one rather than listing nothing.
    let anonymous = super::Api::new(family.name, &server.url, "127.0.0.1", None);
    let err = inbox::inbox(&anonymous, &hosting).err().unwrap_or_default();
    if !err.contains("login") {
        return Err(format!("without a login: {err}"));
    }
    Ok(())
}
