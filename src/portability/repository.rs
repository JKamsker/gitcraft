use super::{Remote, Repository, Worktree, leaf, reference, sanitize_url};
use crate::git;
use std::path::Path;

pub(crate) fn snapshot(
    repo: &Path,
    profiles: Vec<u64>,
    hidden: Vec<String>,
    active: &dyn Fn() -> bool,
) -> Result<Repository, String> {
    let run = |args: &[&str]| {
        {
            if !active() {
                return Err("Export cancelled".into());
            }
            git::tour_read(repo, args, None)
        }
        .and_then(|b| String::from_utf8(b).map_err(|_| "Repository metadata is not UTF-8".into()))
    };
    let label = repo
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>();
    let name = if leaf(&label).is_ok() {
        label
    } else {
        "repository".into()
    };
    let mut out = Repository {
        config: super::config::snapshot(repo, active)?,
        name,
        previous_path: repo.to_string_lossy().into_owned(),
        profile_ids: profiles,
        remotes: Vec::new(),
        author_name: git::onboarding_local_config(repo, "user.name", active)?,
        author_email: git::onboarding_local_config(repo, "user.email", active)?,
        hidden,
        branch: None,
        commit: None,
        worktrees: Vec::new(),
        notices: Vec::new(),
    };
    let head = run(&["rev-parse", "--revs-only", "HEAD"])?;
    if !head.trim().is_empty() {
        out.commit = Some(head.trim().into());
    }
    let refs = run(&["for-each-ref", "--format=%(refname) %(HEAD)", "refs/heads"])?;
    out.branch = refs.lines().find_map(|l| {
        l.strip_suffix(" *")
            .and_then(|s| s.strip_prefix("refs/heads/"))
            .map(str::to_owned)
    });
    if out.commit.is_none() {
        let symbolic = run(&["symbolic-ref", "--short", "HEAD"])?;
        out.branch = Some(symbolic.trim().into());
    }
    let remotes = run(&["remote"])?;
    if remotes.lines().count() > 32 {
        return Err("More than 32 remotes; narrow the repository setup first".into());
    }
    for remote in remotes.lines() {
        reference(remote)?;
        let fetch = run(&["remote", "get-url", "--all", remote])?;
        let push = run(&["remote", "get-url", "--push", "--all", remote])?;
        let Some(first) = fetch.lines().next() else {
            continue;
        };
        let (fetch, changed) = match sanitize_url(first) {
            Ok(url) => url,
            Err(_) => {
                out.notices.push(format!("{remote}: local/unsupported source omitted; choose a current-machine local source on import"));
                continue;
            }
        };
        if changed {
            out.notices.push(format!(
                "{remote}: credential material/query/fragment removed from fetch URL"
            ));
        }
        let mut push_urls = Vec::new();
        if push.lines().count() > 16 {
            return Err("More than 16 push URLs; narrow the repository setup first".into());
        }
        for value in push.lines() {
            match sanitize_url(value) {
                Ok((url, changed)) => {
                    if changed {
                        out.notices.push(format!(
                            "{remote}: credential material removed from push URL"
                        ));
                    }
                    push_urls.push(url);
                }
                Err(_) => out
                    .notices
                    .push(format!("{remote}: unsupported push URL omitted")),
            }
        }
        out.remotes.push(Remote {
            name: remote.into(),
            fetch,
            push: push_urls,
        });
    }
    let work = run(&["worktree", "list", "--porcelain", "-z"])?;
    let encoded = |path: &str| {
        crate::remote::parse(repo).map_or_else(
            || std::path::PathBuf::from(path),
            |t| crate::remote::encode(&t.host, path),
        )
    };
    let same = crate::remote::is_same_dir_as(repo);
    let mut current: Option<Worktree> = None;
    let mut skip = false;
    let mut number = 0;
    for field in work.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            if let Some(w) = current.take() {
                out.worktrees.push(w);
            }
            number += 1;
            skip = same(&encoded(path));
            if skip {
                continue;
            }
            let label = Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                        c
                    } else {
                        '_'
                    }
                })
                .take(70)
                .collect::<String>();
            let name = if leaf(&label).is_ok() {
                format!("{label}-{number}")
            } else {
                format!("worktree-{number}")
            };
            current = Some(Worktree {
                name,
                previous_path: path.into(),
                branch: None,
                commit: String::new(),
                was_locked: false,
            });
        } else if !skip && let Some(w) = &mut current {
            if let Some(head) = field.strip_prefix("HEAD ") {
                w.commit = head.into();
            }
            if let Some(branch) = field.strip_prefix("branch refs/heads/") {
                w.branch = Some(branch.into());
            }
            if field == "locked" || field.starts_with("locked ") {
                w.was_locked = true;
            }
            if field == "bare" {
                current = None;
                skip = true;
                out.notices
                    .push("A bare worktree descriptor was omitted".into());
            }
        }
    }
    if let Some(w) = current {
        out.worktrees.push(w);
    }
    if out.worktrees.len() > 64 {
        return Err("More than 64 linked worktrees; narrow the export".into());
    }
    out.validate()?;
    Ok(out)
}
