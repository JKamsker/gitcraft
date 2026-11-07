//! A first-day repository page. Everything here is observed from a bounded snapshot; it is
//! never an inferred contribution policy, and no project hook or script is executed.
#[cfg(test)]
mod tests;
use crate::git::{
    self,
    activity::{Day, History},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, atomic::AtomicUsize},
};

#[derive(Clone, Debug)]
pub(crate) struct Document {
    pub path: String,
    pub text: String,
    pub truncated: bool,
}
#[derive(Default, Debug)]
pub(crate) struct Workflow {
    pub inspected: usize,
    pub merges: usize,
    pub conventional: usize,
    pub signature_headers: usize,
    pub signoffs: usize,
    pub incomplete: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Folder {
    pub path: Vec<u8>,
    pub authors: Vec<(String, String, usize)>,
}
pub(crate) struct Tour {
    pub history: Arc<History>,
    pub head: String,
    pub documents: Vec<Document>,
    pub documents_unavailable: Vec<String>,
    pub workflow: Workflow,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
    pub folders: Vec<Folder>,
    pub hot_files: Vec<(Vec<u8>, usize)>,
    pub since: Day,
    pub through: Day,
    pub recent_commits: usize,
    pub needs: BTreeSet<String>,
    pub identity: (String, String),
}

pub(crate) fn read(
    repo: &Path,
    history: Option<Arc<History>>,
    active: &dyn Fn() -> bool,
    progress: &AtomicUsize,
    now_days: i64,
) -> Result<Tour, String> {
    let history = match history {
        Some(h) => h,
        None => Arc::new(git::activity::read(repo, 5_000, active, progress)?),
    };
    let run = |args: &[&str], input: Option<&[u8]>| git::activity::run(repo, args, input, active);
    let head = run(&["rev-parse", "--revs-only", "HEAD"], None)?;
    let head = String::from_utf8(head)
        .map_err(|_| "Invalid repository HEAD")?
        .trim()
        .to_owned();
    let since = Day::from_unix_days(now_days - 89).ok_or("Unsupported current calendar day")?;
    let through = Day::from_unix_days(now_days).ok_or("Unsupported current calendar day")?;
    let (folders, hot_files, recent_commits) = recent(&history, since, through, active)?;
    let mut tour = Tour {
        workflow: workflow(repo, &history, active),
        history,
        head,
        documents: Vec::new(),
        documents_unavailable: Vec::new(),
        branches: Vec::new(),
        tags: Vec::new(),
        folders,
        hot_files,
        since,
        through,
        recent_commits,
        needs: BTreeSet::new(),
        identity: (String::new(), String::new()),
    };
    let refs = run(
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
        None,
    )?;
    for name in String::from_utf8_lossy(&refs).lines().take(2000) {
        if let Some(name) = name.strip_prefix("refs/tags/") {
            tour.tags.push(name.to_owned());
        } else {
            tour.branches.push(name.to_owned());
        }
    }
    tour.identity.0 = git::onboarding_config(repo, "user.name", active)?.unwrap_or_default();
    tour.identity.1 = git::onboarding_config(repo, "user.email", active)?.unwrap_or_default();
    if tour.head.is_empty() {
        return Ok(tour);
    }
    let tree = run(
        &["--no-lazy-fetch", "ls-tree", "-r", "-z", &tour.head],
        None,
    )?;
    // Prefer the first-day README and policy documents even when a repository has many
    // templates/attribute files. The whole tree listing is bounded by the shared reader.
    let mut records = tree
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .collect::<Vec<_>>();
    records.sort_by_key(|record| {
        let path = record.split(|b| *b == b'\t').nth(1).unwrap_or_default();
        let lower = String::from_utf8_lossy(path).to_ascii_lowercase();
        if lower.starts_with("readme") {
            0
        } else if lower.contains("contributing") {
            1
        } else if lower.contains("pull_request_template") {
            2
        } else {
            3
        }
    });
    let mut documents = 0;
    for record in records {
        if !active() {
            return Err("Repository tour cancelled".into());
        }
        let Some(tab) = record.iter().position(|b| *b == b'\t') else {
            return Err("Invalid repository tree record".into());
        };
        let metadata = &record[..tab];
        let path = std::str::from_utf8(&record[tab + 1..]).unwrap_or_default();
        let lower = path.to_ascii_lowercase();
        if path == ".gitmodules" || metadata.starts_with(b"160000 ") {
            tour.needs.insert("Submodules".into());
        }
        if path == ".editorconfig" {
            tour.needs.insert("EditorConfig".into());
        }
        if path.starts_with(".husky/") {
            tour.needs
                .insert("Husky hooks (review project instructions)".into());
        }
        if lower.contains("lefthook") {
            tour.needs
                .insert("Lefthook hooks (review project instructions)".into());
        }
        if path == ".pre-commit-config.yaml" {
            tour.needs
                .insert("pre-commit hooks (review project instructions)".into());
        }
        let document = lower.starts_with("readme")
            || lower.contains("contributing")
            || lower.contains("pull_request_template")
            || [
                ".gitattributes",
                ".gitmodules",
                ".editorconfig",
                "package.json",
            ]
            .contains(&path)
            || path.ends_with("/.gitattributes");
        if !document {
            continue;
        }
        if documents >= 20 {
            if !tour
                .documents_unavailable
                .iter()
                .any(|s| s == "Additional documents omitted by the 20-document reading limit")
            {
                tour.documents_unavailable
                    .push("Additional documents omitted by the 20-document reading limit".into());
            }
            continue;
        }
        documents += 1;
        let Some(oid) = metadata.split(|b| *b == b' ').next_back() else {
            continue;
        };
        if !matches!(oid.len(), 40 | 64) || !oid.iter().all(u8::is_ascii_hexdigit) {
            return Err("Invalid document object ID".into());
        }
        let oid = std::str::from_utf8(oid).map_err(|_| "Invalid object ID")?;
        let read = || -> Result<Vec<u8>, String> {
            let size = run(&["--no-lazy-fetch", "cat-file", "-s", oid], None)?;
            let size = std::str::from_utf8(&size)
                .map_err(|_| "Invalid document size")?
                .trim()
                .parse::<usize>()
                .map_err(|_| "Invalid document size")?;
            if size > 256 * 1024 {
                return Err("Document is larger than the 256 KiB reading limit".into());
            }
            run(&["--no-lazy-fetch", "cat-file", "blob", oid], None)
        };
        match read() {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                if (path == ".gitattributes" || path.ends_with("/.gitattributes"))
                    && text.lines().any(|l| {
                        !l.trim_start().starts_with('#')
                            && l.split_whitespace().any(|s| s == "filter=lfs")
                    })
                {
                    tour.needs.insert("Git LFS".into());
                }
                if path == "package.json" && text.contains("husky") {
                    tour.needs
                        .insert("Husky hooks (review project instructions)".into());
                }
                if path == "package.json" {
                    continue;
                }
                let excerpt: String = text.chars().take(6000).collect();
                let truncated = excerpt.len() < text.len();
                tour.documents.push(Document {
                    path: path.into(),
                    text: excerpt,
                    truncated,
                });
            }
            Err(_) => tour.documents_unavailable.push(format!(
                "{path}: content unavailable locally or above the reading limit (not downloaded)"
            )),
        }
    }
    Ok(tour)
}

type Identity = (String, String);
type FolderCounts = BTreeMap<Vec<u8>, BTreeMap<Identity, usize>>;
fn recent(
    history: &History,
    since: Day,
    through: Day,
    active: &dyn Fn() -> bool,
) -> Result<(Vec<Folder>, Vec<(Vec<u8>, usize)>, usize), String> {
    let mut folders: FolderCounts = BTreeMap::new();
    let mut files: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut count = 0;
    for c in &history.commits {
        if !active() {
            return Err("Repository tour cancelled".into());
        }
        if c.committer < since || c.committer > through {
            continue;
        }
        count += 1;
        let mut touched = BTreeSet::new();
        for path in &c.paths {
            *files.entry(path.clone()).or_default() += 1;
            let folder = path
                .iter()
                .position(|b| *b == b'/')
                .map_or_else(|| b"(root files)".to_vec(), |at| path[..at].to_vec());
            touched.insert(folder);
        }
        for folder in touched {
            *folders
                .entry(folder)
                .or_default()
                .entry((c.name.clone(), c.email.clone()))
                .or_default() += 1;
        }
    }
    let folders = folders
        .into_iter()
        .take(100)
        .map(|(path, authors)| {
            let mut authors: Vec<_> = authors
                .into_iter()
                .map(|((name, email), n)| (name, email, n))
                .collect();
            authors.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
            authors.truncate(5);
            Folder { path, authors }
        })
        .collect();
    let mut hot: Vec<_> = files.into_iter().collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    hot.truncate(25);
    Ok((folders, hot, count))
}

fn workflow(repo: &Path, history: &History, active: &dyn Fn() -> bool) -> Workflow {
    let mut result = Workflow::default();
    for chunk in history.commits.chunks(100) {
        if !active() {
            result.incomplete = Some("Analysis cancelled".into());
            break;
        }
        let input: Vec<u8> = chunk
            .iter()
            .flat_map(|c| c.hash.bytes().chain(std::iter::once(b'\n')))
            .collect();
        match git::activity::run(
            repo,
            &["--no-lazy-fetch", "cat-file", "--batch"],
            Some(&input),
            active,
        )
        .and_then(|bytes| inspect_objects(&bytes, chunk.len(), &mut result))
        {
            Ok(()) => (),
            Err(_) => {
                result.incomplete=Some("Some commit metadata was unavailable locally or exceeded the reading limit; it was not downloaded".into());
                break;
            }
        }
    }
    result
}

fn inspect_objects(mut bytes: &[u8], expected: usize, out: &mut Workflow) -> Result<(), String> {
    for _ in 0..expected {
        let end = bytes
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("Incomplete commit object header")?;
        let header = std::str::from_utf8(&bytes[..end]).map_err(|_| "Invalid commit header")?;
        let mut parts = header.split(' ');
        let oid = parts.next().ok_or("Missing commit ID")?;
        if !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|b| b.is_ascii_hexdigit())
            || parts.next() != Some("commit")
        {
            return Err("Commit object is unavailable".into());
        }
        let size = parts
            .next()
            .ok_or("Missing object size")?
            .parse::<usize>()
            .map_err(|_| "Invalid object size")?;
        bytes = &bytes[end + 1..];
        let object = bytes.get(..size).ok_or("Truncated commit object")?;
        let sep = object
            .windows(2)
            .position(|b| b == b"\n\n")
            .ok_or("Missing commit message")?;
        let headers = &object[..sep];
        let body = String::from_utf8_lossy(&object[sep + 2..]);
        out.inspected += 1;
        out.merges += usize::from(
            headers
                .split(|b| *b == b'\n')
                .filter(|l| l.starts_with(b"parent "))
                .count()
                > 1,
        );
        out.signature_headers += usize::from(
            headers
                .split(|b| *b == b'\n')
                .any(|l| l.starts_with(b"gpgsig ") || l.starts_with(b"gpgsig-sha256 ")),
        );
        out.signoffs += usize::from(
            body.lines()
                .any(|l| l.to_ascii_lowercase().starts_with("signed-off-by:")),
        );
        let subject = body.lines().next().unwrap_or_default();
        let kind = subject
            .split_once(": ")
            .map(|(kind, _)| kind)
            .unwrap_or_default();
        let kind = kind.trim_end_matches('!');
        let conventional =
            kind.split_once('(').map_or(
                kind,
                |(prefix, scope)| if scope.ends_with(')') { prefix } else { "" },
            );
        out.conventional += usize::from(
            !conventional.is_empty() && conventional.bytes().all(|b| b.is_ascii_lowercase()),
        );
        bytes = bytes
            .get(size.checked_add(1).ok_or("Invalid commit size")?..)
            .ok_or("Missing commit record separator")?;
    }
    if !bytes.is_empty() {
        return Err("Unexpected commit object output".into());
    }
    Ok(())
}
