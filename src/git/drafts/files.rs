//! Bounded raw file reads and local/remote path validation.

use super::*;

/// Missing is a distinct, verified result, not a failed read through a link or
/// unsearchable parent. The final read still opens and verifies its own handle.
pub(super) fn read_script(root: &str, rel: &str, limit: u64) -> Result<String, String> {
    valid_path(rel)?;
    let names: Vec<_> = rel.split('/').collect();
    let mut script = format!(
        "cd -P -- {} || exit 1\nroot=$(pwd -P) || exit 1\n[ \"$root\" = / ] && root=\n",
        remote::sh_path(root)
    );
    let mut at = String::new();
    for (i, name) in names.iter().enumerate() {
        let q = remote::sh_quote(name);
        script.push_str(&format!("[ -x . ] || exit 1\n[ -L {q} ] && exit 1\nif [ ! -e {q} ]; then printf 'M\\n'; exit 0; fi\n"));
        if i + 1 < names.len() {
            at.push('/');
            at.push_str(name);
            script.push_str(&format!(
                "cd -P -- {q} || exit 1\n[ \"$(pwd -P)\" = \"$root\"{} ] || exit 1\n",
                remote::sh_quote(&at)
            ));
        } else {
            script.push_str(&format!(
                "if [ -x {q} ]; then printf 'F100755\\n'; else printf 'F100644\\n'; fi\n"
            ));
        }
    }
    script.push_str(
        &remote::read_within_script(root, rel, limit)
            .ok_or(crate::i18n::text("backend-unsafe-draft-path-12307a"))?,
    );
    Ok(script)
}

#[cfg(unix)]
pub(super) fn missing_inside(root: &Path, rel: &str) -> Result<(), String> {
    use rustix::fs::{AtFlags, Mode, OFlags, open, openat, statat};
    let names: Vec<_> = rel.split('/').collect();
    let (last, folders) = names
        .split_last()
        .ok_or(crate::i18n::text("backend-invalid-draft-path-58bc4d"))?;
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW;
    let mut dir = open(root, flags - OFlags::NOFOLLOW, Mode::empty()).map_err(|e| e.to_string())?;
    for folder in folders {
        match openat(&dir, *folder, flags, Mode::empty()) {
            Ok(next) => dir = next,
            Err(rustix::io::Errno::NOENT) => return Ok(()),
            Err(e) => {
                return Err(crate::i18n::message(
                    "backend-cannot-verify-missing-draft-path-e-599e53",
                    &[("detail", (e).to_string().into())],
                ));
            }
        }
    }
    match statat(&dir, *last, AtFlags::SYMLINK_NOFOLLOW) {
        Err(rustix::io::Errno::NOENT) => Ok(()),
        _ => Err(
            crate::i18n::text("backend-draft-path-changed-or-cannot-be-safely-read-7b72b8").into(),
        ),
    }
}

#[cfg(not(unix))]
pub(super) fn missing_inside(root: &Path, rel: &str) -> Result<(), String> {
    let mut path = root.to_path_buf();
    for component in rel.split('/') {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err(crate::i18n::text(
                            "backend-draft-path-crosses-a-reparse-point-dd9a8e",
                        )
                        .into());
                    }
                }
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(crate::i18n::text(
                        "backend-draft-path-changed-or-crosses-a-link-7acb39",
                    )
                    .into());
                }
            }
        }
    }
    Err(crate::i18n::text("backend-draft-path-changed-retry-after-a-pause-e68c36").into())
}

pub(super) fn valid_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        || path
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | "..") || p.eq_ignore_ascii_case(".git"))
    {
        Err(crate::i18n::message(
            "backend-drafts-cannot-safely-read-this-path-path-6f6fed",
            &[("path", format!("{:?}", path).into())],
        ))
    } else {
        Ok(())
    }
}

pub(super) fn oid_valid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(super) fn read_file(
    ctx: &Context,
    path: &str,
    fallback: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<(String, Vec<u8>)>, String> {
    read_file_limited(ctx, path, fallback, FILE_LIMIT, active)
}

pub(super) fn read_file_limited(
    ctx: &Context,
    path: &str,
    fallback: &str,
    limit: usize,
    active: &dyn Fn() -> bool,
) -> Result<Option<(String, Vec<u8>)>, String> {
    valid_path(path)?;
    let limit = limit.min(FILE_LIMIT);
    if !active() {
        return Err("Draft cancelled".into());
    }
    if let Some(target) = remote::parse(&ctx.repo) {
        let script = read_script(&ctx.worktree, path, limit as u64)?;
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                cwd: Some(ctx.worktree.clone()),
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
            && out.stdout.len() - 8 <= limit
            && (out.stdout.starts_with(b"F100644\n") || out.stdout.starts_with(b"F100755\n"))
        {
            return Ok(Some((
                String::from_utf8_lossy(&out.stdout[1..7]).into_owned(),
                out.stdout[8..].to_vec(),
            )));
        }
        return Err(crate::i18n::message(
            "backend-cannot-draft-path-only-regular-files-within-8-mib-are-supported-l-80a0a0",
            &[("path", (path).to_string().into())],
        ));
    }
    let root = Path::new(&ctx.worktree);
    let meta = match std::fs::symlink_metadata(root.join(path)) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return missing_inside(root, path).map(|()| None);
        }
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(crate::i18n::message(
            "backend-cannot-draft-path-symbolic-links-submodules-and-special-files-are-b6ff13",
            &[("path", (path).to_string().into())],
        ));
    }
    if meta.len() > limit as u64 {
        return Err(crate::i18n::message(
            "backend-cannot-draft-path-exceeds-remaining-snapshot-allowance-of-limit-b-dc54e5",
            &[
                ("path", (path).to_string().into()),
                ("limit", (limit).into()),
            ],
        ));
    }
    let bytes = crate::safe_read::read_inside(root, path, limit as u64).ok_or_else(|| {
        crate::i18n::message(
            "backend-cannot-draft-path-file-changed-is-linked-or-exceeds-its-remaining-294df6",
            &[("path", (path).to_string().into())],
        )
    })?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 != 0 {
            "100755"
        } else {
            "100644"
        }
    };
    #[cfg(not(unix))]
    let mode = fallback;
    #[cfg(unix)]
    let _ = fallback;
    Ok(Some((mode.into(), bytes)))
}
