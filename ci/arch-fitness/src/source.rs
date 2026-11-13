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
    // Feed stdin from a thread while stdout is drained: `cat-file --batch` answers as it
    // reads, so writing every request first deadlocks once both pipe buffers fill (a
    // 64 KiB Linux pipe is far smaller than the request list of a real tree).
    let writer = match (input, child.stdin.take()) {
        (Some(input), Some(mut stdin)) => {
            let input = input.to_vec();
            Some(std::thread::spawn(move || stdin.write_all(&input)))
        }
        _ => None,
    };
    let output = child.wait_with_output().map_err(|e| format!("git: {e}"))?;
    let written = writer.map(std::thread::JoinHandle::join);
    // A failing git closes stdin early; its own error explains more than the broken pipe.
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if let Some(written) = written {
        written
            .map_err(|_| "git: stdin writer panicked".to_owned())?
            .map_err(|e| format!("git: {e}"))?;
    }
    Ok(output.stdout)
}

fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Regular `.rs` blobs from `ls-files -s` / `ls-tree -r` lines: (object id, path). A
/// tracked Rust path that is not UTF-8 is an error, never silently left out.
fn rust_entries(listing: &[u8]) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for entry in listing.split(|b| *b == 0) {
        let Some(tab) = entry.iter().position(|b| *b == b'\t') else {
            continue;
        };
        let (meta, path) = (&entry[..tab], &entry[tab + 1..]);
        let Ok(meta) = std::str::from_utf8(meta) else {
            continue;
        };
        let fields: Vec<&str> = meta.split_whitespace().collect();
        // `ls-files -s`: mode oid stage; `ls-tree`: mode type oid.
        let oid = if fields.get(1) == Some(&"blob") {
            fields.get(2)
        } else {
            fields.get(1)
        };
        let (Some(mode), Some(oid)) = (fields.first(), oid) else {
            continue;
        };
        let regular = *mode == "100644" || *mode == "100755";
        if !regular || !path.ends_with(b".rs") {
            continue;
        }
        let path = std::str::from_utf8(path).map_err(|_| {
            format!(
                "tracked Rust path is not UTF-8: {}",
                String::from_utf8_lossy(path)
            )
        })?;
        out.push(((*oid).to_owned(), path.to_owned()));
    }
    Ok(out)
}

/// The working-tree contents of every tracked, regular Rust file (symlinks are excluded).
pub fn working_tree(root: &Path) -> Result<(BTreeMap<String, String>, SourceInfo), String> {
    let listing = git(root, &["ls-files", "-s", "-z"], None)?;
    let mut files = BTreeMap::new();
    for (_, path) in rust_entries(&listing)? {
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
    let entries = rust_entries(&listing)?;
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

#[cfg(test)]
mod tests {
    use super::rust_entries;

    #[test]
    fn non_utf8_rust_paths_are_errors_and_other_paths_are_skipped() {
        let oid = "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391";
        let mut listing = format!("100644 {oid} 0\tsrc/ok.rs\0").into_bytes();
        listing.extend(format!("100644 blob {oid}\tdocs/").as_bytes());
        listing.extend(b"\xff.md\0");
        assert_eq!(
            rust_entries(&listing),
            Ok(vec![(oid.to_owned(), "src/ok.rs".to_owned())])
        );
        listing.extend(format!("100644 blob {oid}\tsrc/").as_bytes());
        listing.extend(b"\xff.rs\0");
        let err = rust_entries(&listing).err().unwrap_or_default();
        assert!(err.contains("not UTF-8"), "{err}");
    }
}
