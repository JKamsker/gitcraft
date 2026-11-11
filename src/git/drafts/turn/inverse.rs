//! Inverse turn previews and guarded application.

use super::*;

#[derive(Clone)]
pub struct Inverse {
    pub turn: String,
    pub parent: String,
    pub patch: Arc<Vec<u8>>,
    pub paths: Vec<String>,
    pub(super) source: Arc<Source>,
    pub(super) target: Arc<working_transaction::Contents>,
    pub(super) files: Arc<working_transaction::Contents>,
    pub(super) policies: Arc<native_files::Policies>,
}

pub(super) fn read_paths(
    ctx: &Context,
    paths: &[String],
    s: &Source,
    active: &dyn Fn() -> bool,
) -> Result<working_transaction::Contents, String> {
    paths.iter().map(|p|{
        let mode=s.index.get(p).map_or("100644",|e|e.mode.as_str());
        if !matches!(mode,"100644"|"100755"){return Err(crate::i18n::text("backend-turn-reversal-supports-regular-files-links-and-submodules-need-ma-403e2b").into());}
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
        return Err(crate::i18n::text(
            "backend-select-an-immutable-saved-turn-with-a-parent-1cf099",
        )
        .into());
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
        return Err(crate::i18n::text(
            "backend-turn-is-empty-or-exceeds-the-2000-file-reversal-limit-2623c0",
        )
        .into());
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
        return Err(crate::i18n::text("backend-turn-reversal-exceeds-32-mib-7dc25f").into());
    }
    // Mode changes, links and gitlinks are checked from both immutable trees, even if the
    // current index no longer contains the path.
    for rev in [parent, turn] {
        let data = ctx.run(&["ls-tree", "-r", "-z", rev], vec![], &[0], active)?;
        for row in data.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let row = std::str::from_utf8(row)
                .map_err(|_| crate::i18n::text("backend-turn-paths-must-be-utf-8-37109e"))?;
            let (info, path) = row
                .split_once('\t')
                .ok_or(crate::i18n::text("backend-invalid-tree-c5c836"))?;
            if paths.iter().any(|p| p == path)
                && !info.starts_with("100644 blob ")
                && !info.starts_with("100755 blob ")
            {
                return Err(crate::i18n::text(
                    "backend-turn-contains-links-or-gitlinks-resolve-those-manually-bd0eb0",
                )
                .into());
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
    super::super::restore::raw_attributes(&ctx, active)?;
    ctx.run(
        &[
            "-c",
            "core.autocrlf=false",
            "apply",
            "--cached",
            "--binary",
            "--whitespace=nowarn",
            "-",
        ],
        patch.clone(),
        &[0],
        active,
    )
    .map_err(|_| {
        crate::i18n::text(
            "backend-later-changes-overlap-this-turn-nothing-was-changed-inspect-the-t-9aac0a",
        )
        .to_owned()
    })?;
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
        let row = std::str::from_utf8(&rows)
            .map_err(|_| crate::i18n::text("backend-invalid-result-path-0ce530"))?;
        let data = if row.is_empty() {
            None
        } else {
            let info = row
                .split_once('\t')
                .ok_or(crate::i18n::text("backend-invalid-result-index-53c3ce"))?
                .0
                .split_whitespace()
                .collect::<Vec<_>>();
            if info.len() != 3 || !matches!(info[0], "100644" | "100755") {
                return Err(crate::i18n::text("backend-unsupported-result-mode-958628").into());
            }
            let size = ctx
                .text(&["cat-file", "-s", info[1]], vec![], active)?
                .parse::<usize>()
                .map_err(|_| crate::i18n::text("backend-invalid-blob-size-924b45"))?;
            if size > FILE_LIMIT {
                return Err(crate::i18n::text("backend-result-exceeds-8-mib-53d56f").into());
            }
            Some((
                info[0].into(),
                ctx.run(&["cat-file", "blob", info[1]], vec![], &[0], active)?,
            ))
        };
        target.insert(path.clone(), data);
    }
    if source(&ctx, active)? != now || read_paths(&ctx, &paths, &now, active)? != files {
        return Err(
            crate::i18n::text("backend-checkout-changed-during-preview-reopen-it-67a5ba").into(),
        );
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
            return Err(crate::i18n::text(
                "backend-head-index-or-working-files-changed-since-the-turn-preview-reopen-944e9e",
            )
            .into());
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
    .ok_or(crate::i18n::text(
        "backend-could-not-protect-current-work-b0d371",
    ))?;
    verify()?;
    let check = || -> Result<(), String> {
        if source(&ctx, active)? != *p.source {
            return Err(
                crate::i18n::text("backend-head-or-index-changed-while-reverting-cc7a49").into(),
            );
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
