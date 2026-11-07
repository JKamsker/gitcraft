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
    pub symlinks: String,
    pub intent_to_add: bool,
    pub attributes_stamp: String,
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
    let symlinks = symlinks(repo, active)?;
    if symlinks == "false" && second.mode.as_deref() == Some("120000") && third.regular() {
        // Git checks links out as regular target-text files on these repositories,
        // while git add preserves the indexed link type. Keep the physical read
        // separately in work_raw for stale/type checks and never follow a target.
        third.mode = Some("120000".into());
    }
    let (staging_warning, attributes_stamp) = attribute_state(repo, &spec.path, active)?;
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
        symlinks,
        intent_to_add,
        attributes_stamp,
        versions: [first, second, third],
        staging_warning,
    })
}

fn intent_to_add(repo: &Path, path: &str, active: &dyn Fn() -> bool) -> Result<bool, String> {
    let names = |visibility| {
        run(
            repo,
            &[
                "--no-lazy-fetch",
                "--literal-pathspecs",
                "diff",
                "--cached",
                "--name-only",
                "-z",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                visibility,
                "--",
                path,
            ],
            active,
        )
    };
    let visible = names("--ita-visible-in-index")?;
    let invisible = names("--ita-invisible-in-index")?;
    Ok(!visible.is_empty() && invisible.is_empty())
}

/// Partial patches use raw bytes; a filter/encoding conversion belongs to normal Git staging.
pub(super) fn attribute_state(
    repo: &Path,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<(Option<String>, String), String> {
    require_attribute_sources(repo, path, active)?;
    let response = output(
        repo,
        &[
            "--no-lazy-fetch",
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
    // Git can report unreadable attribute blobs on stderr while exiting zero.
    // Unknown conversion rules must not become permission to stage raw bytes.
    if response.code != 0 || !response.stderr.is_empty() {
        return Err(format!(
            "Could not read file attributes without downloading objects: {}",
            String::from_utf8_lossy(&response.stderr).trim()
        ));
    }
    let data = response.stdout;
    let fields: Vec<_> = data.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
    if !fields.len().is_multiple_of(3) {
        return Err("Invalid attribute response".into());
    }
    let conversions = fields
        .chunks(3)
        .any(|f| f[2] != b"unspecified" && f[2] != b"unset");
    let autocrlf = output(repo, &["config", "--get", "core.autocrlf"], active)?;
    if !matches!(autocrlf.code, 0 | 1) || !autocrlf.stderr.is_empty() {
        return Err("Could not establish the file's EOL conversion settings".into());
    }
    let value = String::from_utf8_lossy(&autocrlf.stdout);
    let warning = (conversions || (!value.trim().is_empty() && value.trim() != "false"))
        .then(|| "Line staging is unavailable with clean filters, text/EOL or working-tree-encoding conversion. Use the normal patch to stage with Git's conversions.".into());
    let mut state = data;
    state.extend_from_slice(b"autocrlf\0");
    state.extend_from_slice(&autocrlf.stdout);
    Ok((warning, format!("{:x}", md5::compute(state))))
}

/// With lazy fetch disabled, check-attr silently treats an unavailable indexed
/// attribute blob as absent (success, no diagnostics). Check every governing
/// directory's working file and possible index fallback before trusting conversion
/// rules. Requiring the fallback even with a working file also makes removal of
/// that working file safe during the later locked remote transaction.
fn require_attribute_sources(
    repo: &Path,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let (_, entries) = index(repo, active)?;
    let mut parent = String::new();
    for component in path.split('/') {
        let name = format!("{parent}.gitattributes");
        let working = working(repo, &name, active)?;
        if working.warning.is_some() || (working.mode.is_some() && !working.regular()) {
            return Err(format!("File attributes are not safely readable: {name}"));
        }
        if let Some(entry) = entries.get(name.as_bytes()) {
            if !matches!(entry.mode.as_str(), "100644" | "100755") {
                return Err(format!("Unsupported indexed attributes: {name}"));
            }
            blob(repo, Some(entry), active).map_err(|_| format!(
                "File attributes are unavailable without downloading objects: {name}. Fetch explicitly before using the staging lens."
            ))?;
        }
        parent.push_str(component);
        parent.push('/');
    }
    Ok(())
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
    boolean_config(repo, "core.filemode", active)
}
pub(super) fn symlinks(repo: &Path, active: &dyn Fn() -> bool) -> Result<String, String> {
    boolean_config(repo, "core.symlinks", active)
}
fn boolean_config(repo: &Path, key: &str, active: &dyn Fn() -> bool) -> Result<String, String> {
    let out = output(repo, &["config", "--bool", "--get", key], active)?;
    if !matches!(out.code, 0 | 1) {
        return Err(format!("Could not read {key}"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}
