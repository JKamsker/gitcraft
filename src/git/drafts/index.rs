//! Private index creation, caching, and changed-path enumeration.

use super::*;

/// Copy metadata only, without following an index/cache link or persisting its contents
/// in the draft's object graph. The copied stat cache avoids hashing clean large files.
pub(super) fn internal_file(
    ctx: &Context,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<Vec<u8>>, String> {
    const LIMIT: u64 = 32 * 1024 * 1024;
    if let Some(target) = remote::parse(&ctx.repo) {
        let (parent, name) = path.rsplit_once('/').ok_or(crate::i18n::text(
            "backend-invalid-git-metadata-path-f4a8cd",
        ))?;
        let script = read_script(parent, name, LIMIT)?;
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                script,
                ..Default::default()
            },
            active,
        )?;
        if out.code == 0 && out.stdout == b"M\n" {
            return Ok(None);
        }
        if out.code == 0
            && out.stdout.len() >= 8
            && matches!(&out.stdout[..8], b"F100644\n" | b"F100755\n")
            && out.stdout.len() as u64 <= LIMIT + 8
        {
            return Ok(Some(out.stdout[8..].to_vec()));
        }
        return Err(crate::i18n::text(
            "backend-cannot-read-a-regular-index-cache-file-within-32-mib-136f8d",
        )
        .into());
    }
    let path = Path::new(path);
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err(crate::i18n::text(
                "backend-index-cache-links-and-special-files-are-not-supported-fcc001",
            )
            .into());
        }
        _ => {}
    }
    let parent = path
        .parent()
        .ok_or(crate::i18n::text("backend-invalid-index-path-d4427c"))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(crate::i18n::text("backend-index-path-is-not-utf-8-4ef0dd"))?;
    crate::safe_read::read_inside(parent, name, LIMIT)
        .map(Some)
        .ok_or_else(|| {
            crate::i18n::text("backend-index-cache-file-is-unreadable-or-exceeds-32-mib-0ad77d")
                .into()
        })
}

pub(super) fn private_index_file(
    ctx: &Context,
    name: &str,
    source_path: &str,
    bytes: &[u8],
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let path = ctx.path.join(name);
    if let Some(target) = remote::parse(&path) {
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                script: format!(
                    "umask 077; cat > {target} && test -f {source} && test ! -L {source} && touch -r {source} {target}",
                    target = remote::sh_path(&target.path),
                    source = remote::sh_path(source_path),
                ),
                stdin: Some(bytes.to_vec()),
                ..Default::default()
            },
            active,
        )?;
        if out.code != 0 {
            return Err(
                crate::i18n::text("backend-cannot-copy-the-private-index-cache-6185c5").into(),
            );
        }
    } else {
        let metadata = std::fs::symlink_metadata(source_path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(crate::i18n::text(
                "backend-index-cache-links-and-special-files-are-not-supported-fcc001",
            )
            .into());
        }
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_times(std::fs::FileTimes::new().set_modified(modified)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Concrete read-only subtree consumer: copy the existing index and split dependency
/// into the empty private context, using the same stat-cache checks as draft capture.
pub(in crate::git) fn inspection_index(
    ctx: &Context,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    cached_index(ctx, &source(ctx, active)?, active)
}

pub(super) fn cached_index(
    ctx: &Context,
    state: &Source,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    if !state.cache.is_empty() {
        private_index_file(ctx, "index", &ctx.index_file, &state.cache, active)?;
    } else if !state.raw.is_empty() {
        return Err(crate::i18n::text(
            "backend-index-disappeared-while-preparing-the-draft-1afbba",
        )
        .into());
    }
    if let Some((name, bytes)) = &state.shared {
        private_index_file(ctx, name, &ctx.shared_file, bytes, active)?;
    }
    if ctx.run(&["ls-files", "-v", "--stage", "-z"], vec![], &[0], active)? != state.raw {
        return Err(crate::i18n::text(
            "backend-index-moved-while-copying-its-private-stat-cache-3454e3",
        )
        .into());
    }
    // Preserve the source index's mtime when copying: advancing it makes Git skip
    // its racy-stat content check for same-size edits made in the original tick.
    // The source cache is immutable. Drop daemon/UC state and materialize any split
    // index only in the private copy before asking about working changes.
    ctx.run(
        &[
            "update-index",
            "--no-fsmonitor",
            "--no-untracked-cache",
            "--no-split-index",
        ],
        vec![],
        &[0],
        active,
    )?;
    let mut assume = Vec::new();
    for record in state.raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        if record.first().is_some_and(u8::is_ascii_lowercase) {
            let record = std::str::from_utf8(record)
                .map_err(|_| crate::i18n::text("backend-invalid-index-bb1331"))?;
            let (_, path) = record
                .split_once('\t')
                .ok_or(crate::i18n::text("backend-invalid-index-bb1331"))?;
            assume.extend_from_slice(path.as_bytes());
            assume.push(0);
        }
    }
    if !assume.is_empty() {
        ctx.run(
            &["update-index", "--no-assume-unchanged", "-z", "--stdin"],
            assume,
            &[0],
            active,
        )?;
    }
    Ok(())
}

pub(super) fn tree(
    ctx: &Context,
    entries: &BTreeMap<String, Entry>,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    ctx.run(&["read-tree", "--empty"], vec![], &[0], active)?;
    let input = entries
        .iter()
        .flat_map(|(p, e)| format!("{} {}\t{p}\0", e.mode, e.oid).into_bytes())
        .collect();
    ctx.run(&["update-index", "-z", "--index-info"], input, &[0], active)?;
    ctx.text(&["write-tree", "--missing-ok"], vec![], active)
}

pub(super) fn changed(
    ctx: &Context,
    state: &Source,
    index_tree: &str,
    active: &dyn Fn() -> bool,
) -> Result<BTreeSet<String>, String> {
    let mut result = paths(ctx.run(
        &[
            "diff-files",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
        ],
        vec![],
        &[0],
        active,
    )?)?;
    if let Some(head) = &state.head {
        result.extend(paths(ctx.run(
            &[
                "diff-tree",
                "--no-commit-id",
                "-r",
                "--name-only",
                "-z",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                head,
                index_tree,
            ],
            vec![],
            &[0],
            active,
        )?)?);
    } else {
        result.extend(state.index.keys().cloned());
    }
    Ok(result)
}

pub(super) fn paths(bytes: Vec<u8>) -> Result<BTreeSet<String>, String> {
    bytes
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .map(|p| {
            let p = std::str::from_utf8(p)
                .map_err(|_| crate::i18n::text("backend-draft-paths-must-be-utf-8-7ae280"))?;
            valid_path(p)?;
            Ok(p.to_owned())
        })
        .collect()
}

// Native ls-files lists an untracked nested working repository as a directory
// marker. Exclude its contents, rather than descending into another repository.
pub(super) fn untracked_paths(bytes: Vec<u8>) -> Result<BTreeSet<String>, String> {
    let mut result = BTreeSet::new();
    for name in bytes.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let name = std::str::from_utf8(name)
            .map_err(|_| crate::i18n::text("backend-draft-paths-must-be-utf-8-7ae280"))?;
        if let Some(directory) = name.strip_suffix('/') {
            valid_path(directory)?;
        } else {
            valid_path(name)?;
            result.insert(name.to_owned());
        }
    }
    Ok(result)
}
