//! What travels and how, as plain data: the pieces a bundle is sent in, the revisions it is
//! made of, and the answers of the git commands the teleport reads. No git is run here.

use std::ops::Range;

/// How much of the bundle goes in one command to the other machine: as base64 in a script,
/// a third more than this.
pub(super) const CHUNK: usize = 4 << 20;

/// A bundle larger than this is only sent once the user says so.
pub(crate) const LARGE: u64 = 100 << 20;

/// The pieces of a bundle of `len` bytes, `chunk` bytes each (the last one less): at least one,
/// so that even an empty one makes its file.
pub(super) fn chunks(len: usize, chunk: usize) -> Vec<Range<usize>> {
    let chunk = chunk.max(1);
    if len == 0 {
        return std::iter::once(0..0).collect();
    }
    (0..len)
        .step_by(chunk)
        .map(|start| start..(start + chunk).min(len))
        .collect()
}

/// The temporary ref the snapshot is held by on the source while it is bundled.
pub(super) fn snapshot_ref(id: &str) -> String {
    format!("refs/gitcraft/teleport/{id}")
}

/// What `git bundle create --stdin` is given: the snapshot's ref, and every commit the other
/// machine already has, left out (`^<hash>`).
pub(super) fn bundle_revs(snapshot_ref: &str, bases: &[String]) -> String {
    let mut revs = format!("{snapshot_ref}\n");
    for base in bases {
        revs.push('^');
        revs.push_str(base);
        revs.push('\n');
    }
    revs
}

/// The hashes `for-each-ref --format='%(objectname) %(*objectname)'` printed, an annotated
/// tag as the commit it points to: what the other machine has.
pub(super) fn parse_tips(out: &str) -> Vec<String> {
    let mut tips: Vec<String> = out
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let object = words.next()?;
            Some(words.next().unwrap_or(object).to_owned())
        })
        .filter(|hash| is_hash(hash))
        .collect();
    tips.sort();
    tips.dedup();
    tips
}

/// Of what `cat-file --batch-check='%(objectname) %(objecttype)'` printed, the commits (and
/// tags) this machine has: those a bundle can leave out.
pub(super) fn parse_present(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| {
            let (hash, kind) = line.split_once(' ')?;
            (matches!(kind.trim(), "commit" | "tag") && is_hash(hash)).then(|| hash.to_owned())
        })
        .collect()
}

/// A full object name, SHA-1 or SHA-256.
pub(super) fn is_hash(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The paths `diff-tree -r -z` (raw, no renames) printed as being a repository of their own
/// (`160000`) on the new side: submodules, or a folder with a `.git` of its own that `add -A`
/// took for one.
pub(super) fn parse_gitlinks(out: &[u8]) -> Vec<String> {
    let mut found = Vec::new();
    let mut fields = out.split(|&b| b == 0);
    while let Some(head) = fields.next() {
        if head.is_empty() {
            continue;
        }
        let Some(path) = fields.next() else { break };
        // `:<old mode> <new mode> <old> <new> <status>`
        let head = String::from_utf8_lossy(head);
        if head.split_whitespace().nth(1) == Some("160000") {
            found.push(String::from_utf8_lossy(path).into_owned());
        }
    }
    found
}

/// The paths `check-attr -z filter` printed as going through Git LFS.
pub(super) fn parse_lfs_paths(out: &[u8]) -> Vec<String> {
    let fields: Vec<&[u8]> = out.split(|&b| b == 0).collect();
    fields
        .chunks(3)
        .filter(|f| f.len() == 3 && f[1] == b"filter" && f[2] == b"lfs")
        .map(|f| String::from_utf8_lossy(f[0]).into_owned())
        .collect()
}

/// NUL-separated paths as git printed them, byte for byte (a name needn't be UTF-8).
pub(super) fn parse_raw_paths(out: &[u8]) -> Vec<Vec<u8>> {
    out.split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

/// Paths as NUL-separated input (`--stdin -z`, `--pathspec-from-file=- --pathspec-file-nul`).
pub(super) fn nul_list<P: AsRef<[u8]>>(paths: &[P]) -> Vec<u8> {
    let mut out = Vec::new();
    for path in paths {
        out.extend_from_slice(path.as_ref());
        out.push(0);
    }
    out
}

/// A locale-independent size in raw transfer diagnostics; display localizes it separately.
pub(super) fn size_label(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}

/// The name of the source's temporary index, in its git folder.
pub(super) fn temp_index_name(id: &str) -> String {
    format!("gitcraft-teleport-{id}.index")
}
