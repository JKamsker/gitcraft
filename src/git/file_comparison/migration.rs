use super::*;

#[derive(Clone, Debug)]
pub(crate) enum MigrationSource {
    Staged,
    Commit(String),
    Range(String, String),
}
#[derive(Clone, Debug)]
pub(crate) struct Move {
    pub before: Entry,
    pub after: Entry,
    pub copy: bool,
    pub similarity: Option<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Migration {
    pub label: String,
    pub moves: Vec<Move>,
    pub other_changes: usize,
    pub warning: Option<String>,
}

pub(crate) fn read_migration(
    repo: &Path,
    source: &MigrationSource,
    active: &dyn Fn() -> bool,
) -> Result<Migration, String> {
    let snapshot = if matches!(source, MigrationSource::Staged) {
        Some((head(repo, active)?, index(repo, active)?.0))
    } else {
        None
    };
    let mut args = vec!["--no-lazy-fetch"];
    let (label, pinned) = match source {
        MigrationSource::Staged => {
            args.extend(["diff", "--cached"]);
            (
                crate::i18n::text("backend-staged-changes-head-index-3cc293").into(),
                snapshot
                    .as_ref()
                    .and_then(|s| s.0.clone())
                    .into_iter()
                    .collect::<Vec<_>>(),
            )
        }
        MigrationSource::Commit(rev) => {
            args.extend(["show", "--format=", "--root", "--diff-merges=first-parent"]);
            let oid = resolve(repo, rev, active)?;
            (
                crate::i18n::message(
                    "backend-commit-oid-first-parent-7f4877",
                    &[("oid", (oid).to_string().into())],
                ),
                vec![oid],
            )
        }
        MigrationSource::Range(from, to) => {
            args.push("diff");
            let a = resolve(repo, from, active)?;
            let b = resolve(repo, to, active)?;
            (
                format!("{from} → {to} · {} → {}", &a[..8], &b[..8]),
                vec![a, b],
            )
        }
    };
    args.extend([
        "--raw",
        "-z",
        "--no-abbrev",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules=none",
        "--find-renames=50%",
        "--find-copies=50%",
        "--find-copies-harder",
        "-l2000",
    ]);
    args.extend(pinned.iter().map(String::as_str));
    args.push("--");
    let out = output(repo, &args, active)?;
    if out.code != 0 {
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
    let (moves, other_changes) = parse_moves(&out.stdout)?;
    if let Some((before_head, before_index)) = snapshot
        && (head(repo, active)? != before_head || index(repo, active)?.0 != before_index)
    {
        return Err(crate::i18n::text(
            "backend-head-or-index-changed-while-detecting-moves-refresh-the-migration-728ab0",
        )
        .into());
    }
    Ok(Migration {
        label,
        moves,
        other_changes,
        warning: (!out.stderr.is_empty())
            .then(|| String::from_utf8_lossy(&out.stderr).trim().into()),
    })
}
fn parse_moves(bytes: &[u8]) -> Result<(Vec<Move>, usize), String> {
    let mut fields = bytes.split(|b| *b == 0).filter(|s| !s.is_empty());
    let mut moves = Vec::new();
    let mut other = 0;
    while let Some(meta) = fields.next() {
        let meta = meta.strip_prefix(b"\n").unwrap_or(meta);
        if meta.is_empty() {
            continue;
        }
        let text = std::str::from_utf8(meta)
            .map_err(|_| crate::i18n::text("backend-invalid-move-metadata-9be2ad"))?;
        let f: Vec<_> = text.split(' ').collect();
        if f.len() != 5 || !f[0].starts_with(':') || !hash(f[2]) || !hash(f[3]) {
            return Err(crate::i18n::text("backend-invalid-migration-record-72a022").into());
        }
        let old = fields
            .next()
            .ok_or(crate::i18n::text("backend-missing-migration-path-d27471"))?
            .to_vec();
        let code = f[4]
            .as_bytes()
            .first()
            .copied()
            .ok_or(crate::i18n::text("backend-missing-migration-status-4e93c7"))?;
        if matches!(code, b'R' | b'C') {
            let new = fields
                .next()
                .ok_or(crate::i18n::text(
                    "backend-missing-migration-destination-6bf2c0",
                ))?
                .to_vec();
            let score = f[4]
                .get(1..)
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|v| *v <= 100);
            moves.push(Move {
                before: Entry {
                    path: old,
                    mode: f[0][1..].into(),
                    oid: f[2].into(),
                },
                after: Entry {
                    path: new,
                    mode: f[1].into(),
                    oid: f[3].into(),
                },
                copy: code == b'C',
                similarity: score,
            });
        } else {
            other += 1;
        }
        if moves.len() + other > MAX_ROWS {
            return Err(crate::i18n::text(
                "backend-more-than-2-000-changed-paths-use-a-smaller-commit-range-84502d",
            )
            .into());
        }
    }
    Ok((moves, other))
}
