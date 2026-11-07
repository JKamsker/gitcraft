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
        files: Arc::new(files),
        target: Arc::new(target),
    })
}
pub fn inverse_apply(repo: &Path, p: &Inverse, active: &dyn Fn() -> bool) -> Result<Id, String> {
    let ctx = Context::new(repo, active)?;
    let verify = || -> Result<(), String> {
        if source(&ctx, active)? != *p.source
            || read_paths(&ctx, &p.paths, &p.source, active)? != *p.files
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
    working_transaction::apply(repo, &p.files, &p.target, &check, active)?;
    Ok(backup.id)
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
    let mut out = Vec::new();
    let mut file = String::new();
    let mut header = String::new();
    let mut body = String::new();
    let flush = |out: &mut Vec<Hunk>, header: &String, body: &mut String, file: &String| {
        if body.is_empty() {
            return;
        }
        let stable = body
            .lines()
            .filter(|s| !s.starts_with("@@ "))
            .collect::<Vec<_>>()
            .join("\n");
        let key = digest(format!("{file}\n{stable}").as_bytes());
        out.push(Hunk {
            patch: format!("{header}{body}"),
            reviewed: known.contains(&key),
            digest: key,
        });
        body.clear();
    };
    for line in raw.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            flush(&mut out, &header, &mut body, &file);
            file = line.to_owned();
            header = line.to_owned();
        } else if line.starts_with("@@ ") || line.starts_with("GIT binary patch") {
            flush(&mut out, &header, &mut body, &file);
            body.push_str(line);
        } else if body.is_empty() {
            header.push_str(line);
        } else {
            body.push_str(line);
        }
    }
    flush(&mut out, &header, &mut body, &file);
    if !header.is_empty() && out.last().is_none_or(|h| !h.patch.starts_with(&header)) {
        let key = digest(header.as_bytes());
        out.push(Hunk {
            patch: header,
            reviewed: known.contains(&key),
            digest: key,
        });
    }
    out
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
