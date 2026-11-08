//! Strict inverse-turn application. Later non-overlapping work is retained; an overlap
//! fails before writes. Raw private Git context preserves the real index and bypasses
//! checkout filters. A protected safety draft is retained for recovery/undo.
use super::*;
use std::sync::Arc;
#[derive(Clone)]
pub struct Inverse {
    pub turn: String,
    pub parent: String,
    pub patch: Arc<Vec<u8>>,
    pub paths: Vec<String>,
    source: Arc<Source>,
    target: Arc<working_transaction::Contents>,
    files: Arc<BTreeMap<String, Option<(String, Vec<u8>)>>>,
    policies: Arc<native_files::Policies>,
}
fn read_paths(
    ctx: &Context,
    paths: &[String],
    s: &Source,
    active: &dyn Fn() -> bool,
) -> Result<BTreeMap<String, Option<(String, Vec<u8>)>>, String> {
    paths.iter().map(|p|{
        let mode=s.index.get(p).map_or("100644",|e|e.mode.as_str());
        if !matches!(mode,"100644"|"100755"){return Err("Turn reversal supports regular files; links and submodules need manual resolution".into());}
        Ok((p.clone(),read_file(ctx,p,mode,active)?))
    }).collect()
}
pub fn inverse_preview(
    repo: &Path,
    parent: &str,
    turn: &str,
    active: &dyn Fn() -> bool,
) -> Result<Inverse, String> {
    if !oid_valid(parent) || !oid_valid(turn) {
        return Err("Select an immutable saved turn with a parent".into());
    }
    let ctx = Context::new(repo, active)?;
    let paths = ctx.run(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--name-only",
            "-z",
            turn,
            parent,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    let paths = untracked_paths(paths)?.into_iter().collect::<Vec<_>>();
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("Turn is empty or exceeds the 2000-file reversal limit".into());
    }
    let now = source(&ctx, active)?;
    let files = read_paths(&ctx, &paths, &now, active)?;
    if files
        .values()
        .flatten()
        .map(|(_, b)| b.len())
        .sum::<usize>()
        > TOTAL_LIMIT
    {
        return Err("Turn reversal exceeds 32 MiB".into());
    }
    // Mode changes, links and gitlinks are checked from both immutable trees, even if the
    // current index no longer contains the path.
    for rev in [parent, turn] {
        let data = ctx.run(&["ls-tree", "-r", "-z", rev], vec![], &[0], active)?;
        for row in data.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let row = std::str::from_utf8(row).map_err(|_| "Turn paths must be UTF-8")?;
            let (info, path) = row.split_once('\t').ok_or("Invalid tree")?;
            if paths.iter().any(|p| p == path)
                && !info.starts_with("100644 blob ")
                && !info.starts_with("100755 blob ")
            {
                return Err("Turn contains links or gitlinks; resolve those manually".into());
            }
        }
    }
    let patch = ctx.run(
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            turn,
            parent,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    let mut entries = now.index.clone();
    for (path, data) in &files {
        match data {
            Some((mode, bytes)) => {
                entries.insert(
                    path.clone(),
                    Entry {
                        mode: mode.clone(),
                        oid: ctx.blob(bytes.clone(), active)?,
                    },
                );
            }
            None => {
                entries.remove(path);
            }
        }
    }
    tree(&ctx, &entries, active)?;
    super::restore::raw_attributes(&ctx, active)?;
    ctx.run(&["-c","core.autocrlf=false","apply","--cached","--binary","--whitespace=nowarn","-"],patch.clone(),&[0],active).map_err(|_|"Later changes overlap this turn. Nothing was changed; inspect the turn and resolve the overlap explicitly".to_owned())?;
    let mut target = BTreeMap::new();
    for path in &paths {
        let rows = ctx.run(
            &[
                "--literal-pathspecs",
                "ls-files",
                "--stage",
                "-z",
                "--",
                path,
            ],
            vec![],
            &[0],
            active,
        )?;
        let row = std::str::from_utf8(&rows).map_err(|_| "Invalid result path")?;
        let data = if row.is_empty() {
            None
        } else {
            let info = row
                .split_once('\t')
                .ok_or("Invalid result index")?
                .0
                .split_whitespace()
                .collect::<Vec<_>>();
            if info.len() != 3 || !matches!(info[0], "100644" | "100755") {
                return Err("Unsupported result mode".into());
            }
            let size = ctx
                .text(&["cat-file", "-s", info[1]], vec![], active)?
                .parse::<usize>()
                .map_err(|_| "Invalid blob size")?;
            if size > FILE_LIMIT {
                return Err("Result exceeds 8 MiB".into());
            }
            Some((
                info[0].into(),
                ctx.run(&["cat-file", "blob", info[1]], vec![], &[0], active)?,
            ))
        };
        target.insert(path.clone(), data);
    }
    if source(&ctx, active)? != now || read_paths(&ctx, &paths, &now, active)? != files {
        return Err("Checkout changed during preview; reopen it".into());
    }
    Ok(Inverse {
        turn: turn.into(),
        parent: parent.into(),
        patch: Arc::new(patch),
        paths,
        source: Arc::new(now),
        policies: Arc::new(native_files::capture_policies(repo, &files, active)?),
        files: Arc::new(files),
        target: Arc::new(target),
    })
}
pub struct Applied {
    pub draft: Id,
    pub evidence: String,
}
pub fn inverse_apply(
    repo: &Path,
    p: &Inverse,
    active: &dyn Fn() -> bool,
) -> Result<Applied, String> {
    let ctx = Context::new(repo, active)?;
    let verify = || -> Result<(), String> {
        if source(&ctx, active)? != *p.source
            || read_paths(&ctx, &p.paths, &p.source, active)? != *p.files
            || native_files::capture_policies(repo, &p.files, active)? != *p.policies
        {
            return Err(
                "HEAD, index or working files changed since the turn preview; reopen it".into(),
            );
        }
        Ok(())
    };
    verify()?;
    let backup = capture_preserving_paths(
        repo,
        "Before reverting one agent turn",
        None,
        &p.paths,
        active,
    )?
    .ok_or("Could not protect current work")?;
    verify()?;
    let check = || -> Result<(), String> {
        if source(&ctx, active)? != *p.source {
            return Err("HEAD or index changed while reverting".into());
        }
        Ok(())
    };
    let evidence =
        working_transaction::apply(repo, &p.files, &p.target, &p.policies, &check, active)?;
    Ok(Applied {
        draft: backup.id,
        evidence,
    })
}

#[derive(Clone)]
pub struct Hunk {
    pub patch: String,
    pub digest: String,
    pub reviewed: bool,
}
#[derive(Clone)]
pub struct Review {
    pub parent: String,
    pub turn: String,
    pub hunks: Vec<Hunk>,
    reference: String,
    old: String,
    known: BTreeSet<String>,
}
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn split(raw: &str, known: &BTreeSet<String>) -> Vec<Hunk> {
    let mut parts = Vec::<(String, String)>::new();
    let mut header = String::new();
    let mut body = String::new();
    let flush = |parts: &mut Vec<(String, String)>, header: &String, body: &mut String| {
        if !body.is_empty() {
            parts.push((header.clone(), std::mem::take(body)));
        }
    };
    for line in raw.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            flush(&mut parts, &header, &mut body);
            if !header.is_empty() && parts.last().is_none_or(|(h, _)| h != &header) {
                parts.push((header.clone(), String::new()));
            }
            header = line.into();
        } else if line.starts_with("@@ ") || line.starts_with("GIT binary patch") {
            flush(&mut parts, &header, &mut body);
            body.push_str(line);
        } else if body.is_empty() {
            header.push_str(line);
        } else {
            body.push_str(line);
        }
    }
    flush(&mut parts, &header, &mut body);
    if !header.is_empty() && parts.last().is_none_or(|(h, _)| h != &header) {
        parts.push((header, String::new()));
    }
    // Blob IDs vary when another hunk changes; paths, mode and other file metadata
    // still belong to the review identity. Unique text survives line-number shifts.
    let stable = parts
        .iter()
        .map(|(header, body)| {
            let metadata = header
                .lines()
                .filter_map(|line| {
                    if line.starts_with("index ") {
                        line.split_whitespace()
                            .nth(2)
                            .map(|mode| format!("index-mode {mode}"))
                    } else {
                        Some(line.to_owned())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let text = body
                .lines()
                .filter(|line| !line.starts_with("@@ "))
                .collect::<Vec<_>>()
                .join("\n");
            format!("review-v2\n{metadata}\n{text}")
        })
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    for key in &stable {
        *counts.entry(key).or_insert(0usize) += 1;
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(i, (header, body))| {
            let identity = if counts[&stable[i]] == 1 {
                stable[i].clone()
            } else {
                // Ambiguous identical occurrences bind location and ordinal. A moved,
                // inserted or removed duplicate is shown again rather than guessed.
                format!(
                    "{}\noccurrence {i}\n{}",
                    stable[i],
                    body.lines().next().unwrap_or_default()
                )
            };
            let key = digest(identity.as_bytes());
            Hunk {
                patch: format!("{header}{body}"),
                reviewed: known.contains(&key),
                digest: key,
            }
        })
        .collect()
}

pub fn review(
    repo: &Path,
    parent: &str,
    turn: &str,
    session: &str,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    if !oid_valid(parent) || !oid_valid(turn) {
        return Err("Turn has no immutable parent".into());
    }
    let ctx = Context::new(repo, active)?;
    let reference = format!(
        "refs/gitcraft/reviewed-turns/{}/{}",
        ctx.id,
        digest(session.as_bytes())
    );
    let old = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", &reference],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| "Invalid review ref")?
    .trim()
    .to_owned();
    let known = if old.is_empty() {
        BTreeSet::new()
    } else {
        let size = ctx
            .text(&["cat-file", "-s", &old], vec![], active)?
            .parse::<usize>()
            .map_err(|_| "Invalid review data")?;
        if size > 1024 * 1024 {
            return Err("Review state exceeds 1 MiB".into());
        }
        serde_json::from_slice::<BTreeSet<String>>(&ctx.run(
            &["cat-file", "blob", &old],
            vec![],
            &[0],
            active,
        )?)
        .map_err(|_| "Review state is invalid")?
    };
    let stat = ctx.run(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--numstat",
            parent,
            turn,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    if stat.len() > 128 * 1024 {
        return Err("Turn has too many files for review".into());
    }
    let raw = ctx.run(
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            parent,
            turn,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    if raw.len() > TOTAL_LIMIT {
        return Err("Turn diff exceeds 32 MiB; inspect files separately".into());
    }
    let raw = String::from_utf8(raw).map_err(|_| "Turn diff cannot be displayed as UTF-8")?;
    let hunks = split(&raw, &known);
    Ok(Review {
        parent: parent.into(),
        turn: turn.into(),
        hunks,
        reference,
        old,
        known,
    })
}
pub fn mark_reviewed(
    repo: &Path,
    review: &Review,
    selected: &[usize],
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let ctx = Context::new(repo, active)?;
    let mut known = review.known.clone();
    for &i in selected {
        known.insert(
            review
                .hunks
                .get(i)
                .ok_or("Invalid reviewed hunk")?
                .digest
                .clone(),
        );
    }
    if known.len() > 10000 {
        return Err("Review history reached 10000 hunks; choose another session".into());
    }
    let oid = ctx.blob(
        serde_json::to_vec(&known).map_err(|e| e.to_string())?,
        active,
    )?;
    let zero = "0".repeat(oid.len());
    ctx.source(
        &[
            "update-ref",
            &review.reference,
            &oid,
            if review.old.is_empty() {
                &zero
            } else {
                &review.old
            },
        ],
        vec![],
        &[0],
        active,
    )
    .map_err(|_| "Review marks changed concurrently; reload the turn".to_owned())?;
    Ok(())
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn patch(mode: &str, hunks: &str) -> String {
        format!("diff --git a/f b/f\nindex aaaa..bbbb {mode}\n--- a/f\n+++ b/f\n{hunks}")
    }
    #[test]
    fn pure_turn_review_identity_distinguishes_duplicate_locations_and_file_modes() {
        let text = " same\n-old\n+new\n same\n";
        let raw = patch(
            "100644",
            &format!("@@ -1,3 +1,3 @@\n{text}@@ -21,3 +21,3 @@\n{text}"),
        );
        let hunks = split(&raw, &BTreeSet::new());
        assert_eq!(hunks.len(), 2);
        assert_ne!(hunks[0].digest, hunks[1].digest);
        let known = [hunks[0].digest.clone()].into();
        let marked = split(&raw, &known);
        assert!(marked[0].reviewed);
        assert!(!marked[1].reviewed);
        let unique = patch("100644", &format!("@@ -1,3 +1,3 @@\n{text}"));
        assert!(!split(&unique, &known)[0].reviewed);
        let unique_mark = [split(&unique, &BTreeSet::new())[0].digest.clone()].into();
        let shifted = patch("100644", &format!("@@ -11,3 +11,3 @@\n{text}"));
        assert!(split(&shifted, &unique_mark)[0].reviewed);
        assert!(
            !split(
                &patch("100755", &format!("@@ -1,3 +1,3 @@\n{text}")),
                &unique_mark
            )[0]
            .reviewed
        );
        assert!(split(&raw, &unique_mark).iter().all(|h| !h.reviewed));
        let mode_change = format!(
            "diff --git a/f b/f\nold mode 100644\nnew mode 100755\nindex aaaa..bbbb\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n{text}"
        );
        assert!(!split(&mode_change, &unique_mark)[0].reviewed);
        let old_key = digest(format!("diff --git a/f b/f\n{}", text.trim_end()).as_bytes());
        assert!(!split(&unique, &[old_key].into())[0].reviewed);
    }
}
