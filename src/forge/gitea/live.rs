//! The whole of a pull request's life against real servers: a Forgejo and a Gitea started by
//! `ci/forge-server.sh`, which says how to run these. They're ignored by a plain `cargo test`:
//!
//! ```sh
//! ci/forge-server.sh forgejo 3000 forges.env && ci/forge-server.sh gitea 3001 forges.env
//! set -a; . ./forges.env; set +a
//! cargo test live -- --ignored
//! ```
//!
//! Each run makes a new repository on the server, so a server can be used again.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::details::{self, thread_id};
use super::post::GiteaTarget;
use super::{
    Api, FORGEJO, GITEA, GiteaFamily, Login, create, create_release, list, retarget, upload_assets,
};
use crate::forge::{self, Forge, Found, Hosting, NewPullRequest, Provider};
use crate::git;
use crate::pr_post::{self, LineComment, MergeMethod, Suggestion, Target, Verdict};
use crate::pull_request::{self, DiffSide, EntryKind, PrDetails, at};

macro_rules! ensure {
    ($cond:expr, $($why:tt)+) => {
        if !$cond {
            return Err(format!($($why)+));
        }
    };
}

#[test]
#[ignore = "needs a Forgejo: see ci/forge-server.sh"]
fn live_forgejo() -> Result<(), String> {
    whole_life(&FORGEJO)
}

#[test]
#[ignore = "needs a Gitea: see ci/forge-server.sh"]
fn live_gitea() -> Result<(), String> {
    whole_life(&GITEA)
}

/// A server and its two users.
pub(super) struct Server {
    family: &'static GiteaFamily,
    /// `http://127.0.0.1:3000`.
    pub(super) url: String,
    pub(super) author_token: String,
    reviewer_token: String,
}

impl Server {
    pub(super) fn from_env(family: &'static GiteaFamily) -> Result<Server, String> {
        let upper = family.name.to_uppercase();
        let var = |name: &str| {
            let key = format!("GITCRAFT_{upper}_{name}");
            std::env::var(&key).map_err(|_| {
                format!("{key} isn't set: start a server with ci/forge-server.sh (see its header)")
            })
        };
        Ok(Server {
            family,
            url: var("URL")?.trim_end_matches('/').to_owned(),
            author_token: var("AUTHOR_TOKEN")?,
            reviewer_token: var("REVIEWER_TOKEN")?,
        })
    }

    fn api(&self, token: Option<&str>) -> Api {
        let login = token.map(|t| Login::Token(t.to_owned()));
        Api::new(self.family.name, &self.url, "127.0.0.1", login)
    }

    pub(super) fn author(&self) -> Api {
        self.api(Some(&self.author_token))
    }

    pub(super) fn reviewer(&self) -> Api {
        self.api(Some(&self.reviewer_token))
    }
}

/// Polls `f` until it has something, for what the server works out in the background.
fn eventually<T>(
    what: &str,
    mut f: impl FnMut() -> Result<Option<T>, String>,
) -> Result<T, String> {
    let start = Instant::now();
    loop {
        if let Some(found) = f()? {
            return Ok(found);
        }
        if start.elapsed() > Duration::from_secs(60) {
            return Err(format!("timed out waiting for {what}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub(super) fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map_err(|e| format!("git {}: {e}", args.join(" ")))
}

pub(super) fn write(dir: &Path, file: &str, text: &str) -> Result<(), String> {
    std::fs::write(dir.join(file), text).map_err(|e| e.to_string())
}

pub(super) fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn whole_life(family: &'static GiteaFamily) -> Result<(), String> {
    let server = Server::from_env(family)?;
    let key = family.forge.key();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let name = format!("gitcraft-{stamp}");
    let path = format!("author/{name}");

    // The repository, with the reviewer allowed to push.
    eprintln!(
        "{}: The repository, with the reviewer allowed to push",
        family.name
    );
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

    // A clone with a branch to propose.
    eprintln!("{}: A clone with a branch to propose", family.name);
    let dir = scratch(&format!("gitcraft-live-{key}-{stamp}"))?;
    let remote = server
        .url
        .replacen("://", &format!("://author:{}@", server.author_token), 1)
        + &format!("/{path}.git");
    run(&dir, &["init", "-q", "-b", "main"])?;
    for (k, v) in [
        ("user.name", "Author"),
        ("user.email", "author@example.org"),
        ("commit.gpgsign", "false"),
        ("gitcraft.forge", key),
    ] {
        run(&dir, &["config", k, v])?;
    }
    write(&dir, "a.txt", "one\ntwo\nthree\nfour\nfive\n")?;
    run(&dir, &["add", "."])?;
    run(&dir, &["commit", "-q", "-m", "first"])?;
    run(&dir, &["remote", "add", "origin", &remote])?;
    run(&dir, &["push", "-q", "-u", "origin", "main"])?;
    run(&dir, &["checkout", "-q", "-b", "feat"])?;
    write(&dir, "a.txt", "one\nTWO\nthree\nfour\nfive\nsix\n")?;
    write(&dir, "b.txt", "new file\n")?;
    run(&dir, &["add", "."])?;
    run(&dir, &["commit", "-q", "-m", "feat: change things"])?;
    run(&dir, &["push", "-q", "origin", "feat"])?;
    let head = run(&dir, &["rev-parse", "feat"])?.trim().to_owned();
    run(&dir, &["checkout", "-q", "-b", "wip", "main"])?;
    write(&dir, "c.txt", "later\n")?;
    run(&dir, &["add", "."])?;
    run(&dir, &["commit", "-q", "-m", "wip"])?;
    run(&dir, &["push", "-q", "origin", "wip"])?;

    // git's config names the forge of a server whose name doesn't.
    eprintln!(
        "{}: git's config names the forge of a server whose name doesn't",
        family.name
    );
    let remotes = git::remotes(&dir)?;
    let hosting = forge::detect(&dir, &remotes).ok_or("the forge wasn't detected")?;
    ensure!(
        hosting
            == Hosting {
                forge: family.forge,
                host: "127.0.0.1".to_owned(),
                path: path.clone(),
                web: server.url.clone(),
            },
        "detected {hosting:?}"
    );

    // Opening pull requests, and the list.
    eprintln!("{}: Opening pull requests, and the list", family.name);
    let asked = ["reviewer".to_owned()];
    let new = |branch, title, draft, reviewers| NewPullRequest {
        branch,
        base: "main",
        title,
        body: "Does things.\n\n![upload](/attachments/none)",
        draft,
        reviewers,
    };
    let url = create(
        &server.author(),
        &hosting,
        &new("feat", "Change things", false, &asked),
    )?;
    ensure!(
        url == format!("{}/{path}/pulls/1", server.url),
        "created {url}"
    );
    // The reviewers CODEOWNERS suggested were asked.
    let opened = server.author().get(&format!("repos/{path}/pulls/1"))?;
    ensure!(
        at(&opened, "/requested_reviewers/0/login") == "reviewer",
        "reviewers asked: {:?}",
        opened.get("requested_reviewers")
    );
    create(&server.author(), &hosting, &new("wip", "Later", true, &[]))?;
    let prs = list(&server.author(), &hosting)?;
    let feat = prs
        .iter()
        .find(|p| p.branch == "feat")
        .ok_or("feat isn't listed")?;
    let wip = prs
        .iter()
        .find(|p| p.branch == "wip")
        .ok_or("wip isn't listed")?;
    ensure!(
        feat.number == 1 && !feat.draft && feat.author == "author",
        "{feat:?}"
    );
    ensure!(wip.draft && wip.title.starts_with("WIP: "), "{wip:?}");
    ensure!(
        feat.base == "main" && wip.base == "main",
        "{feat:?} {wip:?}"
    );
    // Stacked: wip into feat, then back into main.
    eprintln!("{}: Retargeting a pull request", family.name);
    for base in ["feat", "main"] {
        retarget(&server.author(), &hosting, wip.number, base)?;
        let now = list(&server.author(), &hosting)?;
        let moved = now.iter().find(|p| p.number == wip.number);
        ensure!(
            moved.is_some_and(|p| p.base == base),
            "not into {base}: {moved:?}"
        );
    }
    // The forges call a draft unmergeable; that's no conflict.
    let wip_details = details::read(&server.author(), family.forge, &hosting, wip.number)?;
    ensure!(
        wip_details.draft && wip_details.mergeable != "CONFLICTING",
        "the draft reads as conflicting: {:?}",
        wip_details.mergeable
    );
    // Through the provider too, with whatever login it finds: the repository is public.
    eprintln!(
        "{}: Through the provider too, with whatever login it finds: the repository is public",
        family.name
    );
    let found = family.list(&dir, &hosting)?;
    ensure!(found.len() == 2, "the provider listed {found:?}");

    let read = |api: &Api| details::read(api, family.forge, &hosting, 1);
    let author_view = eventually("the merge check", || {
        let d = read(&server.author())?;
        Ok((d.mergeable == "MERGEABLE").then_some(d))
    })?;
    let d = &author_view;
    ensure!(d.inline_error.is_none(), "{:?}", d.inline_error);
    ensure!(
        d.title == "Change things"
            && d.state == "OPEN"
            && !d.draft
            && d.author == "author"
            && d.base_branch == "main"
            && d.head_branch == "feat"
            && d.head_oid == head
            && d.head_ref == "refs/pull/1/head"
            && d.changed_files == 2,
        "read {d:#?}"
    );
    ensure!(
        d.body.contains(&format!("{}/attachments/none", server.url)),
        "body {}",
        d.body
    );
    ensure!(
        d.extras.viewer.as_deref() == Some("author") && d.extras.can_write(),
        "{:?}",
        d.extras
    );
    ensure!(d.extras.merge_allowed, "merge commits aren't allowed");
    ensure!(
        d.commits.len() == 1 && d.commits[0].oid == head,
        "{:?}",
        d.commits
    );

    // The changes, fetched by the pull request's ref into a clone that hasn't got them.
    eprintln!(
        "{}: The changes, fetched by the pull request's ref into a clone that hasn't got them",
        family.name
    );
    let other = scratch(&format!("gitcraft-live-{key}-{stamp}-clone"))?;
    git::run(
        None,
        &[
            "clone",
            "-q",
            "--single-branch",
            "--branch",
            "main",
            &remote,
            &other.to_string_lossy(),
        ],
    )?;
    let files = pull_request::files(&other, d, &git::remotes(&other)?, None)?;
    let mut changed: Vec<&str> = files.files.iter().map(|f| f.path.as_str()).collect();
    changed.sort();
    ensure!(changed == ["a.txt", "b.txt"], "changes {changed:?}");
    ensure!(files.to == head, "changes up to {}", files.to);

    let target = |api: Api, d: &PrDetails| -> Result<GiteaTarget, String> {
        GiteaTarget::of(api, family.name, &hosting, d).ok_or_else(|| "no target".to_owned())
    };
    let as_reviewer = || read(&server.reviewer());
    let line = |side, line, body: &str| LineComment {
        path: "a.txt".to_owned(),
        line,
        side,
        body: body.to_owned(),
    };

    // Someone else reads it.
    eprintln!("{}: Someone else reads it", family.name);
    let d = as_reviewer()?;
    ensure!(
        d.extras.viewer.as_deref() == Some("reviewer") && d.extras.permission == "WRITE",
        "{:?}",
        d.extras
    );

    // A comment in the conversation.
    eprintln!("{}: A comment in the conversation", family.name);
    target(server.reviewer(), &d)?.comment("Looks interesting")?;
    let d = as_reviewer()?;
    ensure!(
        d.timeline.iter().any(|e| e.kind == EntryKind::Comment
            && e.author == "reviewer"
            && e.body == "Looks interesting"),
        "the comment isn't there: {:?}",
        d.timeline
    );

    // A pending review gathers comments on lines, and gives one back.
    eprintln!(
        "{}: A pending review gathers comments on lines, and gives one back",
        family.name
    );
    target(server.reviewer(), &d)?.add_pending(&line(DiffSide::Right, 2, "pending one"))?;
    let d = as_reviewer()?;
    let review = d.extras.pending_review.clone().ok_or("no pending review")?;
    // The author is a site admin, who is shown the reviewer's draft: it isn't the author's.
    let admin = read(&server.author())?;
    ensure!(
        admin.extras.pending_review.is_none(),
        "the admin took the reviewer's draft for theirs: {:?}",
        admin.extras.pending_review
    );
    ensure!(
        admin
            .extras
            .threads
            .iter()
            .flat_map(|t| &t.comments)
            .all(|c| c.body != "pending one"),
        "the admin sees the reviewer's draft"
    );
    target(server.reviewer(), &d)?.add_pending(&line(DiffSide::Right, 6, "pending two"))?;
    let d = as_reviewer()?;
    ensure!(
        d.extras.pending_review.as_ref() == Some(&review),
        "the second comment started another review: {:?}",
        d.extras.pending_review
    );
    let pending: Vec<(String, String)> = d
        .extras
        .pending_comments()
        .map(|(_, c)| (c.id.clone(), c.body.clone()))
        .collect();
    ensure!(pending.len() == 2, "pending {pending:?}");
    ensure!(
        !d.timeline.iter().any(|e| e.body.starts_with("pending")),
        "a pending comment is in the conversation"
    );
    let first = pending
        .iter()
        .find(|(_, body)| body == "pending one")
        .map(|(id, _)| id.clone())
        .ok_or("the first pending comment is gone")?;
    let removed = target(server.reviewer(), &d)?.remove_pending(&first, false);
    match family.forge {
        // Forgejo has the endpoint: the comment has to go.
        Forge::Forgejo => {
            removed?;
            let d = as_reviewer()?;
            let left: Vec<String> = d
                .extras
                .pending_comments()
                .map(|(_, c)| c.body.clone())
                .collect();
            ensure!(left == ["pending two"], "left pending {left:?}");
        }
        // Gitea hasn't: it says why, and the review stays as it was.
        _ => {
            let e = removed
                .err()
                .ok_or("Gitea took one comment out of a review")?;
            ensure!(e.contains("can't take one comment out"), "removing: {e}");
        }
    }

    // Sending it asks for changes.
    eprintln!("{}: Sending it asks for changes", family.name);
    let d = as_reviewer()?;
    target(server.reviewer(), &d)?.review(Verdict::RequestChanges, "Please fix")?;
    let d = as_reviewer()?;
    ensure!(
        d.extras.pending_review.is_none(),
        "the review is still pending"
    );
    ensure!(
        d.review_decision == "CHANGES_REQUESTED",
        "decision {}",
        d.review_decision
    );
    let request = d
        .timeline
        .iter()
        .find(|e| e.kind == EntryKind::Review("requested changes") && e.author == "reviewer")
        .ok_or("no request for changes")?;
    ensure!(
        request.body == "Please fix",
        "the review says {:?}",
        request.body
    );
    let request = request
        .review_id
        .clone()
        .ok_or("the request for changes can't be dismissed")?;
    let on_six = thread_id(DiffSide::Right, 6, "a.txt");
    ensure!(
        d.extras
            .threads
            .iter()
            .any(|t| t.id == on_six && t.comments.iter().all(|c| !c.pending)),
        "threads {:?}",
        d.extras.threads
    );

    // A reply joins the thread; a comment on a removed line is on the old side.
    eprintln!(
        "{}: A reply joins the thread; a comment on a removed line is on the old side",
        family.name
    );
    target(server.reviewer(), &d)?.reply(&on_six, "And a reply")?;
    target(server.reviewer(), &d)?.comment_now(&line(DiffSide::Left, 2, "The old line"))?;
    let d = as_reviewer()?;
    let thread = d
        .extras
        .threads
        .iter()
        .find(|t| t.id == on_six)
        .ok_or("the thread is gone")?;
    let said: Vec<&str> = thread.comments.iter().map(|c| c.body.as_str()).collect();
    ensure!(said.last() == Some(&"And a reply"), "thread says {said:?}");
    ensure!(
        thread.can_reply && !thread.can_resolve && !thread.outdated,
        "{thread:?}"
    );
    // Forgejo files a comment on a removed line under the merge base: that's no outdated one.
    ensure!(
        d.extras.threads.iter().any(|t| t.side == DiffSide::Left
            && t.line == Some(2)
            && !t.outdated
            && t.comments.iter().any(|c| c.body == "The old line")),
        "no current thread on the old line: {:?}",
        d.extras.threads
    );
    ensure!(
        target(server.reviewer(), &d)?
            .set_resolved(&on_six, true)
            .is_err(),
        "resolving should say it can't"
    );

    // A review of comments on lines alone: Gitea 28 sends it, Forgejo and older Gitea want words
    // with it, and gitcraft says so.
    eprintln!("{}: A review of comments on lines alone", family.name);
    target(server.reviewer(), &d)?.add_pending(&line(DiffSide::Right, 5, "Only this"))?;
    let d = as_reviewer()?;
    let version = server.author().get("version")?;
    let major: u32 = at(&version, "/version")
        .split('.')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let sends = family.forge == Forge::Gitea && major >= 28;
    match target(server.reviewer(), &d)?.review(Verdict::Comment, "") {
        Ok(()) => ensure!(sends, "{} {major} sent it without words", family.name),
        Err(e) => {
            ensure!(!sends && e.contains("a few words"), "sending: {e}");
            target(server.reviewer(), &d)?.discard_pending()?;
        }
    }
    let d = as_reviewer()?;
    ensure!(
        d.extras.pending_review.is_none(),
        "a review is still pending"
    );
    ensure!(
        d.timeline
            .iter()
            .all(|e| !e.body.contains(super::post::PENDING_WORDS)),
        "the placeholder shows: {:?}",
        d.timeline
    );

    // A pending review can be thrown away.
    eprintln!("{}: A pending review can be thrown away", family.name);
    target(server.reviewer(), &d)?.add_pending(&line(DiffSide::Right, 1, "Throwaway"))?;
    let d = as_reviewer()?;
    ensure!(d.extras.pending_review.is_some(), "nothing pending");
    target(server.reviewer(), &d)?.discard_pending()?;
    let d = as_reviewer()?;
    ensure!(
        d.extras.pending_review.is_none()
            && d.extras
                .threads
                .iter()
                .flat_map(|t| &t.comments)
                .all(|c| c.body != "Throwaway"),
        "the discarded review is still there"
    );

    // The author dismisses the request.
    eprintln!("{}: The author dismisses the request", family.name);
    let d = read(&server.author())?;
    target(server.author(), &d)?.dismiss(&request, "Handled elsewhere")?;
    let d = read(&server.author())?;
    ensure!(
        d.timeline
            .iter()
            .any(|e| e.kind == EntryKind::Review("had a review dismissed")),
        "no dismissed review: {:?}",
        d.timeline
    );
    ensure!(
        d.review_decision != "CHANGES_REQUESTED",
        "decision {}",
        d.review_decision
    );
    let before_suggestion = d;

    // A suggested change, committed by the author.
    eprintln!(
        "{}: A suggested change, committed by the author",
        family.name
    );
    let d = as_reviewer()?;
    target(server.reviewer(), &d)?.comment_now(&line(
        DiffSide::Right,
        3,
        "Louder:\n\n```suggestion\nTHREE\n```",
    ))?;
    let d = read(&server.author())?;
    let on_three = thread_id(DiffSide::Right, 3, "a.txt");
    let comment = d
        .extras
        .threads
        .iter()
        .find(|t| t.id == on_three)
        .and_then(|t| t.comments.first())
        .ok_or("no thread on line 3")?;
    let text = pr_post::shown_suggestion(&comment.body, "127.0.0.1")
        .ok_or("the suggestion isn't offered")?;
    ensure!(text == "THREE\n", "suggestion {text:?}");
    target(server.author(), &d)?.apply_suggestion(&Suggestion {
        path: "a.txt".to_owned(),
        start: 3,
        end: 3,
        text,
        author: comment.author.clone(),
    })?;
    let file = server
        .author()
        .get(&format!("repos/{path}/contents/a.txt?ref=feat"))?;
    let content = pr_post::decode_file(&file, family.name)?;
    ensure!(
        content == "one\nTWO\nTHREE\nfour\nfive\nsix\n",
        "after the suggestion: {content:?}"
    );

    // What was read before the suggestion doesn't get merged.
    eprintln!(
        "{}: What was read before the suggestion doesn't get merged",
        family.name
    );
    ensure!(
        target(server.author(), &before_suggestion)?
            .merge(MergeMethod::Merge, false)
            .is_err(),
        "merged a commit nobody saw"
    );

    // Approved, then merged, and the branch goes.
    eprintln!(
        "{}: Approved, then merged, and the branch goes",
        family.name
    );
    let d = as_reviewer()?;
    target(server.reviewer(), &d)?.review(Verdict::Approve, "")?;
    let d = eventually("the new commit's merge check", || {
        let d = read(&server.author())?;
        Ok((d.mergeable == "MERGEABLE" && d.head_oid != head).then_some(d))
    })?;
    ensure!(
        d.review_decision == "APPROVED",
        "decision {}",
        d.review_decision
    );
    ensure!(
        d.extras
            .threads
            .iter()
            .any(|t| t.id == on_three && t.outdated),
        "the suggestion's thread should be outdated now"
    );
    target(server.author(), &d)?.merge(MergeMethod::Merge, true)?;
    let d = read(&server.author())?;
    ensure!(d.state == "MERGED", "state {}", d.state);
    match server.author().get(&format!("repos/{path}/branches/feat")) {
        Err(failure) if failure.status == Some(404) => {}
        other => return Err(format!("the branch is still there: {other:?}")),
    }

    // Avatars of the accounts the commits belong to.
    eprintln!(
        "{}: Avatars of the accounts the commits belong to",
        family.name
    );
    let avatars = family.avatars(&dir, &hosting, &[]);
    ensure!(
        avatars
            .iter()
            .any(|(email, _)| email == "author@example.org"),
        "avatars {avatars:?}"
    );

    // Without a login, the server's refusal says how to get one.
    eprintln!(
        "{}: Without a login, the server's refusal says how to get one",
        family.name
    );
    let refused = server
        .api(None)
        .send(
            "POST",
            &format!("repos/{path}/issues/1/comments"),
            Some(&json!({ "body": "anonymous" })),
        )
        .map_err(String::from);
    ensure!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("found no login")),
        "anonymous comment: {refused:?}"
    );

    pull_request_from_a_fork(&server, family, &dir, &hosting, &name)?;

    stacked(&server, &hosting, &dir, family.name)?;

    release_with_a_file(&server, family, &dir, &hosting)?;

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&other);
    Ok(())
}

/// The reviewer forks the repository and proposes a branch of the fork, letting maintainers
/// edit it: the list says so, and the author checks it out as gitcraft does and pushes to it.
fn pull_request_from_a_fork(
    server: &Server,
    family: &'static GiteaFamily,
    dir: &Path,
    hosting: &Hosting,
    name: &str,
) -> Result<(), String> {
    eprintln!("{}: A pull request from a fork", family.name);
    let fork_name = format!("{name}-fork");
    server.reviewer().send(
        "POST",
        &format!("repos/{}/forks", hosting.path),
        Some(&json!({ "name": fork_name })),
    )?;
    let fork_path = format!("reviewer/{fork_name}");
    let as_reviewer =
        server
            .url
            .replacen("://", &format!("://reviewer:{}@", server.reviewer_token), 1)
            + &format!("/{fork_path}.git");
    run(dir, &["checkout", "-q", "-b", "patch", "main"])?;
    write(dir, "d.txt", "from the fork\n")?;
    run(dir, &["add", "."])?;
    run(dir, &["commit", "-q", "-m", "fix: from the fork"])?;
    eventually("the fork to be made", || {
        Ok(run(dir, &["push", "-q", &as_reviewer, "patch"])
            .ok()
            .map(|_| ()))
    })?;
    run(dir, &["checkout", "-q", "main"])?;
    run(dir, &["branch", "-D", "patch"])?;
    let opened = server.reviewer().send(
        "POST",
        &format!("repos/{}/pulls", hosting.path),
        Some(&json!({ "head": "reviewer:patch", "base": "main", "title": "From a fork" })),
    )?;
    let number = opened
        .get("number")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    server.reviewer().send(
        "PATCH",
        &format!("repos/{}/pulls/{number}", hosting.path),
        Some(&json!({ "allow_maintainer_edit": true })),
    )?;
    let forks = family.forks(dir, hosting)?;
    let info = forks
        .iter()
        .find(|f| f.number == number)
        .ok_or("the fork's pull request isn't listed")?;
    ensure!(
        info.cross
            && info.maintainers_can_edit
            && info.owner == "reviewer"
            && info.branch == "patch"
            && info.base == "main"
            && info.clone_https.ends_with(&format!("{fork_path}.git")),
        "{info:?}"
    );
    ensure!(
        forks.iter().any(|f| f.number != number && !f.cross),
        "the repository's own branch reads as a fork: {forks:?}"
    );
    // Checked out as gitcraft does, then pushed to with the author's login.
    let creds = git::Credentials {
        scope: None,
        username: "author".to_owned(),
        password: server.author_token.clone(),
    };
    let setup = crate::repo::forks::plan::read_setup(dir)?;
    let locals = vec!["main".to_owned(), "feat".to_owned(), "wip".to_owned()];
    let plan =
        crate::repo::forks::plan::checkout(info, &git::remotes(dir)?, &setup, &locals, "main")?;
    for step in &plan.steps {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        git::run_with(Some(dir), &args, Some(&creds))?;
    }
    write(dir, "d.txt", "fixed by the maintainer\n")?;
    run(dir, &["commit", "-q", "-am", "fix: by the maintainer"])?;
    git::run_with(Some(dir), &["push", "-q"], Some(&creds))?;
    let mine = run(dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    let branch = server
        .author()
        .get(&format!("repos/{fork_path}/branches/patch"))?;
    ensure!(
        at(&branch, "/commit/id") == mine,
        "the fork's branch is at {:?}, not {mine}",
        at(&branch, "/commit/id")
    );
    run(dir, &["checkout", "-q", "main"])?;
    Ok(())
}

/// The provider, logged in as the author: the forge's own calls, with the server's token
/// (a test can't hand the real provider a login without changing the process's environment).
struct AsAuthor<'a>(&'a Server);

impl Provider for AsAuthor<'_> {
    fn name(&self) -> &'static str {
        "live"
    }

    fn claims(&self, _: &str) -> bool {
        false
    }

    fn list(&self, _: &Path, hosting: &Hosting) -> Result<Vec<forge::PullRequest>, String> {
        list(&self.0.author(), hosting)
    }

    fn create(&self, _: &Path, hosting: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        create(&self.0.author(), hosting, new)
    }

    fn retarget(&self, _: &Path, hosting: &Hosting, number: u64, base: &str) -> Result<(), String> {
        retarget(&self.0.author(), hosting, number, base)
    }

    fn find(&self, _: &Path, hosting: &Hosting, branch: &str) -> Result<Option<Found>, String> {
        super::find(&self.0.author(), hosting, branch)
    }

    fn avatars(&self, _: &Path, _: &Hosting, _: &[String]) -> Vec<(String, crate::avatars::Hint)> {
        Vec::new()
    }
}

/// A stack's pull requests on the server: published bottom-up, published again without
/// duplicates, and the upper one retargeted once the lower one is closed.
fn stacked(server: &Server, hosting: &Hosting, dir: &Path, name: &str) -> Result<(), String> {
    use crate::forge::stack::{Input, Target, plan, publish};
    eprintln!("{name}: A stack of pull requests");
    run(dir, &["checkout", "-q", "-b", "stack-low", "main"])?;
    write(dir, "low.txt", "low\n")?;
    run(dir, &["add", "."])?;
    run(dir, &["commit", "-q", "-m", "low layer"])?;
    run(dir, &["checkout", "-q", "-b", "stack-high"])?;
    write(dir, "high.txt", "high\n")?;
    run(dir, &["add", "."])?;
    run(dir, &["commit", "-q", "-m", "high layer"])?;
    run(dir, &["checkout", "-q", "main"])?;
    let main = run(dir, &["rev-parse", "main"])?.trim().to_owned();
    let low = run(dir, &["rev-parse", "stack-low"])?.trim().to_owned();
    for step in [
        git::stack::set_parent_ops("stack-low", "main", &main),
        git::stack::set_parent_ops("stack-high", "stack-low", &low),
    ]
    .concat()
    {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        run(dir, &args)?;
    }
    let inputs = [
        Input {
            branch: "stack-low".into(),
            parent: "main".into(),
        },
        Input {
            branch: "stack-high".into(),
            parent: "stack-low".into(),
        },
    ];
    let provider = AsAuthor(server);
    let publish_now = || -> Result<(), String> {
        let find = |branch: &str| provider.find(dir, hosting, branch);
        let plans = plan(dir, &inputs, &find)?;
        let report = publish(
            dir,
            &plans,
            &Target {
                provider: &provider,
                hosting,
                remote: "origin",
                draft: false,
                creds: None,
            },
        );
        match report.failed {
            Some((branch, e)) => Err(format!("publishing {branch}: {e}")),
            None => Ok(()),
        }
    };
    let bases = || -> Result<Vec<(String, String)>, String> {
        let mut out: Vec<(String, String)> = list(&server.author(), hosting)?
            .into_iter()
            .filter(|p| p.branch.starts_with("stack-"))
            .map(|p| (p.branch, p.base))
            .collect();
        out.sort();
        Ok(out)
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    publish_now()?;
    let first = bases()?;
    ensure!(
        first == [pair("stack-high", "stack-low"), pair("stack-low", "main")],
        "published as {first:?}"
    );
    // Again: nothing opened twice.
    publish_now()?;
    let again = bases()?;
    ensure!(again == first, "published again as {again:?}");
    // The lower one merged: the upper one goes into main.
    let low_pr = list(&server.author(), hosting)?
        .into_iter()
        .find(|p| p.branch == "stack-low")
        .ok_or("stack-low has no pull request")?;
    let merge = format!("repos/{}/pulls/{}/merge", hosting.path, low_pr.number);
    // The forge works out whether it can be merged in the background first.
    eventually("the lower pull request to merge", || {
        match server
            .author()
            .send("POST", &merge, Some(&json!({ "Do": "merge" })))
        {
            Ok(_) => Ok(Some(())),
            Err(f) if f.status == Some(405) => Ok(None),
            Err(f) => Err(String::from(f)),
        }
    })?;
    publish_now()?;
    let after = bases()?;
    ensure!(
        after == [pair("stack-high", "main")],
        "after the merge: {after:?}"
    );
    let parent = run(dir, &["config", "branch.stack-high.gitcraft-parent"])?;
    ensure!(parent.trim() == "main", "stack-high is based on {parent}");
    // Published again: the merged one isn't opened again.
    publish_now()?;
    ensure!(bases()? == after, "a merged layer was opened again");
    Ok(())
}

/// A tag pushed, its release made with notes, and a file attached to it.
fn release_with_a_file(
    server: &Server,
    family: &'static GiteaFamily,
    dir: &Path,
    hosting: &Hosting,
) -> Result<(), String> {
    eprintln!("{}: A release with a file", family.name);
    run(dir, &["tag", "-a", "v1.0.0", "-m", "v1.0.0", "main"])?;
    run(dir, &["push", "-q", "origin", "v1.0.0"])?;
    let url = create_release(
        &server.author(),
        hosting,
        &forge::NewRelease {
            tag: "v1.0.0",
            title: "v1.0.0",
            notes: "### Features\n\n- ü things",
            prerelease: true,
        },
    )?;
    ensure!(url.contains("/releases/tag/v1.0.0"), "release at {url}");
    let file = dir.join("dist ü.txt");
    std::fs::write(&file, "attached\n").map_err(|e| e.to_string())?;
    upload_assets(
        &server.author(),
        hosting,
        "v1.0.0",
        std::slice::from_ref(&file),
    )?;
    // Resumed: the same release, the file not attached twice.
    let again = create_release(
        &server.author(),
        hosting,
        &forge::NewRelease {
            tag: "v1.0.0",
            title: "v1.0.0",
            notes: "other",
            prerelease: true,
        },
    )?;
    ensure!(again == url, "made again at {again}");
    upload_assets(
        &server.author(),
        hosting,
        "v1.0.0",
        std::slice::from_ref(&file),
    )?;
    let release = server
        .author()
        .get(&format!("repos/{}/releases/tags/v1.0.0", hosting.path))?;
    ensure!(
        release
            .get("assets")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            == Some(1),
        "attached twice: {release}"
    );
    ensure!(
        at(&release, "/body").contains("ü things")
            && release
                .get("prerelease")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            && at(&release, "/assets/0/name") == "dist ü.txt",
        "{release}"
    );
    Ok(())
}
