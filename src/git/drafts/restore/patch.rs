//! Restoration patch construction, path rewriting, and hunk parsing.

use super::*;

pub(in crate::git::drafts) fn included(
    ctx: &Context,
    now: &Source,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    if !now.index.contains_key(path) {
        let mut input = path.as_bytes().to_vec();
        input.push(0);
        if !ctx
            .source(
                &["check-ignore", "--no-index", "-z", "--stdin"],
                input,
                &[0, 1],
                active,
            )?
            .is_empty()
        {
            return Err(crate::i18n::text(
                "backend-this-path-is-now-ignored-recovery-will-not-overwrite-or-create-ig-3ffec0",
            )
            .into());
        }
    }
    Ok(())
}

pub(super) fn scratch(
    ctx: &Context,
    name: &str,
    bytes: &[u8],
    mode: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if !name.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-') {
        return Err(crate::i18n::text("backend-invalid-scratch-name-6abb39").into());
    }
    let path = ctx.path.join(name);
    if let Some(target) = remote::parse(&path) {
        let q = remote::sh_path(&target.path);
        let perms = if mode == "100755" { "700" } else { "600" };
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                script: format!("umask 077; cat > {q} && chmod {perms} -- {q}"),
                stdin: Some(bytes.to_vec()),
                ..Default::default()
            },
            active,
        )?;
        if out.code != 0 {
            return Err(crate::i18n::text("backend-could-not-prepare-recovery-diff-d0b416").into());
        }
    } else {
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &path,
                std::fs::Permissions::from_mode(if mode == "100755" { 0o700 } else { 0o600 }),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(remote::git_path(&path))
}

pub(super) fn diff(
    ctx: &Context,
    before: Option<&(String, Vec<u8>)>,
    saved: Option<&(String, Vec<u8>)>,
    binary: bool,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let mode = before.or(saved).map_or("100644", |(m, _)| m.as_str());
    let a = if let Some((_, b)) = before {
        scratch(ctx, "before", b, mode, active)?
    } else {
        "/dev/null".into()
    };
    let b = if let Some((_, b)) = saved {
        scratch(ctx, "after", b, mode, active)?
    } else {
        "/dev/null".into()
    };
    if before.is_none() && saved.is_none() {
        return Ok(vec![]);
    }
    let attrs = scratch(
        ctx,
        "binary-attributes",
        if binary { b"* -diff\n" } else { b"" },
        "100644",
        active,
    )?;
    let attr = format!("core.attributesFile={attrs}");
    let empty = ctx.text(&["mktree"], vec![], active)?;
    let source = format!("--attr-source={empty}");
    ctx.run(
        &[
            "-c",
            &attr,
            &source,
            "diff",
            "--no-index",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "-U3",
            "--",
            &a,
            &b,
        ],
        vec![],
        &[0, 1],
        active,
    )
}

pub(super) fn repath(
    raw: &[u8],
    path: &str,
    create: bool,
    delete: bool,
) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| crate::i18n::text("backend-recovery-patch-is-not-utf-8-47aee6"))?;
    if text.is_empty() {
        return Ok(vec![]);
    }
    let quote = |prefix: &str| format!("\"{prefix}/{}\"", path.replace('"', "\\\""));
    let mut result = String::new();
    let mut header = true;
    for line in text.split_inclusive('\n') {
        if line.starts_with("@@ ") || line.starts_with("GIT binary patch") {
            header = false;
        }
        if header && line.starts_with("diff --git ") {
            result.push_str(&format!("diff --git {} {}\n", quote("a"), quote("b")));
        } else if header && line.starts_with("--- ") {
            result.push_str(&format!(
                "--- {}\n",
                if create {
                    "/dev/null".into()
                } else {
                    quote("a")
                }
            ));
        } else if header && line.starts_with("+++ ") {
            result.push_str(&format!(
                "+++ {}\n",
                if delete {
                    "/dev/null".into()
                } else {
                    quote("b")
                }
            ));
        } else {
            result.push_str(line);
        }
    }
    Ok(result.into_bytes())
}

pub(super) fn offsets(bytes: &[u8]) -> Vec<usize> {
    let mut result = vec![0];
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            result.push(i + 1);
        }
    }
    if *result.last().unwrap_or(&0) != bytes.len() {
        result.push(bytes.len());
    }
    result
}

pub(super) fn parse_hunks(
    text: &str,
    before: &[u8],
    saved: &[u8],
) -> Result<Vec<RestoreHunk>, String> {
    let (a, b) = (offsets(before), offsets(saved));
    let mut hunks = Vec::new();
    let range = |s: &str, offsets: &[usize]| -> Result<Range<usize>, String> {
        let (start, count) = s.split_once(',').unwrap_or((s, "1"));
        let start = start
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-hunk-start-1152df"))?;
        let count = count
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-hunk-count-f8ceba"))?;
        let from = if count == 0 {
            start
        } else {
            start
                .checked_sub(1)
                .ok_or(crate::i18n::text("backend-invalid-hunk-position-774dce"))?
        };
        let end = from
            .checked_add(count)
            .ok_or(crate::i18n::text("backend-hunk-range-overflow-e3e7c7"))?;
        Ok(*offsets
            .get(from)
            .ok_or(crate::i18n::text("backend-hunk-outside-file-c360da"))?
            ..*offsets
                .get(end)
                .ok_or(crate::i18n::text("backend-hunk-outside-file-c360da"))?)
    };
    for line in text.lines().filter(|s| s.starts_with("@@ -")) {
        let rest = line
            .strip_prefix("@@ -")
            .ok_or(crate::i18n::text("backend-invalid-hunk-1e6899"))?;
        let (ranges, _) = rest
            .split_once(" @@")
            .ok_or(crate::i18n::text("backend-invalid-hunk-1e6899"))?;
        let (old, new) = ranges
            .split_once(" +")
            .ok_or(crate::i18n::text("backend-invalid-hunk-1e6899"))?;
        let old = range(old, &a)?;
        let new = range(new, &b)?;
        hunks.push(RestoreHunk {
            label: line.into(),
            range: old,
            replacement: Arc::new(saved[new].to_vec()),
        });
        if hunks.len() > 200 {
            return Err(crate::i18n::text(
                "backend-this-file-has-over-200-recovery-hunks-use-git-in-the-terminal-f94fd3",
            )
            .into());
        }
    }
    Ok(hunks)
}

pub(in crate::git::drafts) fn raw_attributes(
    ctx: &Context,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let bytes = b"* -text -filter -ident -working-tree-encoding -eol\n";
    let dir = ctx.path.join("info");
    let file = dir.join("attributes");
    if let Some(t) = remote::parse(&ctx.path) {
        let out = git::forecast_remote(
            &t.host,
            remote::Request {
                script: format!(
                    "umask 077; mkdir -p -- {} && cat > {}",
                    remote::sh_path(&remote::git_path(&dir)),
                    remote::sh_path(&remote::git_path(&file))
                ),
                stdin: Some(bytes.to_vec()),
                ..Default::default()
            },
            active,
        )?;
        if out.code != 0 {
            return Err(
                crate::i18n::text("backend-could-not-isolate-recovery-attributes-b2c844").into(),
            );
        }
    } else {
        std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::write(file, bytes))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
