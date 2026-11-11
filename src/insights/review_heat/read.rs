//! Read and compare reviewed repository versions.
use super::*;

/// `git diff -U0`'s hunks, by file (its new name; its old one when it was deleted).
pub(crate) fn parse_hunks(out: &str) -> HashMap<String, Vec<Hunk>> {
    let mut found: HashMap<String, Vec<Hunk>> = HashMap::new();
    let mut file: Option<String> = None;
    let mut old_name: Option<String> = None;
    let mut in_hunk = false;
    for line in out.lines() {
        if line.starts_with("diff --git ") {
            file = None;
            old_name = None;
            in_hunk = false;
            continue;
        }
        if !in_hunk {
            if let Some(path) = line.strip_prefix("--- a/") {
                old_name = Some(path.trim_end_matches('\t').to_owned());
            } else if let Some(path) = line.strip_prefix("+++ b/") {
                file = Some(path.trim_end_matches('\t').to_owned());
            } else if line == "+++ /dev/null" {
                file = old_name.clone();
            }
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            in_hunk = true;
            if let (Some(path), Some(hunk)) = (&file, hunk_header(rest)) {
                found.entry(path.clone()).or_default().push(hunk);
            }
        }
    }
    found
}

/// `-12,3 +14,0 @@ fn x()`: the hunk's ranges (a length left out is 1).
fn hunk_header(rest: &str) -> Option<Hunk> {
    let mut parts = rest.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let range = |s: &str| -> Option<(u32, u32)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (old_start, old_len) = range(old)?;
    let (new_start, new_len) = range(new)?;
    Some(Hunk {
        old_start,
        old_len,
        new_start,
        new_len,
    })
}

/// The hunks from `from` to `to`, by file.
fn hunks(repo: &Path, from: &str, to: &str) -> Result<HashMap<String, Vec<Hunk>>, String> {
    let out = git::run(
        Some(repo),
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "-U0",
            "-M",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--end-of-options",
            from,
            to,
            "--",
        ],
    )?;
    Ok(parse_hunks(&out))
}

/// Whether `oid` is a whole commit id, as the forges give them.
fn is_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What the version seen of pull request `number` says against `head`. `repo_key`: the forge
/// repository the versions seen are kept for (`None`: no forge selected).
pub fn seen_state(repo: &Path, repo_key: Option<&str>, number: u64, head: &str) -> Seen {
    let Some(key) = repo_key else {
        return Seen::Unknown("no primary forge is selected".to_owned());
    };
    let seen = match git::reviewed(repo, key, number) {
        Ok(Some(seen)) => seen,
        Ok(None) => return Seen::Never,
        Err(e) => return Seen::Unknown(format!("the versions seen couldn't be read: {e}")),
    };
    if seen.tip == head {
        return Seen::Fresh {
            tip: seen.tip,
            since: HashMap::new(),
        };
    }
    if git::is_ancestor(repo, &seen.tip, head) {
        // A file changed without hunks (a binary one, a mode) is changed all the same.
        let found = hunks(repo, &seen.tip, head).and_then(|mut since| {
            for path in changed_paths(repo, &seen.tip, head)? {
                since.entry(path).or_default();
            }
            Ok(since)
        });
        return match found {
            Ok(since) => Seen::Fresh {
                tip: seen.tip,
                since,
            },
            Err(e) => Seen::Unknown(format!("what changed since couldn't be read: {e}")),
        };
    }
    match changed_paths(repo, &seen.tip, head) {
        Ok(differing) => Seen::Stale {
            tip: seen.tip,
            differing,
        },
        Err(e) => Seen::Unknown(format!("the version seen couldn't be compared: {e}")),
    }
}

/// The files whose content differs from `from` to `to`.
fn changed_paths(repo: &Path, from: &str, to: &str) -> Result<HashSet<String>, String> {
    let out = git::run(
        Some(repo),
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--end-of-options",
            from,
            to,
            "--",
        ],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Why `seen` is stale, for the reader, when it is.
pub fn stale_reason(seen: &Seen, head: &str) -> Option<String> {
    let Seen::Stale { tip, .. } = seen else {
        return None;
    };
    Some(format!(
        "The version last seen in gitcraft ({}) is not part of the head now ({}): the branch \
         was force-pushed or rebased since. What was seen can't be matched hunk by hunk; files \
         whose content is the same as then are marked so (inferred), the rest is stale.",
        crate::repo::short(tip),
        crate::repo::short(head)
    ))
}

/// The pull request `details` (as the forge gave it) by file and hunk, against the version
/// seen. Its commits must be here: nothing is fetched (opening its page fetches them).
pub fn analyze(repo: &Path, details: &PrDetails, repo_key: Option<&str>) -> Result<Heat, String> {
    let (head, base) = (&details.head_oid, &details.base_oid);
    if !is_oid(head) || !is_oid(base) {
        return Err("the forge named commits that aren't commit ids".to_owned());
    }
    let here = |oid: &str| {
        git::run(
            Some(repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    };
    if !here(head) || !here(base) {
        return Err(format!(
            "the commits of #{} aren't in this repository yet: open the pull request's page \
             once, which fetches them",
            details.number
        ));
    }
    let from = git::merge_base(repo, base, head)?;
    let mut changed = git::diff_files(repo, &from, Some(head))?;
    let cut = changed.len() > MAX_FILES;
    changed.truncate(MAX_FILES);
    let mut by_file = hunks(repo, &from, head)?;
    let files: Vec<FileDiff> = changed
        .into_iter()
        .map(|f| FileDiff {
            hunks: by_file.remove(&f.path).unwrap_or_default(),
            path: f.path,
            old_path: f.old_path,
            status: f.status,
            added: f.added,
            removed: f.removed,
        })
        .collect();
    let seen = seen_state(repo, repo_key, details.number, head);
    let threads: Option<Vec<ThreadAt>> = details.inline_error.is_none().then(|| {
        details
            .extras
            .threads
            .iter()
            .map(ThreadAt::from_thread)
            .collect()
    });
    let heat_files = derive(files, &seen, threads.as_deref());
    let threads_elsewhere = threads.is_some()
        && !details.extras.head_oid.is_empty()
        && details.extras.head_oid != *head;
    let mut provenance = Provenance::new(
        "forge (threads) · git diff · versions seen in gitcraft",
        format!(
            "#{}: {} into {}",
            details.number, details.head_branch, details.base_branch
        ),
    )
    .tip(head_name(details.number), head);
    if let Seen::Fresh { tip, .. } | Seen::Stale { tip, .. } = &seen {
        provenance = provenance.tip("seen", tip);
    }
    let mut partial = Vec::new();
    if cut {
        partial.push(format!("only the first {MAX_FILES} files"));
    }
    if heat_files.iter().any(|f| f.more_hunks) {
        partial.push(format!("only the first {MAX_HUNKS} hunks of a file"));
    }
    if let Some(e) = &details.inline_error {
        partial.push(format!("the review threads couldn't be read: {e}"));
    }
    if threads_elsewhere {
        partial.push(
            "the threads' lines were read at another head: where they are is a guess".to_owned(),
        );
    }
    if details.extras.threads.iter().any(|t| t.more_comments) {
        partial.push("some threads have more comments than were read".to_owned());
    }
    if !partial.is_empty() {
        provenance = provenance.partial(partial.join("; "));
    }
    Ok(Heat {
        number: details.number,
        stale_review: stale_reason(&seen, head),
        from,
        head: head.clone(),
        files: heat_files,
        threads,
        threads_elsewhere,
        seen: Some(seen),
        provenance,
    })
}

/// What the provenance calls the head of pull request `number`.
pub fn head_name(number: u64) -> String {
    format!("#{number} head")
}
