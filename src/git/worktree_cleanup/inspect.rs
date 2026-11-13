//! Reading what a worktree holds and what keeps it, as it is now: the preview, and again just
//! before removing it.

use std::path::Path;
use std::time::{Duration, Instant};

use super::files::{classify, footprint, footprint_within, sha256_hex};
use super::{Blocker, Footprint, Inspection, Kind, Proof, Target, Untracked};
use crate::git::{self, Worktree};

/// In a worktree's own git folder: an operation in progress there.
const OPERATIONS: [&str; 7] = [
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
    "sequencer",
];

/// What `target` holds and what keeps it, `repo` being any worktree of the repository (the
/// tab's). An error only when git can't list the worktrees at all.
pub(crate) fn inspect(repo: &Path, target: &Target) -> Result<Inspection, String> {
    let mut found = Inspection {
        path: target.path.clone(),
        branch: target.branch.clone(),
        head: String::new(),
        changed: Vec::new(),
        untracked: Vec::new(),
        ignored: Vec::new(),
        ignored_size: Footprint::default(),
        footprint: Footprint::default(),
        blockers: Vec::new(),
        fingerprint: String::new(),
    };
    if crate::remote::is_remote(repo) || crate::remote::is_remote(&target.path) {
        found.blockers.push(Blocker::Remote);
        return Ok(found);
    }
    let listed = git::worktree_list(repo, None)?;
    let same = crate::remote::is_same_dir_as(&target.path);
    let Some(wt) = listed.iter().find(|w| same(&w.path)) else {
        found.blockers.push(Blocker::Missing);
        return Ok(found);
    };
    found.head = wt.head.clone();
    listed_blockers(wt, target, &mut found.blockers);
    if wt.prunable.is_some() || wt.bare || !target.path.is_dir() {
        return Ok(found);
    }
    let tip = branch_tip(repo, &target.branch);
    if tip.as_deref() != Some(target.evidence.tip.as_str()) || wt.head != target.evidence.tip {
        found.blockers.push(Blocker::Moved);
    } else {
        merge_blockers(repo, target, &mut found.blockers);
    }
    operation_blockers(&target.path, &mut found.blockers);
    read_files(&target.path, &mut found)?;
    if !found.changed.is_empty() {
        found.blockers.push(Blocker::Changed(found.changed.len()));
    }
    found.footprint = footprint(&target.path);
    found.fingerprint = fingerprint(&found, tip.as_deref().unwrap_or(""));
    Ok(found)
}

/// What the worktree list says keeps it.
fn listed_blockers(wt: &Worktree, target: &Target, out: &mut Vec<Blocker>) {
    if wt.main {
        out.push(Blocker::Primary);
    }
    if wt.bare {
        out.push(Blocker::Bare);
    }
    if let Some(reason) = &wt.locked {
        out.push(Blocker::Locked(reason.clone()));
    }
    if wt.prunable.is_some() {
        out.push(Blocker::Missing);
    }
    if wt.current {
        out.push(Blocker::InUse(
            crate::i18n::text("worktree-cleanup-in-use-this-tab").to_owned(),
        ));
    }
    if let Some(why) = &target.in_use {
        out.push(Blocker::InUse(why.clone()));
    }
    if wt.branch.as_deref() != Some(target.branch.as_str()) {
        out.push(Blocker::BranchChanged);
    }
}

fn branch_tip(repo: &Path, branch: &str) -> Option<String> {
    let rev = format!("refs/heads/{branch}^{{commit}}");
    git::run(Some(repo), &["rev-parse", "--verify", "--quiet", &rev])
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Whether the evidence still holds for the tip found merged.
fn merge_blockers(repo: &Path, target: &Target, out: &mut Vec<Blocker>) {
    let evidence = &target.evidence;
    let branch = format!("refs/heads/{}", target.branch);
    let holds = match evidence.proof {
        Proof::Ancestor => git::is_ancestor(repo, &evidence.tip, &evidence.base_ref),
        Proof::PatchEquivalent => {
            git::cherry_equivalent(repo, &evidence.base_ref, &branch).unwrap_or(false)
        }
        // Read from the forge with the tip it was found at, which is checked above.
        Proof::PullRequest(_) => true,
    };
    if !holds {
        out.push(Blocker::NotMerged);
        return;
    }
    if evidence.proof == Proof::Ancestor {
        let not = format!("^{}", evidence.base_ref);
        let count = git::run(
            Some(repo),
            &[
                "rev-list",
                "--count",
                &evidence.tip,
                &not,
                "--not",
                "--remotes",
            ],
        );
        match count.map(|c| c.trim().parse::<usize>()) {
            Ok(Ok(0)) => {}
            Ok(Ok(n)) => out.push(Blocker::Unpublished(n)),
            _ => out.push(Blocker::NotMerged),
        }
    }
}

/// An operation in progress, or git busy (its index locked), in the worktree.
fn operation_blockers(path: &Path, out: &mut Vec<Blocker>) {
    let Ok(dir) = git::run(Some(path), &["rev-parse", "--absolute-git-dir"]) else {
        out.push(Blocker::Missing);
        return;
    };
    let dir = Path::new(dir.trim());
    if OPERATIONS.iter().any(|name| dir.join(name).exists()) {
        out.push(Blocker::Operation);
    }
    if dir.join("index.lock").exists() {
        out.push(Blocker::InUse(
            crate::i18n::text("worktree-cleanup-in-use-git").to_owned(),
        ));
    }
}

/// Tracked changes, untracked files (each looked at) and ignored ones, from one `status`.
fn read_files(path: &Path, found: &mut Inspection) -> Result<(), String> {
    let out = git::run_bytes(
        Some(path),
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignored=matching",
        ],
    )?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ignored_size = Footprint {
        complete: true,
        ..Footprint::default()
    };
    let mut fields = out.split(|b| *b == 0);
    while let Some(field) = fields.next() {
        let [x, y, b' ', name @ ..] = field else {
            continue;
        };
        match (x, y) {
            (b'?', b'?') => found.untracked.push(untracked(path, name)),
            (b'!', b'!') => {
                let rel = String::from_utf8_lossy(name).into_owned();
                let size =
                    footprint_within(&path.join(rel.trim_end_matches('/')), deadline, 1 << 20);
                ignored_size.files += size.files;
                ignored_size.bytes += size.bytes;
                ignored_size.complete &= size.complete;
                found.ignored.push(rel);
            }
            _ => {
                found
                    .changed
                    .push(String::from_utf8_lossy(name).into_owned());
                // A rename or copy is followed by the path it came from.
                if [x, y].iter().any(|c| matches!(c, b'R' | b'C')) {
                    fields.next();
                }
            }
        }
    }
    found.ignored_size = ignored_size;
    Ok(())
}

fn untracked(root: &Path, name: &[u8]) -> Untracked {
    match std::str::from_utf8(name) {
        Ok(rel) => {
            let (size, kind) = classify(root, rel);
            Untracked {
                path: rel.to_owned(),
                size,
                kind,
            }
        }
        Err(_) => Untracked {
            path: String::from_utf8_lossy(name).into_owned(),
            size: 0,
            kind: Kind::Other,
        },
    }
}

/// HEAD, the tip, the changed files and every untracked file with what it is (text with its
/// checksum): what must not change between the preview and the removal.
fn fingerprint(found: &Inspection, tip: &str) -> String {
    let mut text = format!("{}\0{tip}\0", found.head);
    for path in &found.changed {
        text.push_str(&format!("c {path}\0"));
    }
    for file in &found.untracked {
        text.push_str(&format!(
            "u {}\0{}\0{:?}\0",
            file.path, file.size, file.kind
        ));
    }
    sha256_hex(text.as_bytes())
}
