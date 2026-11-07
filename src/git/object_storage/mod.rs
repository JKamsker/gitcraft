//! Read-only object representations. Git runs on the repository machine, without lazy fetch.
mod files;
pub(crate) mod independent;
#[cfg(test)]
mod tests;
mod topology;

use super::command::{activity_bytes_with_limit, comparison_output};
use crate::remote;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_PACKS: usize = 64;
const MAX_TREE: usize = 2_000;

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub name: String,
    pub oid: String,
    pub kind: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Packed {
    pub oid: String,
    pub kind: String,
    pub size: u64,
    pub disk: u64,
    pub offset: u64,
    pub depth: usize,
    pub base: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Representation {
    pub path: PathBuf,
    pub loose_bytes: Option<u64>,
    pub chain: Vec<Packed>,
}
#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub oid: String,
    pub kind: String,
    pub size: u64,
    pub disk: u64,
    pub base: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Inspection {
    pub query: String,
    pub format: String,
    pub git_dir: String,
    pub common_dir: String,
    pub objects: PathBuf,
    pub object: Option<Object>,
    pub missing: Option<String>,
    pub representations: Vec<Representation>,
    pub entries: Vec<Entry>,
    pub notes: Vec<String>,
    pub topology: topology::Topology,
    pub independence: Option<independent::Snapshot>,
}

pub(super) fn bytes(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let args: Vec<_> = [
        "--no-lazy-fetch",
        "--no-replace-objects",
        "--no-optional-locks",
    ]
    .into_iter()
    .chain(args.iter().copied())
    .collect();
    activity_bytes_with_limit(repo, &args, None, active, Duration::from_secs(20))
        .map_err(|e| format!("Object storage: {e}. This view requires Git with --no-lazy-fetch (2.45+) and verify-pack; install/update Git on the repository machine if unavailable."))
}
pub(super) fn text(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    String::from_utf8(bytes(repo, args, active)?)
        .map_err(|_| "Object storage metadata is not UTF-8; inspect this repository in Git.".into())
}
fn line(repo: &Path, args: &[&str], active: &dyn Fn() -> bool) -> Result<String, String> {
    Ok(text(repo, args, active)?
        .trim_end_matches(['\n', '\r'])
        .into())
}
pub(super) fn valid_oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn resolve(repo: &Path, query: &str, active: &dyn Fn() -> bool) -> Result<String, String> {
    if query.is_empty() || query.len() > 4096 || query.contains(['\n', '\r', '\0']) {
        return Err("Enter an object ID, revision, or revision:path of at most 4096 bytes (without line breaks).".into());
    }
    if valid_oid(query) {
        return Ok(query.to_ascii_lowercase());
    }
    let oid = line(
        repo,
        &["rev-parse", "--verify", "--end-of-options", query],
        active,
    )?;
    if !valid_oid(&oid) {
        return Err("Git did not resolve this expression to one full object ID.".into());
    }
    Ok(oid)
}
fn metadata(repo: &Path, oid: &str, active: &dyn Fn() -> bool) -> Result<Option<Object>, String> {
    let input = format!("{oid}\n");
    let out = super::command::bounded::run(
        repo,
        &[
            "--no-lazy-fetch",
            "--no-replace-objects",
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize) %(objectsize:disk) %(deltabase)",
        ],
        input.into_bytes(),
        4096,
    )?;
    if !active() {
        return Err("Object inspection cancelled".into());
    }
    let s = String::from_utf8(out).map_err(|_| "Invalid cat-file metadata")?;
    let fields: Vec<_> = s.split_whitespace().collect();
    if fields.len() == 2 && fields[0] == oid && fields[1] == "missing" {
        return Ok(None);
    }
    if fields.len() != 5 || fields[0] != oid || !valid_oid(fields[4]) {
        return Err(
            "Git returned unexpected object metadata; no storage claim can be made.".into(),
        );
    }
    let number = |s: &str| {
        s.parse::<u64>()
            .map_err(|_| "Invalid object size".to_owned())
    };
    Ok(Some(Object {
        oid: oid.into(),
        kind: fields[1].into(),
        size: number(fields[2])?,
        disk: number(fields[3])?,
        base: (!fields[4].bytes().all(|b| b == b'0')).then(|| fields[4].into()),
    }))
}

pub(super) fn pack_rows(output: &str) -> Result<BTreeMap<String, Packed>, String> {
    let mut rows = BTreeMap::new();
    for line in output.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.first().is_none_or(|s| !valid_oid(s)) {
            continue;
        }
        if !matches!(f.len(), 5 | 7) {
            return Err("Unexpected verify-pack record".into());
        }
        let parse = |s: &str| {
            s.parse::<u64>()
                .map_err(|_| "Invalid verify-pack number".to_owned())
        };
        let depth = if f.len() == 7 {
            usize::try_from(parse(f[5])?).map_err(|_| "Delta depth is too large")?
        } else {
            0
        };
        let base = if f.len() == 7 {
            if !valid_oid(f[6]) {
                return Err("Invalid delta base".into());
            }
            Some(f[6].to_owned())
        } else {
            None
        };
        rows.insert(
            f[0].into(),
            Packed {
                oid: f[0].into(),
                kind: f[1].into(),
                size: parse(f[2])?,
                disk: parse(f[3])?,
                offset: parse(f[4])?,
                depth,
                base,
            },
        );
    }
    Ok(rows)
}
fn representations(
    repo: &Path,
    store: &Path,
    oid: &str,
    budget: &mut usize,
    active: &dyn Fn() -> bool,
) -> Result<Vec<Representation>, String> {
    let mut found = Vec::new();
    let loose = remote::join(store, &format!("{}/{}", &oid[..2], &oid[2..]));
    if let Some(size) = files::size(&loose, active)? {
        found.push(Representation {
            path: loose,
            loose_bytes: Some(size),
            chain: Vec::new(),
        });
    }
    let packs = files::packs(store, *budget, active)?;
    *budget -= packs.len();
    for pack in packs {
        if !active() {
            return Err("Object inspection cancelled".into());
        }
        let printed = remote::git_path(&pack);
        let rows = pack_rows(&text(repo, &["verify-pack", "-v", "--", &printed], active)?)?;
        let Some(first) = rows.get(oid) else {
            continue;
        };
        let mut chain = vec![first.clone()];
        let mut seen = BTreeSet::from([oid.to_owned()]);
        while let Some(base) = chain.last().and_then(|r| r.base.as_ref()) {
            if chain.len() >= 128 || !seen.insert(base.clone()) {
                return Err("Cyclic or excessive delta chain; inspect the pack with Git.".into());
            }
            let row = rows.get(base).ok_or("A delta base is absent from its pack; storage may have changed or be corrupt. Refresh the inspector.")?;
            chain.push(row.clone());
        }
        let path = pack.with_extension("pack");
        found.push(Representation {
            path,
            loose_bytes: None,
            chain,
        });
    }
    Ok(found)
}
fn entries(repo: &Path, object: &Object, active: &dyn Fn() -> bool) -> Result<Vec<Entry>, String> {
    if object.kind == "tree" {
        let out = bytes(repo, &["ls-tree", "-z", &object.oid], active)?;
        let mut entries = Vec::new();
        for record in out.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            if entries.len() >= MAX_TREE {
                return Err("Tree exceeds 2,000 entries; inspect a subtree by object ID.".into());
            }
            let tab = record
                .iter()
                .position(|b| *b == b'\t')
                .ok_or("Invalid tree entry")?;
            let meta = std::str::from_utf8(&record[..tab]).map_err(|_| "Invalid tree metadata")?;
            let f: Vec<_> = meta.split_whitespace().collect();
            if f.len() != 3 || !valid_oid(f[2]) {
                return Err("Invalid tree object ID".into());
            }
            entries.push(Entry {
                name: String::from_utf8_lossy(&record[tab + 1..]).into_owned(),
                oid: f[2].into(),
                kind: f[1].into(),
            });
        }
        Ok(entries)
    } else if matches!(object.kind.as_str(), "commit" | "tag") {
        let out = super::command::bounded::run(
            repo,
            &[
                "--no-lazy-fetch",
                "--no-replace-objects",
                "cat-file",
                &object.kind,
                &object.oid,
            ],
            Vec::new(),
            128 * 1024,
        )?;
        let header = String::from_utf8_lossy(&out);
        Ok(header
            .lines()
            .take_while(|s| !s.is_empty())
            .filter_map(|s| {
                let (name, oid) = s.split_once(' ')?;
                (matches!(name, "tree" | "parent" | "object") && valid_oid(oid)).then(|| Entry {
                    name: name.into(),
                    oid: oid.into(),
                    kind: "object".into(),
                })
            })
            .collect())
    } else {
        Ok(Vec::new())
    }
}

pub(crate) fn inspect(
    repo: &Path,
    query: &str,
    active: &dyn Fn() -> bool,
) -> Result<Inspection, String> {
    let oid = resolve(repo, query, active)?;
    let format = line(repo, &["rev-parse", "--show-object-format"], active)?;
    if !matches!(format.as_str(), "sha1" | "sha256")
        || oid.len() != if format == "sha256" { 64 } else { 40 }
    {
        return Err("The object ID does not match this repository's object format.".into());
    }
    let git_dir = line(repo, &["rev-parse", "--absolute-git-dir"], active)?;
    let common_dir = line(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        active,
    )?;
    let objects = remote::resolve(
        repo,
        &line(
            repo,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "objects",
            ],
            active,
        )?,
    );
    let object = metadata(repo, &oid, active)?;
    let entries = match &object {
        Some(o) => entries(repo, o, active)?,
        None => Vec::new(),
    };
    let topology = topology::read(repo, &objects, active)?;
    let mut representations = Vec::new();
    let mut budget = MAX_PACKS;
    for store in topology.stores.iter().filter(|s| s.available) {
        representations.extend(self::representations(
            repo,
            &store.path,
            &oid,
            &mut budget,
            active,
        )?);
    }
    let mut notes = topology.warnings.clone();
    if topology.promisor {
        notes.push("Promisor/partial-clone configuration is present. Missing objects are not fetched, and independence is unavailable until the repository is complete.".into());
    }
    let independence = independent::prepare(repo, &objects, &topology, active)
        .map_err(|e| notes.push(e))
        .ok();
    let missing = object.is_none().then(|| format!("{oid} is missing from the available object stores. No download was attempted; a promisor configuration alone does not prove that this particular object is promised."));
    if object.is_some() && representations.is_empty() {
        notes.push("Git can read this object, but no representation was found in the primary store. It may be borrowed from an alternate; inspect shared and borrowed objects.".into());
    }
    let snapshot = comparison_output(repo, &["rev-parse", "--verify", "HEAD"], &[], active)?;
    if snapshot.code != 0 {
        notes.push("HEAD is unborn or unavailable. Object inspection still uses the immutable resolved object ID.".into());
    }
    if resolve(repo, query, active)? != oid {
        return Err("The revision changed while inspecting. Inspect again.".into());
    }
    Ok(Inspection {
        query: query.into(),
        format,
        git_dir,
        common_dir,
        objects,
        object,
        missing,
        representations,
        entries,
        notes,
        topology,
        independence,
    })
}
