//! Recovery pins saved refs and current HEAD/index/bytes. It writes only the working
//! file after a protected safety draft; staged contents are never replaced.
use super::*;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct RestoreHunk {
    pub label: String,
    range: Range<usize>,
    replacement: Arc<Vec<u8>>,
}
#[derive(Clone)]
pub struct RestorePreview {
    pub id: Id,
    pub path: String,
    pub index_layer: bool,
    pub binary: bool,
    pub hunks: Vec<RestoreHunk>,
    pub lines: Arc<Vec<String>>,
    pub before_len: usize,
    pub saved_len: usize,
    before: Option<(String, Arc<Vec<u8>>)>,
    saved: Option<(String, Arc<Vec<u8>>)>,
    index: Arc<Vec<u8>>,
    shared: Option<(String, Arc<Vec<u8>>)>,
    head: Option<String>,
    branch: Option<String>,
}
impl std::fmt::Debug for RestorePreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestorePreview")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("hunks", &self.hunks.len())
            .finish_non_exhaustive()
    }
}

pub fn preview(
    repo: &Path,
    id: &Id,
    path: &str,
    index_layer: bool,
    active: &dyn Fn() -> bool,
) -> Result<RestorePreview, String> {
    valid_path(path)?;
    let ctx = Context::new(repo, active)?;
    current(&ctx, id, active)?;
    let meta = metadata(&ctx, id, active)?;
    let file = meta
        .files
        .iter()
        .find(|f| f.path == path)
        .ok_or(crate::i18n::text(
            "backend-this-file-is-not-in-the-draft-0c674a",
        ))?;
    let (entry, saved_tree) = if index_layer {
        (&file.index, &meta.index_tree)
    } else {
        (&file.work, &meta.work_tree)
    };
    let actual = ctx.run(
        &[
            "--literal-pathspecs",
            "ls-tree",
            "-r",
            "-z",
            saved_tree,
            "--",
            path,
        ],
        vec![],
        &[0],
        active,
    )?;
    let expected = entry
        .as_ref()
        .map(|e| format!("{} blob {}\t{path}\0", e.mode, e.oid))
        .unwrap_or_default();
    if actual != expected.as_bytes() {
        return Err(crate::i18n::text(
            "backend-draft-file-metadata-does-not-match-its-protected-tree-2734e8",
        )
        .into());
    }
    let saved = if let Some(e) = entry {
        if !matches!(e.mode.as_str(), "100644" | "100755") {
            return Err(crate::i18n::text(
                "backend-draft-recovery-supports-regular-file-contents-only-links-and-subm-b6fdc3",
            )
            .into());
        }
        let n = ctx
            .text(&["cat-file", "-s", &e.oid], vec![], active)?
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-saved-blob-size-7b7dd0"))?;
        if n > FILE_LIMIT {
            return Err(crate::i18n::text(
                "backend-saved-file-exceeds-the-8-mib-recovery-limit-b60286",
            )
            .into());
        }
        let bytes = ctx.run(&["cat-file", "blob", &e.oid], vec![], &[0], active)?;
        if bytes.len() > FILE_LIMIT {
            return Err(
                crate::i18n::text("backend-saved-file-exceeds-the-recovery-limit-a79388").into(),
            );
        }
        Some((e.mode.clone(), bytes))
    } else {
        None
    };
    let now = source(&ctx, active)?;
    included(&ctx, &now, path, active)?;
    let before = read_file(
        &ctx,
        path,
        now.index.get(path).map_or("100644", |e| &e.mode),
        active,
    )?;
    let binary = before
        .iter()
        .chain(saved.iter())
        .any(|(_, b)| b.contains(&0) || std::str::from_utf8(b).is_err());
    let raw = diff(&ctx, before.as_ref(), saved.as_ref(), binary, active)?;
    let text = String::from_utf8(raw)
        .map_err(|_| crate::i18n::text("backend-cannot-render-recovery-diff-as-utf-8-1e4512"))?;
    let hunks = if before == saved {
        Vec::new()
    } else if binary || before.is_none() || saved.is_none() {
        vec![RestoreHunk {
            label: if binary {
                crate::i18n::text("backend-whole-binary-file-ba2a52").into()
            } else if before.is_none() {
                crate::i18n::text("backend-create-saved-file-9b75eb").into()
            } else {
                crate::i18n::text("backend-restore-saved-deletion-f99c5b").into()
            },
            range: 0..before.as_ref().map_or(0, |(_, b)| b.len()),
            replacement: Arc::new(saved.as_ref().map_or_else(Vec::new, |(_, b)| b.clone())),
        }]
    } else {
        parse_hunks(
            &text,
            &before
                .as_ref()
                .ok_or(crate::i18n::text("backend-no-current-file-e0bad1"))?
                .1,
            &saved
                .as_ref()
                .ok_or(crate::i18n::text("backend-no-saved-file-9a45c2"))?
                .1,
        )?
    };
    let lines = if binary {
        vec![crate::i18n::message(
            "backend-binary-content-bytes-recovery-is-for-the-whole-file-8d40d8",
            &[
                (
                    "count",
                    (before.as_ref().map_or(0, |(_, b)| b.len()))
                        .to_string()
                        .into(),
                ),
                (
                    "count2",
                    (saved.as_ref().map_or(0, |(_, b)| b.len()))
                        .to_string()
                        .into(),
                ),
            ],
        )]
    } else {
        text.lines()
            .take(5000)
            .map(|s| {
                let mut text: String = s.chars().take(400).collect();
                if s.chars().count() > 400 {
                    text.push_str(" …");
                }
                text
            })
            .collect()
    };
    Ok(RestorePreview {
        id: id.clone(),
        path: path.into(),
        index_layer,
        binary,
        hunks,
        lines: Arc::new(lines),
        before_len: before.as_ref().map_or(0, |(_, b)| b.len()),
        saved_len: saved.as_ref().map_or(0, |(_, b)| b.len()),
        before: before.map(|(m, b)| (m, Arc::new(b))),
        saved: saved.map(|(m, b)| (m, Arc::new(b))),
        index: Arc::new(now.cache),
        shared: now.shared.map(|(name, bytes)| (name, Arc::new(bytes))),
        head: now.head,
        branch: now.branch,
    })
}

pub fn restore(
    repo: &Path,
    view: &RestorePreview,
    selected: &[usize],
    active: &dyn Fn() -> bool,
) -> Result<Id, String> {
    if selected.is_empty() {
        return Err(crate::i18n::text("backend-select-at-least-one-hunk-f99c15").into());
    }
    let selected: BTreeSet<_> = selected.iter().copied().collect();
    if selected.iter().any(|i| *i >= view.hunks.len()) {
        return Err(crate::i18n::text("backend-invalid-recovery-hunk-selection-a77e8e").into());
    }
    let ctx = Context::new(repo, active)?;
    let check = || -> Result<Source, String> {
        current(&ctx, &view.id, active)?;
        let now = source(&ctx, active)?;
        if now.cache.as_slice() != view.index.as_slice()
            || now
                .shared
                .as_ref()
                .map(|(name, bytes)| (name, bytes.as_slice()))
                != view
                    .shared
                    .as_ref()
                    .map(|(name, bytes)| (name, bytes.as_slice()))
            || now.head != view.head
            || now.branch != view.branch
        {
            return Err(crate::i18n::text(
                "backend-head-or-staged-contents-changed-since-preview-reopen-the-recovery-cf955e",
            )
            .into());
        }
        included(&ctx, &now, &view.path, active)?;
        let current = read_file(
            &ctx,
            &view.path,
            now.index.get(&view.path).map_or("100644", |e| &e.mode),
            active,
        )?;
        if current.as_ref().map(|(m, b)| (m, b.as_slice()))
            != view.before.as_ref().map(|(m, b)| (m, b.as_slice()))
        {
            return Err(crate::i18n::text(
                "backend-the-working-file-changed-since-preview-reopen-the-recovery-previe-41803a",
            )
            .into());
        }
        Ok(now)
    };
    let now = check()?;
    let mut bytes = Vec::new();
    let mut from = 0;
    let before = view.before.as_ref().map_or(&[][..], |(_, b)| b.as_slice());
    for i in &selected {
        let h = &view.hunks[*i];
        if h.range.start < from || h.range.end > before.len() {
            return Err(crate::i18n::text("backend-incompatible-recovery-ranges-d5d45b").into());
        }
        bytes.extend_from_slice(&before[from..h.range.start]);
        bytes.extend_from_slice(&h.replacement);
        from = h.range.end;
    }
    bytes.extend_from_slice(&before[from..]);
    if bytes.len() > FILE_LIMIT {
        return Err(crate::i18n::text(
            "backend-selected-hunks-exceed-the-8-mib-recovery-limit-9f4710",
        )
        .into());
    }
    let target = if view.saved.is_none() && selected.len() == view.hunks.len() {
        None
    } else {
        Some((
            view.before
                .as_ref()
                .or(view.saved.as_ref())
                .map_or("100644", |(m, _)| m)
                .to_owned(),
            bytes,
        ))
    };
    let before_owned = view
        .before
        .as_ref()
        .map(|(m, b)| (m.clone(), b.as_ref().clone()));
    let raw = diff(
        &ctx,
        before_owned.as_ref(),
        target.as_ref(),
        view.binary,
        active,
    )?;
    let patch = repath(&raw, &view.path, before_owned.is_none(), target.is_none())?;
    if patch.is_empty() {
        return Err(crate::i18n::text("backend-no-content-changes-to-recover-eaa12d").into());
    }
    let permissions = if view.before.is_some() && target.is_some() {
        Some(super::permissions::Pin::capture(repo, &view.path, active)?)
    } else {
        None
    };
    let backup = capture_preserving(
        repo,
        "Before recovering earlier contents",
        Some(&view.id),
        Some(&view.path),
        active,
    )?
    .ok_or(crate::i18n::text(
        "backend-could-not-protect-the-current-file-8bbe18",
    ))?;
    check()?;
    tree(&ctx, &now.index, active)?;
    // Git 2.52 apply crashes with --attr-source and an explicit private Git directory.
    // Its highest-precedence info/attributes gives the same raw-byte isolation without
    // that option, overriding even working-tree-encoding/eol rules in the real checkout.
    raw_attributes(&ctx, active)?;
    let attributes = format!(
        "core.attributesFile={}",
        remote::git_path(&ctx.path.join("empty"))
    );
    let base = [
        "-c",
        attributes.as_str(),
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.safecrlf=false",
        "apply",
        "--binary",
        "--whitespace=nowarn",
    ];
    let mut args = base.to_vec();
    args.extend(["--check", "-"]);
    ctx.work(&args, patch.clone(), &[0], active)?;
    check()?;
    let mut args = base.to_vec();
    args.push("-");
    ctx.work(&args, patch, &[0], active)?;
    if let (Some(pin), Some((_, bytes))) = (permissions, target.as_ref()) {
        pin.restore(&view.path, bytes, active).map_err(|error| {
            crate::i18n::message(
                "backend-contents-recovered-and-safety-draft-saved-but-permission-restorat-c326e3",
                &[("detail", (error).to_string().into())],
            )
        })?;
    }
    Ok(backup.id)
}

pub(super) fn included(
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
fn scratch(
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
fn diff(
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
fn repath(raw: &[u8], path: &str, create: bool, delete: bool) -> Result<Vec<u8>, String> {
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
fn offsets(bytes: &[u8]) -> Vec<usize> {
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
fn parse_hunks(text: &str, before: &[u8], saved: &[u8]) -> Result<Vec<RestoreHunk>, String> {
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

pub(super) fn raw_attributes(ctx: &Context, active: &dyn Fn() -> bool) -> Result<(), String> {
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
