//! Publication responsibilities for storyboard.
use super::*;

pub(super) fn preserve(repo: &Path, key: &str, board: &Board) -> Result<(), String> {
    // JSON references alone do not retain commit objects through gc.
    for (i, c) in board.cards.iter().enumerate() {
        for (role, rev) in [
            ("base", &c.base),
            ("head", &c.head),
            (
                "anchor",
                match &c.anchor {
                    Anchor::Code { commit, .. } | Anchor::Test { commit, .. } => commit,
                },
            ),
        ] {
            git::run(
                Some(repo),
                &[
                    "update-ref",
                    &format!(
                        "refs/gitcraft/storyboard-anchors/{:x}/{i}/{role}",
                        md5::compute(key)
                    ),
                    rev,
                ],
            )?;
        }
    }
    store::save(repo, "storyboard", key, board)
}
pub(super) fn shared_ref(key: &str) -> String {
    format!("refs/gitcraft/storyboards/{:x}", md5::compute(key))
}
pub(super) fn export(
    repo: &Path,
    key: &str,
    board: &Board,
    previous: Option<&str>,
) -> Result<String, String> {
    if board.cards.len() > 256 {
        return Err("A shared storyboard supports at most 256 cards".into());
    }
    let json = serde_json::to_string(board).map_err(|e| e.to_string())?;
    let blob = git::run_input(Some(repo), &["hash-object", "-w", "--stdin"], &json)?;
    let tree = git::run_input(
        Some(repo),
        &["mktree"],
        &format!("100644 blob {}\tstoryboard.json\n", blob.trim()),
    )?;
    let mut parents = std::collections::BTreeSet::new();
    if let Some(previous) = previous {
        parents.insert(oid(repo, previous)?);
    }
    for card in &board.cards {
        for rev in [
            &card.base,
            &card.head,
            match &card.anchor {
                Anchor::Code { commit, .. } | Anchor::Test { commit, .. } => commit,
            },
        ] {
            parents.insert(oid(repo, rev)?);
        }
    }
    let mut args = vec![
        "-c".into(),
        "commit.gpgSign=false".into(),
        "commit-tree".into(),
        tree.trim().to_owned(),
        "-m".into(),
        format!("gitcraft review storyboard {key}"),
    ];
    for parent in parents {
        args.extend(["-p".into(), parent]);
    }
    git::run(
        Some(repo),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .map(|s| s.trim().into())
}
pub(super) fn imported(repo: &Path, commit: &str) -> Result<Board, String> {
    let json = git::run(Some(repo), &["show", &format!("{commit}:storyboard.json")])?;
    if json.len() > 2 * 1024 * 1024 {
        return Err("Shared storyboard exceeds the 2MiB metadata budget".into());
    }
    let board: Board =
        serde_json::from_str(&json).map_err(|e| format!("Invalid shared storyboard: {e}"))?;
    if board.cards.len() > 256 {
        return Err("Shared storyboard has more than 256 cards".into());
    }
    // Verify pinned sources are present commits. Fetching the artifact retains them through
    // its parent graph, including source commits removed by a later force push.
    for card in &board.cards {
        for rev in [
            &card.base,
            &card.head,
            match &card.anchor {
                Anchor::Code { commit, .. } | Anchor::Test { commit, .. } => commit,
            },
        ] {
            if oid(repo, rev)? != *rev {
                return Err("Shared storyboard revision is not an immutable commit ID".into());
            }
        }
    }
    for card in &board.cards {
        if let Anchor::Code {
            commit, path, blob, ..
        } = &card.anchor
            && file_blob(repo, commit, path)? != *blob
        {
            return Err("Shared storyboard blob does not match its pinned source".into());
        }
    }
    Ok(board)
}
pub(super) fn cached_ref(key: &str, target: &sharing::Target) -> String {
    format!(
        "refs/gitcraft/storyboard-remotes/{}/{:x}",
        target.cache_key(),
        md5::compute(key)
    )
}
pub(super) fn publish_board(
    repo: &Path,
    key: &str,
    board: &Board,
    target: &sharing::Target,
    creds: Option<&crate::git::Credentials>,
) -> Result<(), String> {
    target.guard(repo)?;
    let shared = shared_ref(key);
    let cached = cached_ref(key, target);
    // A remote can have different fetch/push URLs. The lease must describe the push server.
    let remote_tip = target.run(
        repo,
        &["ls-remote", "--refs", target.push_url(), &shared],
        creds,
    )?;
    let previous = remote_tip.split_whitespace().next().map(str::to_owned);
    if previous.is_some() {
        target.guard(repo)?;
        target.fetch(
            repo,
            true,
            &[format!("+{shared}:{cached}")],
            &cached,
            false,
            creds,
        )?;
        if git::run(Some(repo), &["rev-parse", &cached])?.trim()
            != previous.as_deref().unwrap_or("")
        {
            return Err(
                "Shared storyboard changed while preparing publication; fetch and retry".into(),
            );
        }
    }
    let commit = export(repo, key, board, previous.as_deref())?;
    target.guard(repo)?;
    target.run(
        repo,
        &[
            "push",
            &format!(
                "--force-with-lease={shared}:{}",
                previous.as_deref().unwrap_or("")
            ),
            "--",
            target.push_url(),
            &format!("{commit}:{shared}"),
        ],
        creds,
    )?;
    let old_cache = target
        .fetched(&cached)?
        .unwrap_or_else(|| "0".repeat(commit.len()));
    git::run(Some(repo), &["update-ref", &cached, &commit, &old_cache])?;
    Ok(())
}
pub(super) fn fetch_board(
    repo: &Path,
    key: &str,
    target: &sharing::Target,
    creds: Option<&crate::git::Credentials>,
) -> Result<Board, String> {
    target.guard(repo)?;
    let cached = cached_ref(key, target);
    target.fetch(
        repo,
        false,
        &[format!("+{}:{cached}", shared_ref(key))],
        &cached,
        false,
        creds,
    )?;
    let commit = target
        .fetched(&cached)?
        .ok_or("Shared storyboard was not fetched")?;
    imported(repo, &commit)
}
