//! Generic Git inventory, independent of merge-target eligibility and UI state.
use super::classify::Membership;
use super::{Completeness, Inventory, Measurement};
use crate::git;
use std::path::Path;
use std::time::Duration;

fn git_read(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git::command::bounded::local(root, args, Vec::new(), 4 << 20, Duration::from_secs(10))
}

pub(crate) fn collect(
    repo: &Path,
    path: &Path,
    generation: u64,
    presence: Option<String>,
    merged: Option<git::worktree_cleanup::Evidence>,
) -> Result<Inventory, String> {
    if crate::remote::is_remote(repo) || crate::remote::is_remote(path) {
        return Err(crate::i18n::text("resources-remote").into());
    }
    let listed = git::worktree_list(repo, None)?;
    let wt = listed
        .iter()
        .find(|w| w.path == path)
        .ok_or("Worktree no longer registered")?;
    let mut found = Inventory {
        root: wt.path.clone(),
        head: wt.head.clone(),
        branch: wt.branch.clone(),
        collected: git::now(),
        generation,
        staged: 0,
        unstaged: 0,
        conflicted: 0,
        untracked: 0,
        measurements: std::array::from_fn(|_| Measurement::default()),
        candidates: Vec::new(),
        blockers: Vec::new(),
        presence,
        fingerprint: String::new(),
        publication: "resources-local-refs".into(),
    };
    if wt.main || wt.current || wt.locked.is_some() || wt.bare || wt.prunable.is_some() {
        found.blockers.push("resources-registration-blocked".into());
    }
    if wt.prunable.is_some() || wt.bare {
        for m in &mut found.measurements {
            m.status = Completeness::Unavailable("resources-registration-blocked".into());
        }
        return Ok(found);
    }
    let refs = git_read(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname) %(upstream)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    use sha2::{Digest, Sha256};
    found.fingerprint = format!("{:?}", Sha256::digest(&refs));
    let status = git_read(
        path,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
    )?;
    let mut fields = status.split(|b| *b == 0);
    while let Some(field) = fields.next() {
        let [x, y, b' ', ..] = field else {
            continue;
        };
        if (*x, *y) == (b'?', b'?') {
            found.untracked += 1;
        } else {
            found.staged += usize::from(*x != b' ');
            found.unstaged += usize::from(*y != b' ');
            found.conflicted += usize::from(
                *x == b'U' || *y == b'U' || matches!((*x, *y), (b'A', b'A') | (b'D', b'D')),
            );
            if matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C') {
                fields.next();
            }
        }
    }
    let membership = Membership {
        tracked: Membership::insert(&git_read(path, &["ls-files", "-z", "--cached"])?)?,
        untracked: Membership::insert(&git_read(
            path,
            &["ls-files", "-z", "--others", "--exclude-standard"],
        )?)?,
        ignored: Membership::insert(&git_read(
            path,
            &[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
            ],
        )?)?,
    };
    // Local refs are evidence, never a claim that the remote was just contacted.
    let unpublished = git_read(path, &["rev-list", "--count", "HEAD", "--not", "--remotes"])?;
    if unpublished != b"0\n" {
        found.blockers.push("resources-unpublished".into());
    }
    found.blockers.extend(
        git::worktree_cleanup::resource_blockers(
            repo,
            path,
            found.branch.as_deref(),
            &found.head,
            merged.as_ref(),
        )
        .iter()
        .map(|b| b.describe()),
    );
    #[cfg(target_os = "linux")]
    if let Err(reason) = super::walk::measure(&mut found, &membership) {
        for m in &mut found.measurements {
            m.status = Completeness::Unavailable(reason.clone());
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let Membership {
            tracked,
            untracked,
            ignored,
        } = membership;
        drop((tracked, untracked, ignored));
        for measurement in &mut found.measurements {
            measurement.status = Completeness::Unsupported;
        }
        found.blockers.push("resources-platform".into());
    }
    let after = git_read(
        path,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
    )?;
    let current_refs = git_read(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname) %(upstream)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    if after != status || current_refs != refs || git::resolve(path, "HEAD")? != found.head {
        for m in &mut found.measurements {
            m.status = Completeness::Partial("resources-stale".into());
        }
    }
    Ok(found)
}
