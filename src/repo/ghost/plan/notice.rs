//! Localized preview notices from retained raw operation text.

/// Paint only finite original preview notes/errors; cached outcomes remain raw.
pub(in crate::repo::ghost) fn display_preview_text(raw: &str) -> String {
    match raw {
        "Uncommitted changes are stashed first and put back after (rebase.autoStash)." => {
            return crate::i18n::text("repo-workflows-ghost-rebase-e550da8a").to_owned();
        }
        "the rebase refuses to start with uncommitted changes: commit or stash them first" => {
            return crate::i18n::text("repo-workflows-ghost-rebase-b1f8a147").to_owned();
        }
        "Already up to date: nothing to replay, nothing changes." => {
            return crate::i18n::text("repo-workflows-ghost-rebase-1f963916").to_owned();
        }
        "Invalid timeline commit ID" => {
            return crate::i18n::text("repo-workflows-ghost-timeline-48223518").to_owned();
        }
        "Timeline preview is incomplete: required local history is missing or the bounded read stopped. Hover never downloads objects." =>
        {
            return crate::i18n::text("repo-workflows-ghost-timeline-5a1baa67").to_owned();
        }
        "File-change preview is incomplete: required blobs are unavailable locally or the bounded read stopped. No objects were downloaded." =>
        {
            return crate::i18n::text("repo-workflows-ghost-timeline-18df3f5f").to_owned();
        }
        "Uncommitted changes are preserved with --keep; a conflicting checkout will refuse the jump." =>
        {
            return crate::i18n::text("repo-workflows-ghost-timeline-6f3d9015").to_owned();
        }
        "Saved refs keep earlier states recoverable. This preview moves no refs or files." => {
            return crate::i18n::text("repo-workflows-ghost-timeline-c25867a7").to_owned();
        }
        "Uncommitted changes to these files are thrown away, and can't be brought back." => {
            return crate::i18n::text("repo-workflows-ghost-reset-hard-3126bbde").to_owned();
        }
        "Already up to date: the branch has everything its remote has." => {
            return crate::i18n::text("repo-workflows-ghost-pull-dd489c0a").to_owned();
        }
        "Fast-forward: the branch moves to its remote's commit." => {
            return crate::i18n::text("repo-workflows-ghost-pull-c7917d08").to_owned();
        }
        "Uncommitted changes are stashed first and put back after." => {
            return crate::i18n::text("repo-workflows-ghost-pull-3a19c0c2").to_owned();
        }
        "its upstream" => {
            return crate::i18n::text("repo-workflows-ghost-delete-branch-dde58e5a").to_owned();
        }
        "the checked-out branch" => {
            return crate::i18n::text("repo-workflows-ghost-delete-branch-fadd1b38").to_owned();
        }
        "the first commit has no parent to rebase from" => {
            return crate::i18n::text("repo-workflows-ghost-drop-commit-da2d4ead").to_owned();
        }
        "merges came after the commit, which this would flatten" => {
            return crate::i18n::text("repo-workflows-ghost-drop-commit-181a54e0").to_owned();
        }
        "rebase.rebaseMerges is set and merges are among the commits: git keeps them, which the preview can't replay yet" =>
        {
            return crate::i18n::text("repo-workflows-ghost-rewrite-3d8b966c").to_owned();
        }
        "The rebase stops there, asking whether to keep the commit that became empty; the branch doesn't move yet." =>
        {
            return crate::i18n::text("repo-workflows-ghost-rewrite-3477e4cb").to_owned();
        }
        "The rebase stops there until the conflicts are resolved; the branch doesn't move yet." => {
            return crate::i18n::text("repo-workflows-ghost-rewrite-e69265a2").to_owned();
        }
        _ => {}
    }
    if let Some(rest) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" tracks no branch on a remote: nothing to pull"))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-pull-f53705f4",
            &[("branch-name-1", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("As of the last fetch of ")
        .and_then(|s| s.strip_suffix(": what was pushed there since isn't shown."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-pull-9045dbaa",
            &[("up-remote-1", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("there is no branch ")
        .and_then(|s| s.strip_suffix(" here"))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-a9c01edc",
            &[("name", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" tracks no branch on a remote"))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-f06fee94",
            &[("name", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" loses nothing: a plain push would do."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-c637c04f",
            &[("up-tracking-name-1", value0.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("As of the last fetch of ").and_then(|s| {
        s.strip_suffix(": with the lease, the push is refused if someone pushed since.")
    }) {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-2d4460fd",
            &[("up-remote-1", value0.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("").and_then(|s| s.strip_suffix(": Delete refuses unless Force (-D) is ticked, which leaves its own commits on no branch."))
        && let Some((value0, value1)) = rest.split_once(" isn't merged into ") {
            let value1 = match value1 { "its upstream" => crate::i18n::text("repo-workflows-ghost-delete-branch-dde58e5a"), "the checked-out branch" => crate::i18n::text("repo-workflows-ghost-delete-branch-fadd1b38"), _ => return raw.to_owned() };
            return crate::i18n::message("repo-workflows-ghost-delete-branch-26d05fbc", &[("name", value0.into()), ("what", value1.into())]);
        }
    if let Some(rest) = raw.strip_prefix("").and_then(|s| s.strip_suffix(")"))
        && let Some((value0, value1)) =
            rest.split_once(" commits to replay, too many to preview (at most ")
    {
        let Ok(value0) = value0.parse::<u64>() else {
            return raw.to_owned();
        };
        let Ok(value1) = value1.parse::<u64>() else {
            return raw.to_owned();
        };
        return crate::i18n::message(
            "repo-workflows-ghost-rewrite-c84070ef",
            &[
                ("w-picks-len-1", value0.into()),
                ("PICKS-LIMIT", value1.into()),
            ],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("Meanwhile HEAD is detached at ")
        .and_then(|s| s.strip_suffix(", mid-rebase."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-rewrite-fb29af2f",
            &[("waits", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("The files that change aren't listed: git ")
        .and_then(|s| s.strip_suffix(" replays in a scratch clone."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-rewrite-af60968a",
            &[("self-version-1", value0.into())],
        );
    }
    if let Some((count, rest)) = raw.split_once(' ')
        && let Ok(count) = count.parse::<usize>()
        && ((count == 1 && rest == "commit is dropped: its change is already there.")
            || (count != 1 && rest == "commits are dropped: their changes are already there."))
    {
        return dropped_note(count);
    }
    if let Some((branch, tail)) = raw.split_once(" and ")
        && !branch.chars().any(char::is_whitespace)
    {
        if let Some(tracking) =
            tail.strip_suffix(" have diverged, so a fast-forward-only pull refuses")
            && !tracking.chars().any(char::is_whitespace)
        {
            return diverged_pull(branch, tracking, false);
        }
        if let Some((tracking, fetched)) =
            tail.split_once(" have diverged, so a fast-forward-only pull refuses (only ")
            && !tracking.chars().any(char::is_whitespace)
            && fetched == format!("{tracking} moves, with what it fetches)")
        {
            return diverged_pull(branch, tracking, true);
        }
    }
    if raw == "HEAD is detached: check out a branch first" {
        return crate::i18n::text("repo-workflows-ghost-current-7776bacf").to_owned();
    }
    crate::repo::activity::display_workflow_error(raw)
}
pub(super) fn diverged_pull(branch: &str, tracking: &str, fetched: bool) -> String {
    crate::i18n::message(
        "repo-workflows-ghost-diverged-pull",
        &[
            ("branch", (branch).into()),
            ("tracking", (tracking).into()),
            ("fetched", (if fetched { "yes" } else { "no" }).into()),
        ],
    )
}

pub(super) fn dropped_note(count: usize) -> String {
    crate::i18n::message(
        "repo-workflows-ghost-dropped-note",
        &[("count", (count).into())],
    )
}
