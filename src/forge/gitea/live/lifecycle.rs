//! Full lifecycle orchestration, captured repository, checkout, suggestion and merge checks.
use super::*;

pub(super) fn whole_life(family: &'static GiteaFamily) -> Result<(), String> {
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
    let files = pull_request::files(&other, &hosting, d, &git::remotes(&other)?, None)?;
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

    let before_suggestion = reviews::conversation(&server, family, &hosting)?;

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
