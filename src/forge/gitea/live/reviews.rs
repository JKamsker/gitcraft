//! Pending review, line conversation, dismissal and viewer-isolation scenario.
use super::*;

pub(super) fn conversation(
    server: &Server,
    family: &'static GiteaFamily,
    hosting: &Hosting,
) -> Result<PrDetails, String> {
    let read = |api: &Api| details::read(api, family.forge, hosting, 1);
    let target = |api: Api, d: &PrDetails| -> Result<GiteaTarget, String> {
        GiteaTarget::of(api, family.name, hosting, d).ok_or_else(|| "no target".to_owned())
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
            .all(|e| !e.body.contains(super::super::post::PENDING_WORDS)),
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

    Ok(before_suggestion)
}
