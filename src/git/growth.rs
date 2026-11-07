//! First-parent replay follows one coherent repository tree, rather than interleaving
//! incompatible branch states. Baseline and commits are pinned to immutable object IDs.
use super::activity::process;
use std::{collections::BTreeSet, path::Path};
#[derive(Debug)]
pub(crate) struct Frame {
    pub hash: String,
    pub name: String,
    pub date: String,
    pub subject: String,
    pub changes: Vec<(Vec<u8>, bool)>,
}
pub(crate) struct Growth {
    pub frames: Vec<Frame>,
    pub baseline: BTreeSet<Vec<u8>>,
    pub more: bool,
    pub shallow: bool,
}
const FORMAT: &str = "--format=%x00%H%x00%aN%x00%aI%x00%s";
pub(crate) fn read(repo: &Path, active: &dyn Fn() -> bool) -> Result<Growth, String> {
    let run = |args: &[&str]| process::run(repo, args, None, active);
    let head = run(&["rev-parse", "--revs-only", "HEAD"])?;
    let head = std::str::from_utf8(&head)
        .map_err(|_| "Invalid HEAD")?
        .trim();
    let shallow = run(&["rev-parse", "--is-shallow-repository"])? == b"true\n";
    if head.is_empty() {
        return Ok(Growth {
            frames: Vec::new(),
            baseline: BTreeSet::new(),
            more: false,
            shallow,
        });
    }
    let raw = run(&[
        "log",
        "--no-show-signature",
        "--encoding=UTF-8",
        "--use-mailmap",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--no-relative",
        "--no-color",
        "--ignore-submodules=none",
        "--first-parent",
        "--diff-merges=first-parent",
        "--root",
        "--raw",
        "--no-abbrev",
        "-z",
        "--reverse",
        "--max-count=1001",
        FORMAT,
        head,
        "--",
    ])?;
    let mut frames = parse(&raw)?;
    let more = frames.len() > 1000;
    if more {
        frames.remove(0);
    }
    let mut baseline = BTreeSet::new();
    if let Some(first) = frames.first() {
        let tree = run(&["ls-tree", "-r", "-z", "--name-only", &first.hash])?;
        for path in tree.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            validate_path(path)?;
            baseline.insert(path.to_vec());
            if baseline.len() > 100_000 {
                return Err("Growth replay is limited to 100,000 files".into());
            }
        }
    }
    let mut state = baseline.clone();
    for frame in frames.iter().skip(1) {
        for (path, present) in &frame.changes {
            if !active() {
                return Err("Replay cancelled".into());
            }
            if *present {
                state.insert(path.clone());
            } else {
                state.remove(path);
            }
            if state.len() > 100_000 {
                return Err("Growth replay is limited to 100,000 files per tree".into());
            }
        }
    }
    Ok(Growth {
        frames,
        baseline,
        more,
        shallow,
    })
}
fn validate_path(path: &[u8]) -> Result<(), String> {
    if path.is_empty() || path.len() > 32768 || path.split(|b| *b == b'/').count() > 128 {
        return Err("Growth path/depth budget exceeded".into());
    }
    Ok(())
}
fn parse(raw: &[u8]) -> Result<Vec<Frame>, String> {
    let mut fields = raw.split(|b| *b == 0).peekable();
    let mut frames = Vec::new();
    while let Some(hash) = fields.next() {
        if hash.is_empty() || hash == b"\n" {
            continue;
        }
        if !matches!(hash.len(), 40 | 64) || !hash.iter().all(u8::is_ascii_hexdigit) {
            return Err("Invalid replay commit".into());
        }
        let mut next = || fields.next().ok_or("Truncated replay metadata");
        let name = String::from_utf8_lossy(next()?).into_owned();
        let date = String::from_utf8_lossy(next()?).into_owned();
        let subject = String::from_utf8_lossy(next()?).into_owned();
        let mut changes = Vec::new();
        while let Some(field) = fields.peek() {
            if field.is_empty() {
                fields.next();
                break;
            }
            let field = fields.next().ok_or("Missing replay change")?;
            let field = field.strip_prefix(b"\n").unwrap_or(field);
            if field.is_empty() {
                continue;
            }
            let parts: Vec<_> = field.split(|b| *b == b' ').collect();
            if parts.len() != 5
                || !parts[0].starts_with(b":")
                || parts[4].len() != 1
                || !b"ACDMTUXB".contains(&parts[4][0])
            {
                return Err("Invalid replay changed-path record".into());
            }
            let path = fields.next().ok_or("Missing replay path")?;
            if path.is_empty() {
                return Err("Empty replay path".into());
            }
            validate_path(path)?;
            changes.push((path.to_vec(), parts[4] != b"D"));
        }
        frames.push(Frame {
            hash: String::from_utf8_lossy(hash).into_owned(),
            name,
            date,
            subject,
            changes,
        });
    }
    Ok(frames)
}
impl Growth {
    /// Reconstruct with borrowed paths; source streams already bound their total
    /// size. Cancellation is checked inside replay, before expanded rows allocate.
    pub fn files_at_borrowed(
        &self,
        cursor: usize,
        active: &dyn Fn() -> bool,
    ) -> Result<BTreeSet<&[u8]>, String> {
        let mut files = BTreeSet::new();
        for path in &self.baseline {
            if !active() {
                return Err("Frame cancelled".into());
            }
            files.insert(path.as_slice());
        }
        for frame in self.frames.iter().take(cursor.saturating_add(1)).skip(1) {
            for (path, present) in &frame.changes {
                if !active() {
                    return Err("Frame cancelled".into());
                }
                if *present {
                    files.insert(path.as_slice());
                } else {
                    files.remove(path.as_slice());
                }
            }
        }
        Ok(files)
    }
    #[cfg(test)]
    pub fn files_at(&self, cursor: usize) -> BTreeSet<Vec<u8>> {
        self.files_at_borrowed(cursor, &|| true)
            .expect("active fixture")
            .into_iter()
            .map(<[u8]>::to_vec)
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_adds_changes_deletes_and_scrubs_back_to_baseline() -> Result<(), String> {
        let h = "a".repeat(40);
        let z = "0".repeat(40);
        let raw = format!(
            "\0{h}\0One\02026-11-06\0root\0\n:000000 100644 {z} {h} A\0src/a\0\0{h}\0Two\02026-11-07\0move\0\n:100644 000000 {h} {z} D\0src/a\0:000000 100644 {z} {h} A\0src/b\0"
        );
        let frames = parse(raw.as_bytes())?;
        let growth = Growth {
            frames,
            baseline: [b"src/a".to_vec()].into_iter().collect(),
            more: false,
            shallow: false,
        };
        assert_eq!(
            growth.files_at(0),
            [b"src/a".to_vec()].into_iter().collect()
        );
        assert_eq!(
            growth.files_at(1),
            [b"src/b".to_vec()].into_iter().collect()
        );
        assert_eq!(growth.frames[1].name, "Two");
        assert!(parse(b"\0broken\0").is_err());
        Ok(())
    }
}
