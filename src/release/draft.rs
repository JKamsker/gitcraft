//! Read release history and assemble a version and notes draft.
use super::*;

/// Reads what a release of `commit` starts from. `remote`: the one to push to.
pub(crate) fn draft(repo: &Path, commit: &str, remote: Option<String>) -> Result<Draft, String> {
    let commit = git::resolve(repo, &format!("{commit}^{{commit}}"))?;
    // The last tag reachable, and the version it says (tags that aren't versions are passed
    // over by git's own `--match` only loosely: checked here).
    let last_tag = git::run_ok_codes(
        Some(repo),
        &[
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            "v[0-9]*",
            "--match",
            "[0-9]*",
            &commit,
        ],
        &[0, 128],
    )
    .ok()
    .map(|s| s.trim().to_owned())
    .filter(|s| !s.is_empty());
    let range = match &last_tag {
        Some(tag) => format!("refs/tags/{tag}..{commit}"),
        None => commit.clone(),
    };
    let changes = read_changes(repo, &range)?;
    let head = git::head(repo);
    let branch = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?
        .trim()
        .strip_prefix("refs/heads/")
        .map(str::to_owned)
        .filter(|_| head == commit);
    let mut manifests = Vec::new();
    for kind in Kind::ALL {
        let (file, _) = kind.files();
        if let Ok(bytes) = git::file_at(repo, Some(&commit), file)
            && let Some(v) = kind.version(&String::from_utf8_lossy(&bytes))
        {
            manifests.push((kind, v));
        }
    }
    let changelog = git::file_at(repo, Some(&commit), CHANGELOG).is_ok();
    let manifest_note = if manifests.is_empty() && !changelog {
        None
    } else if branch.is_none() {
        Some("Only the tip of the branch checked out can move the version in its files".to_owned())
    } else {
        let mut files: Vec<&str> = manifests
            .iter()
            .flat_map(|(k, _)| {
                let (a, b) = k.files();
                [a, b]
            })
            .collect();
        if changelog {
            files.push(CHANGELOG);
        }
        let mut args = vec!["status", "--porcelain", "--"];
        args.extend(&files);
        let dirty = git::run(Some(repo), &args)?;
        (!dirty.trim().is_empty())
            .then(|| "Commit or stash the changes to the files the version is in first".to_owned())
    };
    if manifest_note.is_some() {
        manifests.clear();
    }
    let sign = git::run_ok_codes(
        Some(repo),
        &["config", "--bool", "--get", "tag.gpgSign"],
        &[0, 1],
    )
    .is_ok_and(|v| v.trim() == "true");
    let remote_ref = branch.as_deref().map(|b| {
        let out = git::run(
            Some(repo),
            &[
                "for-each-ref",
                "--format=%(upstream:remotename)%00%(upstream:remoteref)",
                &format!("refs/heads/{b}"),
            ],
        )
        .unwrap_or_default();
        let mut parts = out.trim().split('\0');
        match (parts.next(), parts.next()) {
            (Some(r), Some(dst)) if Some(r) == remote.as_deref() && !dst.is_empty() => {
                dst.to_owned()
            }
            _ => format!("refs/heads/{b}"),
        }
    });
    let tags: Vec<String> = git::run(Some(repo), &["tag", "--list"])?
        .lines()
        .map(str::to_owned)
        .collect();
    let base = last_tag.as_deref().and_then(Version::parse).or_else(|| {
        manifests
            .first()
            .and_then(|(_, v)| Version::parse(&format!("v{v}")))
    });
    let suggested = match (&base, conventional::bump_of(&changes)) {
        // A manifest's version not tagged yet is the first release's.
        (Some(v), _) if last_tag.is_none() => v.clone(),
        (Some(v), Some(bump)) => v.next(bump),
        (Some(v), None) => v.clone(),
        (None, _) => Version {
            prefix: "v".into(),
            major: 0,
            minor: 1,
            patch: 0,
            pre: String::new(),
        },
    };
    Ok(Draft {
        commit,
        last_tag,
        changes,
        suggested,
        branch,
        manifests,
        manifest_note,
        changelog,
        sign,
        remote,
        remote_ref,
        tags,
    })
}

pub(super) const FIELD: char = '\u{1f}';

pub(super) const RECORD: char = '\u{1e}';

pub(super) fn read_changes(repo: &Path, range: &str) -> Result<Vec<Change>, String> {
    let out = git::run(
        Some(repo),
        &[
            "log",
            "--no-merges",
            "--format=%H%x1f%s%x1f%b%x1e",
            range,
            "--",
        ],
    )?;
    Ok(out
        .split(RECORD)
        .filter_map(|r| {
            let mut f = r.trim_start_matches('\n').splitn(3, FIELD);
            let hash = f.next()?.trim();
            if hash.is_empty() {
                return None;
            }
            Some(Change::parse(
                hash,
                f.next().unwrap_or_default(),
                f.next().unwrap_or_default(),
            ))
        })
        .collect())
}

/// Where a pull request numbered `n` is on `hosting`.
pub(crate) fn pr_url(hosting: &Hosting, n: u64) -> String {
    let base = format!("{}/{}", hosting.web, hosting.path);
    match hosting.forge {
        Forge::GitHub => format!("{base}/pull/{n}"),
        Forge::Azure => format!("{base}/pullrequest/{n}"),
        Forge::Bitbucket => format!("{base}/pull-requests/{n}"),
        Forge::GitLab => format!("{base}/-/merge_requests/{n}"),
        Forge::Forgejo | Forge::Gitea => format!("{base}/pulls/{n}"),
    }
}
