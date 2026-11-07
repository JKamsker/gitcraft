use super::*;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LensSpec {
    pub path: String,
    pub previous: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Lens {
    pub spec: LensSpec,
    pub head: Option<String>,
    pub index: Vec<u8>,
    pub index_stamp: String,
    pub work_raw: Version,
    pub filemode: String,
    pub intent_to_add: bool,
    pub versions: [Version; 3],
    pub staging_warning: Option<String>,
}

pub(crate) fn read_lens(
    repo: &Path,
    spec: &LensSpec,
    active: &dyn Fn() -> bool,
) -> Result<Lens, String> {
    if !valid_path(repo, &spec.path)
        || spec.path.contains(char::REPLACEMENT_CHARACTER)
        || spec.previous.as_ref().is_some_and(|p| !valid_path(repo, p))
    {
        return Err(
            "This working-tree path cannot be read safely on this machine; use Git in the terminal"
                .into(),
        );
    }
    let head = head(repo, active)?;
    let index_stamp = super::transaction::index_stamp(repo, active)?;
    let (before, entries) = index(repo, active)?;
    let old_path = spec.previous.as_deref().unwrap_or(&spec.path);
    let head_entry = match &head {
        Some(h) => tree(repo, h, old_path, active)?.remove(old_path.as_bytes()),
        None => None,
    };
    // Intent-to-add entries contain the empty-blob OID but are absent from the
    // next commit. Ask Git to interpret its index flags instead of displaying a
    // staged empty file or decoding a platform/version-specific index format.
    let intent_to_add = intent_to_add(repo, &spec.path, active)?;
    let staged = entries.get(spec.path.as_bytes()).filter(|_| !intent_to_add);
    let first = blob(repo, head_entry.as_ref(), active)?;
    let second = blob(repo, staged, active)?;
    let work_raw = working(repo, &spec.path, active)?;
    let mut third = work_raw.clone();
    let filemode = filemode(repo, active)?;
    if filemode == "false" && third.regular() && second.regular() {
        third.mode = second.mode.clone();
    }
    let staging_warning = attributes(repo, &spec.path, active)?;
    if index(repo, active)?.0 != before
        || super::head(repo, active)? != head
        || working(repo, &spec.path, active)? != work_raw
        || super::transaction::index_stamp(repo, active)? != index_stamp
    {
        return Err(
            "HEAD, index or working file changed while reading. Refresh the staging lens.".into(),
        );
    }
    Ok(Lens {
        spec: spec.clone(),
        head,
        index: before,
        index_stamp,
        work_raw,
        filemode,
        intent_to_add,
        versions: [first, second, third],
        staging_warning,
    })
}

fn intent_to_add(repo: &Path, path: &str, active: &dyn Fn() -> bool) -> Result<bool, String> {
    let names = |visibility| {
        run(repo, &[
            "--no-lazy-fetch", "--literal-pathspecs", "diff", "--cached", "--name-only", "-z",
            "--no-ext-diff", "--no-textconv", "--no-renames", visibility, "--", path,
        ], active)
    };
    let visible = names("--ita-visible-in-index")?;
    let invisible = names("--ita-invisible-in-index")?;
    Ok(!visible.is_empty() && invisible.is_empty())
}

/// Partial patches use raw bytes; a filter/encoding conversion belongs to normal Git staging.
pub(super) fn attributes(
    repo: &Path,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<String>, String> {
    let data = run(
        repo,
        &[
            "--literal-pathspecs",
            "check-attr",
            "-z",
            "filter",
            "working-tree-encoding",
            "eol",
            "text",
            "--",
            path,
        ],
        active,
    )?;
    let fields: Vec<_> = data.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
    if !fields.len().is_multiple_of(3) {
        return Err("Invalid attribute response".into());
    }
    let conversions = fields
        .chunks(3)
        .any(|f| f[2] != b"unspecified" && f[2] != b"unset");
    let autocrlf = output(repo, &["config", "--get", "core.autocrlf"], active)?;
    let autocrlf = String::from_utf8_lossy(&autocrlf.stdout);
    Ok((conversions||(!autocrlf.trim().is_empty()&&autocrlf.trim()!="false")).then(||"Line staging is unavailable with clean filters, text/EOL or working-tree-encoding conversion. Use the normal patch to stage with Git's conversions.".into()))
}

pub(super) fn working(
    repo: &Path,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<Version, String> {
    if !active() {
        return Err("Comparison cancelled".into());
    }
    if let Some(target) = crate::remote::parse(repo) {
        let out = super::super::command::comparison_remote(
            &target.host,
            crate::remote::Request {
                cwd: Some(target.path.clone()),
                script: working_script(&target.path, path)?,
                ..Default::default()
            },
            active,
        )?;
        if out.code != 0 {
            return Err(format!(
                "Could not read the working file safely: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let split = out
            .stdout
            .iter()
            .position(|b| *b == 0)
            .ok_or("Invalid working-file response")?;
        let mode =
            std::str::from_utf8(&out.stdout[..split]).map_err(|_| "Invalid working-file mode")?;
        if mode == "missing" {
            return Ok(Version::missing());
        }
        if mode == "unsupported" {
            return Ok(unavailable());
        }
        if !matches!(mode, "100644" | "100755" | "120000") {
            return Err("Invalid working-file type".into());
        }
        let bytes = out.stdout[split + 1..].to_vec();
        if bytes.len() > FILE_LIMIT {
            return Err("File comparison is limited to 2 MiB per version".into());
        }
        return Ok(Version {
            mode: Some(mode.into()),
            oid: None,
            bytes,
            warning: None,
        });
    }
    let mut at = repo.to_owned();
    let parts: Vec<_> = path.split('/').collect();
    for part in &parts[..parts.len().saturating_sub(1)] {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.file_type().is_symlink() || !m.is_dir() => return Ok(unavailable()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Version::missing()),
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
    }
    let file = repo.join(path);
    let metadata = match std::fs::symlink_metadata(&file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Version::missing()),
        Err(e) => return Err(e.to_string()),
    };
    let (mode, bytes) = if metadata.file_type().is_symlink() {
        let Some(bytes) = link_inside(repo, path) else {
            return Ok(unavailable());
        };
        ("120000", bytes)
    } else if metadata.is_file() {
        let Some(bytes) = crate::safe_read::read_inside(repo, path, FILE_LIMIT as u64) else {
            return Ok(unavailable());
        };
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        (if executable { "100755" } else { "100644" }, bytes)
    } else {
        return Ok(unavailable());
    };
    Ok(Version {
        mode: Some(mode.into()),
        oid: None,
        bytes,
        warning: None,
    })
}
fn unavailable() -> Version {
    Version{mode:Some("unknown".into()),oid:None,bytes:Vec::new(),warning:Some("Not a safely readable regular file/link within 2 MiB; no staging actions are available".into())}
}
#[cfg(unix)]
fn link_inside(repo: &Path, path: &str) -> Option<Vec<u8>> {
    use rustix::fs::{Mode, OFlags, open, openat, readlinkat};
    let mut parts: Vec<_> = path.split('/').collect();
    let last = parts.pop()?;
    let mut dir = open(
        repo,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
        Mode::empty(),
    )
    .ok()?;
    for part in parts {
        dir = openat(
            &dir,
            part,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .ok()?;
    }
    let bytes = readlinkat(&dir, last, Vec::new()).ok()?.into_bytes();
    (bytes.len() <= FILE_LIMIT).then_some(bytes)
}
#[cfg(not(unix))]
fn link_inside(_: &Path, _: &str) -> Option<Vec<u8>> {
    None
}

/// Physical-directory checks and held fd prevent following a changed parent or final symlink.
pub(super) fn working_script(repo: &str, path: &str) -> Result<String, String> {
    let parts: Vec<_> = path.split('/').collect();
    if parts.is_empty()
        || parts
            .iter()
            .any(|p| p.is_empty() || matches!(*p, "." | ".."))
        || path.starts_with('/')
        || path.contains('\0')
    {
        return Err("Invalid working path".into());
    }
    let mut script = format!(
        "cd -P -- {} || exit 1\nroot=$(pwd -P) || exit 1\n[ \"$root\" = / ] && root=\n",
        crate::remote::sh_path(repo)
    );
    let mut at = String::new();
    for part in &parts[..parts.len() - 1] {
        let q = crate::remote::sh_quote(part);
        at.push('/');
        at.push_str(part);
        script.push_str(&format!("if [ ! -e {q} ] && [ ! -L {q} ]; then printf 'missing\\0'; exit 0; fi\n[ -L {q} ] && {{ printf 'unsupported\\0'; exit 0; }}\ncd -P -- {q} || exit 1\n[ \"$(pwd -P)\" = \"$root\"{} ] || exit 1\n",crate::remote::sh_quote(&at)));
    }
    let f = crate::remote::sh_quote(parts[parts.len() - 1]);
    script.push_str(&format!(concat!(
        "if [ -L {f} ]; then printf '120000\\0'; readlink -n -- {f}; exit $?; fi\n",
        "if [ ! -e {f} ]; then printf 'missing\\0'; exit 0; fi\n",
        "[ -f {f} ] || {{ printf 'unsupported\\0'; exit 0; }}\n",
        "exec 3< {f} || exit 1\n",
        "if [ -e /proc/self/fd/3 ]; then [ \"$(readlink /proc/self/fd/3)\" = \"$(pwd -P)\"/{f} ] || exit 1; else [ -L {f} ] && exit 1; fi\n",
        "if [ -x {f} ]; then printf '100755\\0'; else printf '100644\\0'; fi\n",
        "exec head -c {max} <&3\n"),f=f,max=FILE_LIMIT+1));
    Ok(script)
}

pub(super) fn index_path(repo: &Path, active: &dyn Fn() -> bool) -> Result<PathBuf, String> {
    let raw = run(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        active,
    )?;
    // Git terminates its path with exactly one LF; a newline in the directory is data.
    let raw = raw.strip_suffix(b"\n").unwrap_or(&raw);
    let path = std::str::from_utf8(raw).map_err(|_| "The Git index path is not UTF-8")?;
    Ok(crate::remote::resolve(repo, path))
}

pub(super) fn filemode(repo: &Path, active: &dyn Fn() -> bool) -> Result<String, String> {
    let out = output(
        repo,
        &["config", "--bool", "--get", "core.filemode"],
        active,
    )?;
    if !matches!(out.code, 0 | 1) {
        return Err("Could not read core.filemode".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}
