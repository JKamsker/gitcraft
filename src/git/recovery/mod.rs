//! Remembered text in raw Git objects, without checking out, fetching or filtering data.
mod batch;
mod file;
mod recover;
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

pub(super) struct Reader<'a> {
    pub repo: &'a Path,
    active: &'a dyn Fn() -> bool,
    began: Instant,
}
impl<'a> Reader<'a> {
    pub fn new(repo: &'a Path, active: &'a dyn Fn() -> bool) -> Self {
        Self {
            repo,
            active,
            began: Instant::now(),
        }
    }
    pub fn active(&self) -> bool {
        (self.active)() && self.began.elapsed() < Duration::from_secs(120)
    }
    pub fn output(&self, args: &[&str], input: Option<&[u8]>) -> Result<remote::Output, String> {
        let null = if remote::is_remote(self.repo) || !cfg!(windows) {
            "/dev/null"
        } else {
            "NUL"
        };
        let hooks = format!("core.hooksPath={null}");
        let mut pre = vec![
            "--no-lazy-fetch",
            "--no-replace-objects",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            &hooks,
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "commit.gpgSign=false",
        ];
        pre.extend_from_slice(args);
        let remaining = Duration::from_secs(120).saturating_sub(self.began.elapsed());
        git::command::recovery_output(
            self.repo,
            &pre,
            input,
            &|| self.active(),
            remaining.min(Duration::from_secs(30)),
        )
    }
    pub fn run(&self, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let out = self.output(args, input)?;
        if out.code == 0 && out.stderr.is_empty() {
            Ok(out.stdout)
        } else {
            Err(format!(
                "Content recovery Git failed ({}): {}",
                out.code,
                String::from_utf8_lossy(&out.stderr)
                    .chars()
                    .take(2000)
                    .collect::<String>()
            ))
        }
    }
    pub fn identity(&self) -> Result<Identity, String> {
        let text = |args: &[&str]| -> Result<String, String> {
            let bytes = self.run(args, None)?;
            let bytes = bytes
                .strip_suffix(b"\n")
                .ok_or("Truncated repository metadata")?;
            let bytes = if cfg!(windows) && !remote::is_remote(self.repo) {
                bytes.strip_suffix(b"\r").unwrap_or(bytes)
            } else {
                bytes
            };
            let value =
                std::str::from_utf8(bytes).map_err(|_| "Repository metadata is not UTF-8")?;
            if value.chars().any(char::is_control) {
                return Err(
                    "Repository metadata paths contain unsupported control characters".into(),
                );
            }
            Ok(value.to_owned())
        };
        let bare = text(&["rev-parse", "--is-bare-repository"])? == "true";
        let identity = Identity {
            host: remote::parse(self.repo).map(|target| target.host),
            common: text(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?,
            root: if bare {
                None
            } else {
                Some(text(&["rev-parse", "--show-toplevel"])?)
            },
            format: text(&["rev-parse", "--show-object-format"])?,
        };
        if !matches!(identity.format.as_str(), "sha1" | "sha256") {
            return Err("Unsupported object format".into());
        }
        Ok(identity)
    }
}

pub(crate) fn search(
    repo: &Path,
    query: &str,
    active: &dyn Fn() -> bool,
) -> Result<Results, String> {
    if query.trim().is_empty() || query.len() > 4096 || query.contains('\0') {
        return Err("Enter 1–4096 bytes of literal text to find".into());
    }
    // The worker's captured profile remains pinned across every subprocess and remote hop.
    let context = crate::connections::current();
    context.run(|| search_inner(repo, query, active))
}
fn search_inner(repo: &Path, query: &str, active: &dyn Fn() -> bool) -> Result<Results, String> {
    let reader = Reader::new(repo, active);
    let identity = reader.identity()?;
    let mut report = Report {
        shallow: reader.run(&["rev-parse", "--is-shallow-repository"], None)? == b"true\n",
        ..Default::default()
    };
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
        return Err(format!(
            "Recovery scan could not verify the local object store: {}",
            String::from_utf8_lossy(&fsck.stderr)
                .chars()
                .take(2000)
                .collect::<String>()
        ));
    }
    let diagnostics =
        std::str::from_utf8(&fsck.stderr).map_err(|_| "Invalid object scan diagnostics")?;
    if diagnostics
        .lines()
        .any(|line| !line.starts_with("notice: "))
    {
        return Err(
            "Object scan returned warnings; inspect the repository with Git before recovery".into(),
        );
    }
    if !diagnostics.is_empty() {
        report.notes.push(diagnostics.chars().take(1000).collect());
    }
    let mut commits = BTreeSet::new();
    let mut unreachable = BTreeSet::<String>::new();
    let mut blobs: BTreeMap<String, Vec<Provenance>> = BTreeMap::new();
    for line in std::str::from_utf8(&fsck.stdout)
        .map_err(|_| "Invalid object scan output")?
        .lines()
    {
        let parts: Vec<_> = line.split_whitespace().collect();
        let [kind, ty, oid] = parts.as_slice() else {
            return Err("Malformed object scan record".into());
        };
        if !matches!(*kind, "unreachable" | "dangling") || !valid_oid(oid) {
            return Err("Malformed object scan identity".into());
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
            _ => return Err("Unsupported object scan type".into()),
        }
    }
    let reflogs = reader.run(&["rev-list", "--max-count=501", "--reflog", "--all"], None)?;
    for oid in std::str::from_utf8(&reflogs)
        .map_err(|_| "Invalid reflog walk")?
        .lines()
    {
        if !valid_oid(oid) {
            return Err("Invalid reflog commit ID".into());
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
        .map_err(|_| "Invalid stash reflog")?
        .lines()
        {
            if !valid_oid(oid) {
                return Err("Invalid stash commit ID".into());
            }
            live.insert(oid.to_owned());
        }
    } else if stash.code != 1 || !stash.stderr.is_empty() {
        return Err("Cannot inspect the current stash ref safely".into());
    }
    let all = read_commits(&reader, &commits, &mut report)?;
    let parents: BTreeSet<_> = all
        .values()
        .filter(|commit| {
            matches!(commit.parents.len(), 2 | 3)
                && (commit.subject.starts_with("WIP on ") || commit.subject.starts_with("On "))
        })
        .flat_map(|commit| commit.parents.iter().skip(1).cloned())
        .collect();
    let extra = read_commits(&reader, &parents, &mut report)?;
    let mut trees = BTreeMap::<String, Vec<(String, String, String)>>::new();
    let mut entries = 0;
    let mut ordered: Vec<_> = all.values().collect();
    ordered.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| a.oid.cmp(&b.oid)));
    for commit in ordered {
        let dropped = stash_shape(commit, &extra);
        let origin = if dropped {
            if live.contains(&commit.oid) {
                Origin::LiveStash
            } else {
                Origin::DroppedStash
            }
        } else if unreachable.contains(&commit.oid) {
            Origin::Unreachable
        } else {
            Origin::History
        };
        let mut layers = vec![(&commit.tree, "working")];
        if dropped {
            for (i, parent) in commit.parents.iter().enumerate().skip(1) {
                if let Some(parent) = extra.get(parent) {
                    layers.push((&parent.tree, if i == 1 { "staged" } else { "untracked" }));
                }
            }
        }
        for (tree, layer) in layers {
            if !reader.active() {
                return Err("Content recovery cancelled or timed out".into());
            }
            if !trees.contains_key(tree) {
                trees.insert(tree.clone(), tree_entries(&reader, tree, &mut report)?);
            }
            for (path, mode, blob) in &trees[tree] {
                if entries >= ENTRIES || (blobs.len() >= OBJECTS && !blobs.contains_key(blob)) {
                    report.limited = true;
                    break;
                }
                entries += 1;
                let sources = blobs.entry(blob.clone()).or_default();
                if sources.len() >= 50 {
                    report.limited = true;
                    continue;
                }
                sources.push(Provenance {
                    commit: commit.oid.clone(),
                    tree: tree.clone(),
                    path: path.clone(),
                    mode: mode.clone(),
                    when: commit.when,
                    subject: commit.subject.clone(),
                    origin: origin.clone(),
                    layer,
                });
            }
            if entries >= ENTRIES {
                break;
            }
        }
        if entries >= ENTRIES {
            break;
        }
    }
    let ids: Vec<_> = blobs.keys().cloned().collect();
    let checked = batch::check(&reader, &ids)?;
    let mut requested = Vec::new();
    for item in checked {
        let Some((kind, size)) = item.info else {
            report.missing += 1;
            continue;
        };
        if kind != "blob" {
            return Err("An expected content object changed type".into());
        }
        if size > BLOB_LIMIT {
            report.large += 1;
            continue;
        }
        if report.bytes + size > BYTE_LIMIT {
            report.limited = true;
            continue;
        }
        report.bytes += size;
        requested.push((item.oid, size));
    }
    for group in batch::groups(&requested) {
        for (oid, bytes) in batch::read(&reader, group, "blob")? {
            report.scanned += 1;
            let text = match std::str::from_utf8(&bytes) {
                Ok(text) if !text.contains('\0') => text,
                _ => {
                    report.binary += 1;
                    continue;
                }
            };
            if let Some(at) = text.find(query) {
                let (line, excerpt) = excerpt(text, at);
                let occurrences = text.match_indices(query).take(1001).count();
                if report.hits.len() >= HITS {
                    report.limited = true;
                    continue;
                }
                let mut provenance = blobs.remove(&oid).unwrap_or_default();
                provenance.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| a.path.cmp(&b.path)));
                report.hits.push(Hit {
                    blob: oid,
                    size: bytes.len(),
                    line,
                    excerpt,
                    occurrences,
                    provenance,
                });
            }
        }
    }
    report
        .hits
        .sort_by(|a, b| b.when().cmp(&a.when()).then_with(|| a.blob.cmp(&b.blob)));
    if reader.identity()? != identity {
        return Err("Repository identity changed during search; search again".into());
    }
    Ok(Results {
        identity,
        query: query.to_owned(),
        report,
    })
}
fn read_commits(
    reader: &Reader<'_>,
    ids: &BTreeSet<String>,
    report: &mut Report,
) -> Result<BTreeMap<String, Commit>, String> {
    let ids: Vec<_> = ids.iter().cloned().collect();
    let mut requests = Vec::new();
    for item in batch::check(reader, &ids)? {
        match item.info {
            Some((kind, size)) if kind == "commit" && size <= 256 * 1024 => {
                requests.push((item.oid, size))
            }
            None => report.missing += 1,
            _ => {
                report.limited = true;
            }
        }
    }
    let mut commits = BTreeMap::new();
    for group in batch::groups(&requests) {
        for (oid, bytes) in batch::read(reader, group, "commit")? {
            commits.insert(oid.clone(), parse_commit(&oid, &bytes)?);
        }
    }
    Ok(commits)
}
fn parse_commit(oid: &str, bytes: &[u8]) -> Result<Commit, String> {
    let split = bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .ok_or("Malformed commit object")?;
    let mut tree = None;
    let mut parents = Vec::new();
    let mut when = None;
    for line in bytes[..split].split(|b| *b == b'\n') {
        if let Some(value) = line.strip_prefix(b"tree ") {
            let value = std::str::from_utf8(value).map_err(|_| "Invalid commit tree")?;
            if tree.is_some() || !valid_oid(value) {
                return Err("Invalid commit tree".into());
            }
            tree = Some(value.to_owned());
        }
        if let Some(value) = line.strip_prefix(b"parent ") {
            let value = std::str::from_utf8(value).map_err(|_| "Invalid commit parent")?;
            if !valid_oid(value) || parents.len() >= 64 {
                return Err("Invalid commit parents".into());
            }
            parents.push(value.to_owned());
        }
        if let Some(value) = line.strip_prefix(b"committer ") {
            when = value
                .rsplit(|b| b.is_ascii_whitespace())
                .filter(|part| !part.is_empty())
                .nth(1)
                .and_then(|time| std::str::from_utf8(time).ok())
                .and_then(|time| time.parse::<i64>().ok());
        }
    }
    let message = bytes.get(split + 2..).ok_or("Missing commit message")?;
    let subject = message.split(|b| *b == b'\n').next().unwrap_or_default();
    Ok(Commit {
        oid: oid.into(),
        tree: tree.ok_or("Missing commit tree")?,
        parents,
        when: when.ok_or("Missing commit time")?,
        subject: String::from_utf8_lossy(subject)
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect(),
    })
}
fn stash_shape(commit: &Commit, parents: &BTreeMap<String, Commit>) -> bool {
    matches!(commit.parents.len(), 2 | 3)
        && (commit.subject.starts_with("WIP on ") || commit.subject.starts_with("On "))
        && parents.get(&commit.parents[1]).is_some_and(|index| {
            index.parents == commit.parents[..1] && index.subject.starts_with("index on ")
        })
        && (commit.parents.len() == 2
            || parents.get(&commit.parents[2]).is_some_and(|untracked| {
                untracked.parents.is_empty() && untracked.subject.starts_with("untracked files on ")
            }))
}
fn tree_entries(
    reader: &Reader<'_>,
    tree: &str,
    report: &mut Report,
) -> Result<Vec<(String, String, String)>, String> {
    let bytes = reader.run(&["ls-tree", "-r", "-z", "--full-tree", tree], None)?;
    let mut entries = Vec::new();
    for row in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let Some(tab) = row.iter().position(|b| *b == b'\t') else {
            return Err("Invalid tree record".into());
        };
        let info = std::str::from_utf8(&row[..tab]).map_err(|_| "Invalid tree metadata")?;
        let parts: Vec<_> = info.split_whitespace().collect();
        let [mode, kind, oid] = parts.as_slice() else {
            return Err("Invalid tree entry".into());
        };
        if !valid_oid(oid) {
            return Err("Invalid tree object ID".into());
        }
        if *kind != "blob" {
            continue;
        }
        let Ok(path) = std::str::from_utf8(&row[tab + 1..]) else {
            report.limited = true;
            continue;
        };
        if path.len() > 4096 || path.chars().any(char::is_control) {
            report.limited = true;
            continue;
        }
        if !matches!(*mode, "100644" | "100755" | "120000") {
            return Err("Invalid blob mode".into());
        }
        if entries.len() >= ENTRIES {
            return Err(
                "A tree exceeds 15000 file entries; narrow recovery with Git in the terminal"
                    .into(),
            );
        }
        entries.push((path.into(), (*mode).into(), (*oid).into()));
    }
    Ok(entries)
}
fn excerpt(text: &str, at: usize) -> (usize, String) {
    let line = text[..at].bytes().filter(|b| *b == b'\n').count() + 1;
    let start = text[..at]
        .char_indices()
        .rev()
        .nth(120)
        .map_or(0, |(i, _)| i);
    let mut excerpt: String = text[start..].chars().take(400).collect();
    if start > 0 {
        excerpt.insert(0, '…');
    }
    if text[start..].chars().count() > 400 {
        excerpt.push('…');
    }
    (line, excerpt)
}
pub(super) fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}
