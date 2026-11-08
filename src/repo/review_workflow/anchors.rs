use super::store;
use crate::{
    git,
    pull_request::{DiffSide, PrDetails},
    repo::pr_review::LineTarget,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Default)]
pub(in crate::repo) struct Relocations {
    base: String,
    head: String,
    targets: HashMap<String, LineTarget>,
    pub error: Option<String>,
}
impl Relocations {
    pub fn current(&self, base: &str, head: &str) -> Option<&HashMap<String, LineTarget>> {
        (self.error.is_none() && self.base == base && self.head == head).then_some(&self.targets)
    }
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    head: String,
    base: String,
    anchors: HashMap<String, Anchor>,
}
#[derive(Serialize, Deserialize)]
struct Anchor {
    path: String,
    line: u64,
    start: Option<u64>,
    right: bool,
}
fn mapped_span(patch: &str, start: Option<u64>, end: u64) -> Option<(Option<u64>, u64)> {
    let line = map_line(patch, end)?;
    let mapped = match start {
        Some(start) => {
            let mapped = map_line(patch, start)?;
            let length = end.checked_sub(start)?;
            if length > 10_000
                || (start..=end).any(|n| map_line(patch, n) != mapped.checked_add(n - start))
                || line.checked_sub(mapped) != Some(length)
            {
                return None;
            }
            Some(mapped)
        }
        None => None,
    };
    Some((mapped, line))
}

/// Only unchanged spans move. Edited/deleted lines have no justified destination. An
/// insertion at n is after n (including insertion at 0), as zero-context Git hunks specify.
fn map_line(patch: &str, line: u64) -> Option<u64> {
    if line == 0 {
        return None;
    }
    let mut delta = 0i64;
    for header in patch.lines().filter(|l| l.starts_with("@@ ")) {
        let mut words = header.split_whitespace();
        let _ = words.next();
        let (old, new) = (words.next()?, words.next()?);
        let span = |text: &str| -> Option<(u64, u64)> {
            let text = text.get(1..)?;
            match text.split_once(',') {
                Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
                None => Some((text.parse().ok()?, 1)),
            }
        };
        let (a, n) = span(old)?;
        let (b, m) = span(new)?;
        if n > 0 && (a..a.checked_add(n)?).contains(&line) {
            return None;
        }
        if (n == 0 && line > a) || (n > 0 && line >= a.checked_add(n)?) {
            delta = i64::try_from(b).ok()? - i64::try_from(a).ok()? + i64::try_from(m).ok()?
                - i64::try_from(n).ok()?;
            // Git's empty old/new spans point to the line before the change.
            if n == 0 {
                delta -= 1;
            }
            if m == 0 {
                delta += 1;
            }
        }
    }
    u64::try_from(i64::try_from(line).ok()?.checked_add(delta)?)
        .ok()
        .filter(|n| *n > 0)
}

fn relocate(
    repo: &Path,
    old: &str,
    new: &str,
    anchor: &Anchor,
) -> Result<Option<(LineTarget, Option<u64>)>, String> {
    let files = git::diff_files(repo, old, Some(new))?;
    let path = files
        .iter()
        .find(|f| f.old_path.as_deref() == Some(&anchor.path))
        .map(|f| f.path.as_str())
        .unwrap_or(&anchor.path);
    if files.iter().any(|f| f.path == path && f.status == 'D') {
        return Ok(None);
    }
    let patch = git::run(
        Some(repo),
        &[
            "--literal-pathspecs",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--unified=0",
            "-M",
            old,
            new,
            "--",
            &anchor.path,
            path,
        ],
    )?;
    let Some((start, line)) = mapped_span(&patch, anchor.start, anchor.line) else {
        return Ok(None);
    };
    // Reject unavailable blobs; a missing path is never a successful identity mapping.
    git::run(Some(repo), &["cat-file", "-e", &format!("{new}:{path}")])?;
    Ok(Some((
        LineTarget {
            path: path.to_owned(),
            line,
            side: if anchor.right {
                DiffSide::Right
            } else {
                DiffSide::Left
            },
        },
        start,
    )))
}

pub(in crate::repo) fn relocate_threads(
    repo: &Path,
    details: &PrDetails,
    base: &str,
) -> Result<Relocations, String> {
    let key = format!("{}#{}", details.url, details.number);
    let before = store::load::<Snapshot>(repo, "anchors", &key)?;
    let mut moved = HashMap::new();
    let mut anchors = HashMap::new();
    for thread in &details.extras.threads {
        // Pending comments belong to the forge's pending review. Never reinterpret them.
        if thread.comments.iter().any(|c| c.pending) {
            continue;
        }
        if !thread.outdated {
            if let Some(line) = thread.line {
                anchors.insert(
                    thread.id.clone(),
                    Anchor {
                        path: thread.path.clone(),
                        line,
                        start: thread.start_line,
                        right: thread.side == DiffSide::Right,
                    },
                );
            }
        } else if let Some(before) = &before
            && let Some(anchor) = before.anchors.get(&thread.id)
        {
            let old = if anchor.right {
                &before.head
            } else {
                &before.base
            };
            let new = if anchor.right {
                &details.head_oid
            } else {
                base
            };
            if let Some((target, start)) = relocate(repo, old, new, anchor)? {
                anchors.insert(
                    thread.id.clone(),
                    Anchor {
                        path: target.path.clone(),
                        line: target.line,
                        start,
                        right: anchor.right,
                    },
                );
                moved.insert(thread.id.clone(), target);
            }
        }
    }
    store::save(
        repo,
        "anchors",
        &key,
        &Snapshot {
            head: details.head_oid.clone(),
            base: base.to_owned(),
            anchors,
        },
    )?;
    Ok(Relocations {
        base: base.into(),
        head: details.head_oid.clone(),
        targets: moved,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renamed_files_follow_insertions_but_changed_lines_do_not() -> Result<(), String> {
        let r = crate::git::stack::test_repo::TestRepo::new("review-anchor-relocation")?;
        let old = r.commit("old.rs", "one\ntwo\nthree\nfour\nfive\nsix\n", "old")?;
        r.git(&["mv", "old.rs", "new.rs"])?;
        let new = r.commit(
            "new.rs",
            "inserted\none\ntwo\nchanged\nfour\nfive\nsix\n",
            "rewrite",
        )?;
        let mut a = Anchor {
            path: "old.rs".into(),
            line: 2,
            start: None,
            right: true,
        };
        let (mapped, _) = relocate(r.path(), &old, &new, &a)?.ok_or("missing relocation")?;
        assert_eq!((mapped.path.as_str(), mapped.line), ("new.rs", 3));
        a.line = 3;
        assert!(relocate(r.path(), &old, &new, &a)?.is_none());
        // Local evidence round-trips independently of forge objects and survives reopening.
        store::save(r.path(), "anchor-test", "https://h/o/r#1", &a)?;
        let saved = store::load::<Anchor>(r.path(), "anchor-test", "https://h/o/r#1")?
            .ok_or("missing saved anchor")?;
        assert_eq!(saved.line, 3);
        assert!(store::load::<Anchor>(r.path(), "anchor-test", "other")?.is_none());
        Ok(())
    }

    #[test]
    fn anchors_follow_only_unchanged_lines() {
        let patch = "@@ -0,0 +1,2 @@\n@@ -4,2 +6,1 @@\n@@ -9 +9,0 @@";
        assert_eq!(map_line(patch, 1), Some(3));
        assert_eq!(map_line(patch, 3), Some(5));
        assert_eq!(map_line(patch, 4), None);
        assert_eq!(map_line(patch, 5), None);
        assert_eq!(map_line(patch, 6), Some(7));
        assert_eq!(map_line(patch, 9), None);
        assert_eq!(map_line(patch, 10), Some(10));
        assert_eq!(map_line("@@ -3,0 +4,2 @@", 3), Some(3));
        assert_eq!(map_line("@@ -3,0 +4,2 @@", 4), Some(6));
        assert_eq!(map_line("", 0), None);
    }
    #[test]
    fn r2_pure_multiline_ranges_survive_serialization_and_repeated_rewrites() -> Result<(), String>
    {
        let (start, line) = mapped_span("@@ -0,0 +1 @@", Some(2), 3).ok_or("first mapping")?;
        let anchor = Anchor {
            path: "a.rs".into(),
            line,
            start,
            right: true,
        };
        let bytes = serde_json::to_vec(&anchor).map_err(|e| e.to_string())?;
        let reopened: Anchor = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        assert_eq!((reopened.start, reopened.line), (Some(3), 4));
        assert!(mapped_span("@@ -3 +3 @@", reopened.start, reopened.line).is_none());
        assert_eq!(
            mapped_span("@@ -0,0 +1,2 @@", reopened.start, reopened.line),
            Some((Some(5), 6))
        );
        Ok(())
    }
    #[test]
    fn r2_pure_relocation_maps_cannot_cross_generations_or_errors() {
        let mut map = Relocations {
            base: "A".into(),
            head: "B".into(),
            ..Default::default()
        };
        assert!(map.current("A", "B").is_some());
        assert!(map.current("A", "C").is_none());
        map.error = Some("unavailable".into());
        assert!(map.current("A", "B").is_none());
        assert!(Relocations::default().current("A", "B").is_none());
    }
}
