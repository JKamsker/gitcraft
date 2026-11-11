//! Remembered text in raw Git objects, without checking out, fetching or filtering data.
mod batch;
mod file;
mod provenance;
mod recover;
mod scan;
pub(crate) use recover::{Prepared, Preview, Target, inspect, perform, prepare};
#[cfg(test)]
mod tests;

use crate::{git, remote};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

pub(crate) const BLOB_LIMIT: usize = 2 * 1024 * 1024;
const BYTE_LIMIT: usize = 64 * 1024 * 1024;
const COMMITS: usize = 500;
const OBJECTS: usize = 10_000;
const ENTRIES: usize = 15_000;
const HITS: usize = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    History,
    Unreachable,
    DroppedStash,
    LiveStash,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Provenance {
    pub commit: String,
    pub tree: String,
    pub path: String,
    pub mode: String,
    pub when: i64,
    pub subject: String,
    pub origin: Origin,
    pub layer: &'static str,
}
#[derive(Clone, Debug)]
pub(crate) struct Hit {
    pub blob: String,
    pub size: usize,
    pub line: usize,
    pub excerpt: String,
    pub occurrences: usize,
    pub provenance: Vec<Provenance>,
}
impl Hit {
    pub fn when(&self) -> Option<i64> {
        self.provenance.iter().map(|source| source.when).max()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub host: Option<remote::Host>,
    pub common: String,
    pub root: Option<String>,
    pub root_pin: Option<file::RootPin>,
    pub format: String,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Report {
    pub hits: Vec<Hit>,
    pub scanned: usize,
    pub bytes: usize,
    pub binary: usize,
    pub large: usize,
    pub missing: usize,
    pub limited: bool,
    pub shallow: bool,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Results {
    pub identity: Identity,
    pub query: String,
    pub report: Report,
}
#[derive(Clone, Debug)]
struct Commit {
    oid: String,
    tree: String,
    parents: Vec<String>,
    when: i64,
    subject: String,
}

pub(crate) fn search(
    repo: &Path,
    query: &str,
    active: &dyn Fn() -> bool,
) -> Result<Results, String> {
    if query.trim().is_empty() || query.len() > 4096 || query.contains('\0') {
        return Err(
            crate::i18n::text("backend-enter-1-4096-bytes-of-literal-text-to-find-d295ef").into(),
        );
    }
    // The worker's captured profile remains pinned across every subprocess and remote hop.
    search_inner(repo, query, active)
}
fn search_inner(repo: &Path, query: &str, active: &dyn Fn() -> bool) -> Result<Results, String> {
    let reader = Reader::new(repo, active);
    let identity = reader.identity()?;
    let mut report = Report {
        shallow: reader.run(&["rev-parse", "--is-shallow-repository"], None)? == b"true\n",
        ..Default::default()
    };
    if identity.root.is_some() && identity.root_pin.is_none() {
        report.notes.push(
            crate::i18n::text(
                "backend-safe-file-recovery-is-unavailable-because-this-machine-could-not-fb246d",
            )
            .into(),
        );
    }
    let fsck = reader.output(
        &[
            "fsck",
            "--connectivity-only",
            "--unreachable",
            "--no-reflogs",
            "--no-progress",
        ],
        None,
    )?;
    if fsck.code != 0 {
        return Err(crate::i18n::message(
            "backend-recovery-scan-could-not-verify-the-local-object-store-f4153b",
            &[(
                "from_utf8_lossy",
                (String::from_utf8_lossy(&fsck.stderr)
                    .chars()
                    .take(2000)
                    .collect::<String>())
                .to_string()
                .into(),
            )],
        ));
    }
    let diagnostics = std::str::from_utf8(&fsck.stderr)
        .map_err(|_| crate::i18n::text("backend-invalid-object-scan-diagnostics-7c43a7"))?;
    if diagnostics
        .lines()
        .any(|line| !line.starts_with("notice: "))
    {
        return Err(crate::i18n::text(
            "backend-object-scan-returned-warnings-inspect-the-repository-with-git-bef-456b39",
        )
        .into());
    }
    if !diagnostics.is_empty() {
        report.notes.push(diagnostics.chars().take(1000).collect());
    }
    let mut commits = BTreeSet::new();
    let mut unreachable = BTreeSet::<String>::new();
    let mut blobs: BTreeMap<String, Vec<Provenance>> = BTreeMap::new();
    for line in std::str::from_utf8(&fsck.stdout)
        .map_err(|_| crate::i18n::text("backend-invalid-object-scan-output-14d7dd"))?
        .lines()
    {
        let parts: Vec<_> = line.split_whitespace().collect();
        let [kind, ty, oid] = parts.as_slice() else {
            return Err(crate::i18n::text("backend-malformed-object-scan-record-724ff2").into());
        };
        if !matches!(*kind, "unreachable" | "dangling") || !valid_oid(oid) {
            return Err(crate::i18n::text("backend-malformed-object-scan-identity-6b6829").into());
        }
        match *ty {
            "commit" => {
                if commits.len() < COMMITS {
                    commits.insert((*oid).into());
                    unreachable.insert((*oid).into());
                } else {
                    report.limited = true;
                }
            }
            "blob" => {
                if blobs.len() < OBJECTS {
                    blobs.entry((*oid).into()).or_default();
                } else {
                    report.limited = true;
                }
            }
            "tree" | "tag" => (),
            _ => {
                return Err(
                    crate::i18n::text("backend-unsupported-object-scan-type-9f8f3f").into(),
                );
            }
        }
    }
    let reflogs = reader.run(&["rev-list", "--max-count=501", "--reflog", "--all"], None)?;
    for oid in std::str::from_utf8(&reflogs)
        .map_err(|_| crate::i18n::text("backend-invalid-reflog-walk-2f6ac0"))?
        .lines()
    {
        if !valid_oid(oid) {
            return Err(crate::i18n::text("backend-invalid-reflog-commit-id-d0b3b5").into());
        }
        if commits.len() < COMMITS || commits.contains(oid) {
            commits.insert(oid.into());
        } else {
            report.limited = true;
        }
    }
    let mut live = BTreeSet::new();
    let stash = reader.output(&["rev-parse", "--verify", "--quiet", "refs/stash"], None)?;
    if stash.code == 0 {
        for oid in std::str::from_utf8(&reader.run(
            &[
                "log",
                "-g",
                "--format=%H",
                "--no-show-signature",
                "--max-count=501",
                "refs/stash",
                "--",
            ],
            None,
        )?)
        .map_err(|_| crate::i18n::text("backend-invalid-stash-reflog-4b1662"))?
        .lines()
        {
            if !valid_oid(oid) {
                return Err(crate::i18n::text("backend-invalid-stash-commit-id-e3d52e").into());
            }
            live.insert(oid.to_owned());
        }
    } else if stash.code != 1 || !stash.stderr.is_empty() {
        return Err(crate::i18n::text(
            "backend-cannot-inspect-the-current-stash-ref-safely-8a96e7",
        )
        .into());
    }
    provenance::collect(
        &reader,
        &commits,
        &unreachable,
        &live,
        &mut blobs,
        &mut report,
    )?;
    scan::scan(&reader, query, blobs, &mut report)?;
    if reader.identity()? != identity {
        return Err(crate::i18n::text(
            "backend-repository-identity-changed-during-search-search-again-28cf73",
        )
        .into());
    }
    Ok(Results {
        identity,
        query: query.to_owned(),
        report,
    })
}

/// Shared reviewed filesystem pins for explicit, append-only repository-file actions.
/// No scan, ref/index mutation, filter, signing or lazy object download occurs here.
pub(crate) fn pinned_worktree(repo: &Path, active: &dyn Fn() -> bool) -> Result<Identity, String> {
    let identity = Reader::new(repo, active).identity()?;
    if identity.root.is_none() || identity.root_pin.is_none() {
        return Err(crate::i18n::text(
            "backend-a-stable-working-root-identity-is-required-for-this-file-action-753797",
        )
        .into());
    }
    Ok(identity)
}
pub(crate) use file::native_file_identity;
#[cfg(unix)]
pub(crate) use file::parent as pinned_parent;
#[cfg(windows)]
pub(crate) use file::windows_parent as pinned_parent;

mod objects;
use objects::excerpt;
#[cfg(test)]
use objects::parse_commit;
use objects::read_commits;
use objects::stash_shape;
use objects::tree_entries;
pub(super) use objects::valid_oid;

mod reader;
pub(super) use reader::Reader;
