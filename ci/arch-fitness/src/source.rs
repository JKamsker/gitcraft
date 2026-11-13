//! Tracked Rust sources: the working copy of every tracked file, or raw blobs of a commit.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::analysis::SourceInfo;

fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git: {e}"))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input).map_err(|e| format!("git: {e}"))?;
    }
    let output = child.wait_with_output().map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Regular `.rs` blobs from `ls-files -s` / `ls-tree -r` lines: (object id, path).
fn rust_entries(listing: &[u8]) -> Vec<(String, String)> {
    listing
        .split(|b| *b == 0)
        .filter_map(|entry| {
            let entry = std::str::from_utf8(entry).ok()?;
            let (meta, path) = entry.split_once('\t')?;
            let fields: Vec<&str> = meta.split_whitespace().collect();
            let mode = *fields.first()?;
            // `ls-files -s`: mode oid stage; `ls-tree`: mode type oid.
            let oid = if fields.get(1) == Some(&"blob") {
                fields.get(2)?
            } else {
                fields.get(1)?
            };
            let regular = mode == "100644" || mode == "100755";
            (regular && path.ends_with(".rs")).then(|| ((*oid).to_owned(), path.to_owned()))
        })
        .collect()
}

/// The working-tree contents of every tracked, regular Rust file (symlinks are excluded).
pub fn working_tree(root: &Path) -> Result<(BTreeMap<String, String>, SourceInfo), String> {
    let listing = git(root, &["ls-files", "-s", "-z"], None)?;
    let mut files = BTreeMap::new();
    for (_, path) in rust_entries(&listing) {
        match std::fs::read_to_string(root.join(&path)) {
            Ok(source) => {
                files.insert(path, source);
            }
            // A tracked file deleted in the working copy is simply not part of it.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{path}: {e}")),
        }
    }
    Ok((files, SourceInfo::default()))
}

/// The raw blobs of an immutable revision: export attributes and local edits do not apply.
pub fn revision(
    root: &Path,
    revision: &str,
) -> Result<(BTreeMap<String, String>, SourceInfo), String> {
    let commit = text(git(
        root,
        &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
        None,
    )?)?;
    let commit = commit.trim().to_owned();
    let tree = text(git(
        root,
        &["rev-parse", &format!("{commit}^{{tree}}")],
        None,
    )?)?;
    let listing = git(root, &["ls-tree", "-rz", "--full-tree", &commit], None)?;
    let entries = rust_entries(&listing);
    let request: String = entries.iter().map(|(oid, _)| format!("{oid}\n")).collect();
    let blobs = git(root, &["cat-file", "--batch"], Some(request.as_bytes()))?;
    let mut files = BTreeMap::new();
    let mut at = 0;
    for (oid, path) in entries {
        let end = blobs[at..]
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("truncated cat-file output")?
            + at;
        let header = std::str::from_utf8(&blobs[at..end]).map_err(|e| e.to_string())?;
        let fields: Vec<&str> = header.split(' ').collect();
        if fields.first() != Some(&oid.as_str()) || fields.get(1) != Some(&"blob") {
            return Err(format!("unexpected object for {path}: {header}"));
        }
        let size: usize = fields
            .get(2)
            .and_then(|s| s.parse().ok())
            .ok_or("bad blob size")?;
        let body = blobs.get(end + 1..end + 1 + size).ok_or("truncated blob")?;
        files.insert(path, String::from_utf8_lossy(body).into_owned());
        at = end + 1 + size + 1;
    }
    let info = SourceInfo {
        revision: Some(commit),
        tree: Some(tree.trim().to_owned()),
    };
    Ok((files, info))
}
