//! Compare-and-apply against a locked index. Only the user's frozen raw patch is written.
use super::*;
use base64::Engine;
use std::io::{Read, Write};
use std::path::PathBuf;

const INDEX_LIMIT: u64 = 16 * 1024 * 1024;
pub(crate) fn quote_path(prefix: &str, path: &str) -> String {
    let mut out = String::from("\"");
    for b in prefix.bytes().chain(path.bytes()) {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(char::from(b)),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push('"');
    out
}
pub(crate) fn patch_header(path: &str, old: Option<&str>, new: Option<&str>) -> String {
    let a = quote_path("a/", path);
    let b = quote_path("b/", path);
    let mut header = format!("diff --git {a} {b}\n");
    match (old, new) {
        (None, Some(mode)) => header.push_str(&format!("new file mode {mode}\n")),
        (Some(mode), None) => header.push_str(&format!("deleted file mode {mode}\n")),
        (Some(a), Some(b)) if a != b => header.push_str(&format!("old mode {a}\nnew mode {b}\n")),
        _ => (),
    }
    header.push_str(&format!(
        "--- {}\n+++ {}\n",
        if old.is_some() {
            a.as_str()
        } else {
            "/dev/null"
        },
        if new.is_some() {
            b.as_str()
        } else {
            "/dev/null"
        }
    ));
    header
}
fn verify_patch(path: &str, patch: &str) -> Result<(), String> {
    let first = format!(
        "diff --git {} {}",
        quote_path("a/", path),
        quote_path("b/", path)
    );
    if patch.lines().next() != Some(first.as_str())
        || patch.contains("\ndiff --git ")
        || patch
            .lines()
            .any(|l| l.starts_with("rename ") || l.starts_with("copy "))
    {
        return Err("The selected patch no longer belongs to this file".into());
    }
    let a = format!("--- {}", quote_path("a/", path));
    let b = format!("+++ {}", quote_path("b/", path));
    if !patch.lines().any(|l| l == a || l == "--- /dev/null")
        || !patch.lines().any(|l| l == b || l == "+++ /dev/null")
    {
        return Err("Invalid staging boundary header".into());
    }
    Ok(())
}
fn verify(repo: &Path, lens: &Lens) -> Result<(), String> {
    if lens.staging_warning.is_some() {
        return Err("Use normal Git staging for files with conversions".into());
    }
    if super::head(repo, &|| true)? != lens.head
        || index(repo, &|| true)?.0 != lens.index
        || lens::working(repo, &lens.spec.path, &|| true)? != lens.work_raw
        || index_stamp(repo, &|| true)? != lens.index_stamp
        || lens::filemode(repo, &|| true)? != lens.filemode
        || lens::attributes(repo, &lens.spec.path, &|| true)?.is_some()
    {
        return Err("The staging lens is stale. Refresh before staging or unstaging.".into());
    }
    Ok(())
}

pub(crate) fn apply_lens_patch(
    repo: &Path,
    lens: &Lens,
    patch: &str,
    reverse: bool,
) -> Result<String, String> {
    verify_patch(&lens.spec.path, patch)?;
    verify(repo, lens)?;
    if let Some(target) = crate::remote::parse(repo) {
        return remote_apply(&target, lens, patch, reverse);
    }
    let index = lens::index_path(repo, &|| true)?;
    let lock = PathBuf::from(format!("{}.lock", index.to_string_lossy()));
    let mut reserved = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|e| format!("Could not lock the Git index: {e}"))?;
    let mut guard = IndexLock {
        path: lock.clone(),
        committed: false,
    };
    verify(repo, lens)?;
    let temp = Scratch::new()?;
    let staged = temp.0.join("index");
    match read_index(&index)? {
        Some(bytes) => std::fs::write(&staged, bytes).map_err(|e| e.to_string())?,
        None => {
            alternate(repo, &staged, &["read-tree", "--empty"])?;
        }
    }
    let file = temp.0.join("patch");
    std::fs::write(&file, patch).map_err(|e| e.to_string())?;
    let file = file.to_str().ok_or("Non-UTF8 temporary patch path")?;
    let mut args = vec!["apply", "--cached", "--whitespace=nowarn"];
    if reverse {
        args.push("--reverse");
    }
    args.extend(["--", file]);
    alternate(repo, &staged, &args)?;
    // Git writers are excluded by index.lock. A changed HEAD/file/attribute is refused too.
    verify(repo, lens)?;
    let bytes = read_index(&staged)?.ok_or("Git did not produce an index")?;
    reserved
        .write_all(&bytes)
        .and_then(|_| reserved.sync_all())
        .map_err(|e| e.to_string())?;
    drop(reserved);
    std::fs::rename(&lock, &index).map_err(|e| e.to_string())?;
    guard.committed = true;
    Ok(String::new())
}
fn alternate(repo: &Path, index: &Path, args: &[&str]) -> Result<(), String> {
    let out = super::super::command::comparison_output(
        repo,
        args,
        &[("GIT_INDEX_FILE", index.to_string_lossy().into_owned())],
        &|| true,
    )?;
    if out.code == 0 {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().into())
    }
}
fn read_index(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Symbolic-link index files are not modified".into());
    }
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Git index is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(INDEX_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > INDEX_LIMIT {
        return Err("The index exceeds the 16 MiB staging transaction limit".into());
    }
    Ok(Some(bytes))
}
struct IndexLock {
    path: PathBuf,
    committed: bool,
}
impl Drop for IndexLock {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub(crate) struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Result<Self, String> {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "gitcraft-file-compare-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("Could not create a private comparison folder".into())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn remote_apply(
    target: &crate::remote::Target,
    lens: &Lens,
    patch: &str,
    reverse: bool,
) -> Result<String, String> {
    let b64 = |v: &[u8]| base64::engine::general_purpose::STANDARD.encode(v);
    let mut work = lens
        .work_raw
        .mode
        .as_deref()
        .unwrap_or("missing")
        .as_bytes()
        .to_vec();
    work.push(0);
    work.extend(&lens.work_raw.bytes);
    if lens.work_raw.warning.is_some() {
        return Err("This working version cannot be staged safely".into());
    }
    let working = lens::working_script(&target.path, &lens.spec.path)?;
    let head = crate::remote::sh_quote(lens.head.as_deref().unwrap_or(""));
    // One exclusive index.lock across verification, alternate-index apply and replacement.
    // All scratch names are constants; file paths and payloads are quoted/data, not code.
    let script = format!(
        "umask 077\nd=$(mktemp -d) || exit 1\nown=0\nidx=$(git rev-parse --path-format=absolute --git-path index) || exit 1\nlock=\"$idx.lock\"\ntrap '[ \"$own\" = 1 ] && rm -f -- \"$lock\"; rm -rf -- \"$d\"' 0\n\
         if (set -C; : >\"$lock\") 2>/dev/null; then own=1; else echo 'Could not lock the Git index' >&2; exit 1; fi\n\
         printf '%s' '{index_b64}' | base64 -d >\"$d/expected-index\" || exit 1\n\
         printf '%s' '{work_b64}' | base64 -d >\"$d/expected-work\" || exit 1\n\
         printf '%s' '{patch_b64}' | base64 -d >\"$d/patch\" || exit 1\n\
         check_snapshot() {{\n\
          current=$(git rev-parse --verify -q HEAD || :)\n[ \"$current\" = {head} ] || return 1\n\
          {stamp_check}\n\
          git ls-files --stage -z >\"$d/current-index\" && cmp -s \"$d/expected-index\" \"$d/current-index\" || return 1\n\
          ( {working} ) >\"$d/current-work\" && cmp -s \"$d/expected-work\" \"$d/current-work\" || return 1\n}}\n\
         check_snapshot || {{ echo 'The staging lens is stale; refresh it' >&2; exit 1; }}\n\
         [ -L \"$idx\" ] && {{ echo 'Symbolic-link index files are not modified' >&2; exit 1; }}\n\
         if [ -f \"$idx\" ]; then [ \"$(wc -c <\"$idx\")\" -le {index_limit} ] && cp -- \"$idx\" \"$d/index\" || exit 1; else GIT_INDEX_FILE=\"$d/index\" git read-tree --empty || exit 1; fi\n\
         GIT_INDEX_FILE=\"$d/index\" git apply --cached --whitespace=nowarn {reverse} -- \"$d/patch\" || exit 1\n\
         check_snapshot || {{ echo 'The staging lens changed before the patch was installed; refresh it' >&2; exit 1; }}\n\
         [ \"$(wc -c <\"$d/index\")\" -le {index_limit} ] || exit 1\n\
         cat \"$d/index\" >\"$lock\" && mv -f -- \"$lock\" \"$idx\" || exit 1\nown=0\n",
        stamp_check = remote_stamp_check(&lens.index_stamp),
        index_b64 = b64(&lens.index),
        work_b64 = b64(&work),
        patch_b64 = b64(patch.as_bytes()),
        index_limit = INDEX_LIMIT,
        reverse = if reverse { "--reverse" } else { "" }
    );
    let out = super::super::command::comparison_remote(
        &target.host,
        crate::remote::Request {
            cwd: Some(target.path.clone()),
            script,
            ..Default::default()
        },
        &|| true,
    )?;
    if out.code == 0 {
        Ok(String::new())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().into())
    }
}

pub(super) fn index_stamp(repo: &Path, active: &dyn Fn() -> bool) -> Result<String, String> {
    if let Some(target) = crate::remote::parse(repo) {
        let script = format!(
            "umask 077\nd=$(mktemp -d) || exit 1\ntrap 'rm -rf -- \"$d\"' 0\nidx=$(git rev-parse --path-format=absolute --git-path index) || exit 1\n{}\nprintf '%s' \"$actual\"",
            remote_stamp_read()
        );
        let out = super::super::command::comparison_remote(
            &target.host,
            crate::remote::Request {
                cwd: Some(target.path.clone()),
                script,
                ..Default::default()
            },
            active,
        )?;
        if out.code != 0 {
            return Err("Could not fingerprint the Git index safely (16 MiB limit)".into());
        }
        let stamp = String::from_utf8(out.stdout).map_err(|_| "Invalid index fingerprint")?;
        if stamp != "missing"
            && !(stamp.len() == 32 && stamp.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid index fingerprint".into());
        }
        return Ok(stamp);
    }
    let index = lens::index_path(repo, active)?;
    Ok(read_index(&index)?
        .map(|b| format!("{:x}", md5::compute(b)))
        .unwrap_or_else(|| "missing".into()))
}
fn remote_stamp_read() -> String {
    format!(
        "[ -L \"$idx\" ] && exit 1\nif [ -e \"$idx\" ]; then\n[ -f \"$idx\" ] || exit 1\nhead -c {} \"$idx\" >\"$d/index-bytes\" || exit 1\n[ \"$(wc -c <\"$d/index-bytes\")\" -le {} ] || exit 1\nactual=$(md5sum \"$d/index-bytes\") || exit 1\nactual=${{actual%% *}}\nelse actual=missing; fi\n",
        INDEX_LIMIT + 1,
        INDEX_LIMIT
    )
}
fn remote_stamp_check(expected: &str) -> String {
    format!(
        "{}\n[ \"$actual\" = {} ] || return 1",
        remote_stamp_read(),
        crate::remote::sh_quote(expected)
    )
}
