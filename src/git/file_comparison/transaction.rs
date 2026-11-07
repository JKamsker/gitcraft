//! Compare-and-apply against a locked index. Only the user's frozen raw patch is written.
use super::*;
use base64::Engine;
use std::io::Read;
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
pub(super) fn verify(repo: &Path, lens: &Lens) -> Result<(), String> {
    if lens.versions.iter().any(|v| {
        v.warning.is_some()
            || !matches!(
                v.mode.as_deref(),
                None | Some("100644" | "100755" | "120000")
            )
    }) {
        return Err("This file type is read-only in the staging lens".into());
    }
    if lens.staging_warning.is_some() {
        return Err("Use normal Git staging for files with conversions".into());
    }
    let (warning, attributes_stamp) = lens::attribute_state(repo, &lens.spec.path, &|| true)?;
    if super::head(repo, &|| true)? != lens.head
        || index(repo, &|| true)?.0 != lens.index
        || lens::working(repo, &lens.spec.path, &|| true)? != lens.work_raw
        || index_stamp(repo, &|| true)? != lens.index_stamp
        || lens::filemode(repo, &|| true)? != lens.filemode
        || lens::symlinks(repo, &|| true)? != lens.symlinks
        || warning.is_some()
        || attributes_stamp != lens.attributes_stamp
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
    super::index_transaction::apply(repo, lens, patch, reverse, &mut |_| {})
}

pub(super) fn alternate(repo: &Path, index: &Path, args: &[&str]) -> Result<(), String> {
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
    let parent = path.parent().ok_or("Index has no parent")?;
    let directory =
        crate::standalone::directory::Directory::open(parent).map_err(|e| e.to_string())?;
    let name = path.file_name().ok_or("Index has no name")?;
    let file = match directory.read(name) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read a no-follow regular Git index: {e}")),
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
    let script = remote_apply_script(lens, patch, reverse)?;
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
        Ok(String::from_utf8_lossy(&out.stdout).trim().into())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().into())
    }
}

fn remote_apply_script(lens: &Lens, patch: &str, reverse: bool) -> Result<String, String> {
    if lens.work_raw.warning.is_some() {
        return Err("This working version cannot be staged safely".into());
    }
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
    let payload = serde_json::json!({
        "head":lens.head.as_deref().unwrap_or(""), "index_stamp":lens.index_stamp,
        "index":b64(&lens.index), "work":b64(&work), "patch":b64(patch.as_bytes()),
        "path":lens.spec.path,"attributes_stamp":lens.attributes_stamp,
        "filemode":lens.filemode,"symlinks":lens.symlinks,"intent":lens.intent_to_add,"reverse":reverse,
    });
    Ok(format!(
        "command -v python3 >/dev/null 2>&1 || {{ echo 'Safe remote staging requires Python 3 standard-library no-follow file and ACL APIs; install python3 on the repository machine' >&2; exit 1; }}\npython3 -c {} <<'GITCRAFT_INDEX_DATA'\n{}\nGITCRAFT_INDEX_DATA\n",
        crate::remote::sh_quote(include_str!("remote_index.py")),
        payload
    ))
}

pub(super) fn index_stamp(repo: &Path, active: &dyn Fn() -> bool) -> Result<String, String> {
    if let Some(target) = crate::remote::parse(repo) {
        let script = format!(
            "umask 077\nd=$(mktemp -d) || exit 1\ntrap 'rm -rf -- \"$d\"' 0\nidx=$(git --no-lazy-fetch rev-parse --path-format=absolute --git-path index) || exit 1\n{}\nprintf '%s' \"$actual\"",
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
            if String::from_utf8_lossy(&out.stderr).contains("File comparison requires md5sum") {
                return Err("File comparison requires md5sum (Linux/WSL) or md5 (macOS) on the repository machine".into());
            }
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
        "{}\n[ -L \"$idx\" ] && exit 1\nif [ -e \"$idx\" ]; then\n[ -f \"$idx\" ] || exit 1\nhead -c {} \"$idx\" >\"$d/index-bytes\" || exit 1\n[ \"$(wc -c <\"$d/index-bytes\")\" -le {} ] || exit 1\nactual=$(gitcraft_md5 <\"$d/index-bytes\") || exit 1\nelse actual=missing; fi\n",
        portable_md5(),
        INDEX_LIMIT + 1,
        INDEX_LIMIT
    )
}
fn portable_md5() -> &'static str {
    r#"gitcraft_md5() {
 if command -v md5sum >/dev/null 2>&1; then result=$(md5sum) || return 1; result=${result%% *};
 elif command -v md5 >/dev/null 2>&1; then result=$(md5 -q) || return 1;
 else echo 'File comparison requires md5sum (Linux/WSL) or md5 (macOS)' >&2; return 1; fi
 case "$result" in *[!a-fA-F0-9]*|'') return 1;; esac
 [ "${#result}" = 32 ] || return 1
 printf '%s' "$result"
}
probe=$(printf abc | gitcraft_md5) || exit 1
[ "$probe" = 900150983cd24fb0d6963f7d28e17f72 ] || { echo 'Unsupported MD5 capability' >&2; exit 1; }
"#
}

#[cfg(test)]
mod correction_tests {
    use super::*;
    #[test]
    fn remote_program_records_no_follow_create_only_policy_and_literal_data() -> Result<(), String>
    {
        let r = super::super::tests::Repo::new()?;
        let path = "literal ' $(not-code) ü";
        r.write(path, "old\n")?;
        r.commit()?;
        r.write(path, "new\n")?;
        let lens = r.lens(path)?;
        let script = remote_apply_script(&lens, "fixture patch", false)?;
        assert!(script.contains("requires Python 3"));
        assert!(script.contains("O_NOFOLLOW"));
        assert!(script.contains("O_EXCL"));
        assert!(script.contains("src_dir_fd=recovery,dst_dir_fd=parent,follow_symlinks=False"));
        assert!(script.contains("os.fchown"));
        assert!(script.contains("acl_get_fd_np"));
        assert!(script.contains("core.sharedRepository"));
        assert!(script.contains("released-lock"));
        assert!(!script.contains("cat \"$d/index\" >\"$lock\""));
        let json = script
            .split("<<'GITCRAFT_INDEX_DATA'\n")
            .nth(1)
            .ok_or("payload")?
            .lines()
            .next()
            .ok_or("json")?;
        let data: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        assert_eq!(data["path"], path);
        assert_eq!(
            data["patch"],
            base64::engine::general_purpose::STANDARD.encode(b"fixture patch")
        );
        let md5 = portable_md5();
        assert!(md5.contains("md5sum"));
        assert!(md5.contains("md5 -q"));
        assert!(md5.contains("900150983cd24fb0d6963f7d28e17f72"));
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod owned_remote_correction_tests {
    use super::*;
    use crate::git::file_comparison::tests::Repo;
    use std::os::unix::fs::PermissionsExt;
    fn run(r: &Repo, script: &str, bin: Option<&Path>) -> Result<std::process::Output, String> {
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", script]).current_dir(&r.0);
        if let Some(bin) = bin {
            cmd.env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
        crate::processes::run(
            cmd,
            crate::processes::Options {
                timeout: Some(std::time::Duration::from_secs(20)),
                ..Default::default()
            },
        )?
        .complete()
    }
    #[test]
    fn owned_remote_program_preserves_foreign_locks_and_applies_native_sharing_policy()
    -> Result<(), String> {
        for case in ["0600", "group", "acl", "foreign-file", "foreign-link"] {
            let r = Repo::new()?;
            let path = "a ' $(literal) ü";
            r.write(path, "before\n")?;
            r.write("unrelated", "unchanged\n")?;
            r.commit()?;
            let sharing = if case == "group" { "group" } else { "0600" };
            r.git(&["config", "core.sharedRepository", sharing])?;
            // A no-op add can retain the old index inode/mode. Force native writes
            // before taking the reference policy, just as the real mutation will.
            r.write(path, "native reference rewrite\n")?;
            r.git(&["add", path])?;
            r.write(path, "before\n")?;
            r.git(&["add", path])?;
            #[cfg(target_os = "macos")]
            if case == "acl" {
                exacl::setfacl(
                    &[r.0.join(".git/index")],
                    &[exacl::AclEntry::allow_user(
                        "nobody",
                        exacl::Perm::READ,
                        None,
                    )],
                    None,
                )
                .map_err(|e| e.to_string())?;
            }
            #[cfg(target_os = "macos")]
            let expected_acl =
                exacl::getfacl(r.0.join(".git/index"), exacl::AclOption::SYMLINK_ACL)
                    .map_err(|e| e.to_string())?;
            let expected = std::fs::metadata(r.0.join(".git/index"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777;
            r.write(path, "after\n")?;
            let lens = r.lens(path)?;
            let patch = patch_header(path, Some("100644"), Some("100644"))
                + "@@ -1 +1 @@\n-before\n+after\n";
            let mut bin = None;
            if case.starts_with("foreign") {
                let tools = r.0.join("tools");
                std::fs::create_dir(&tools).map_err(|e| e.to_string())?;
                let real = std::process::Command::new("/bin/sh")
                    .args(["-c", "command -v git"])
                    .output()
                    .map_err(|e| e.to_string())?;
                let real = String::from_utf8(real.stdout).map_err(|e| e.to_string())?;
                let lock = r.0.join(".git/index.lock");
                let displaced = r.0.join("displaced-lock");
                let sentinel = r.0.join("sentinel");
                std::fs::write(&sentinel, b"outside-original").map_err(|e| e.to_string())?;
                let replace = if case == "foreign-link" {
                    format!(
                        "ln -s {} {}",
                        crate::remote::sh_quote(&sentinel.to_string_lossy()),
                        crate::remote::sh_quote(&lock.to_string_lossy())
                    )
                } else {
                    format!(
                        "printf FOREIGN-LOCK-BYTES > {}",
                        crate::remote::sh_quote(&lock.to_string_lossy())
                    )
                };
                let wrapper = format!(
                    "#!/bin/sh\n{} \"$@\"\nrc=$?\ncase \"$*\" in *'apply --cached'*) case \"$GIT_INDEX_FILE\" in {}/.git/gitcraft-index-recovery-*/candidate) mv {} {} && {};; esac;; esac\nexit \"$rc\"\n",
                    crate::remote::sh_quote(real.trim()),
                    crate::remote::sh_quote(&r.0.to_string_lossy()),
                    crate::remote::sh_quote(&lock.to_string_lossy()),
                    crate::remote::sh_quote(&displaced.to_string_lossy()),
                    replace
                );
                let exe = tools.join("git");
                std::fs::write(&exe, wrapper).map_err(|e| e.to_string())?;
                std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
                bin = Some(tools);
            }
            let out = run(
                &r,
                &remote_apply_script(&lens, &patch, false)?,
                bin.as_deref(),
            )?;
            if case.starts_with("foreign") {
                assert!(
                    !out.status.success(),
                    "{case}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert!(std::fs::symlink_metadata(r.0.join(".git/index.lock")).is_ok());
                assert_eq!(
                    std::fs::read(r.0.join("sentinel")).map_err(|e| e.to_string())?,
                    b"outside-original"
                );
                assert_eq!(r.git(&["show", &format!(":{path}")])?, "before");
                if case == "foreign-file" {
                    assert_eq!(
                        std::fs::read(r.0.join(".git/index.lock")).map_err(|e| e.to_string())?,
                        b"FOREIGN-LOCK-BYTES"
                    );
                }
            } else {
                assert!(
                    out.status.success(),
                    "{case}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(r.git(&["show", &format!(":{path}")])?, "after");
                assert_eq!(
                    std::fs::metadata(r.0.join(".git/index"))
                        .map_err(|e| e.to_string())?
                        .permissions()
                        .mode()
                        & 0o777,
                    expected
                );
                assert!(!r.0.join(".git/index.lock").exists());
                #[cfg(target_os = "macos")]
                assert_eq!(
                    exacl::getfacl(r.0.join(".git/index"), exacl::AclOption::SYMLINK_ACL)
                        .map_err(|e| e.to_string())?,
                    expected_acl
                );
            }
            assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
            assert_eq!(
                std::fs::read(r.0.join(path)).map_err(|e| e.to_string())?,
                b"after\n"
            );
        }
        let r = Repo::new()?;
        r.git(&["config", "core.sharedRepository", "0600"])?;
        r.write("new", "new\n")?;
        let lens = r.lens("new")?;
        let patch = patch_header("new", None, Some("100644")) + "@@ -0,0 +1 @@\n+new\n";
        let out = run(&r, &remote_apply_script(&lens, &patch, false)?, None)?;
        assert!(
            out.status.success(),
            "absent index: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(r.git(&["show", ":new"])?, "new");
        assert_eq!(
            std::fs::metadata(r.0.join(".git/index"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        Ok(())
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn stock_darwin_md5_is_accepted_without_gnu_md5sum() -> Result<(), String> {
        let r = Repo::new()?;
        let tools = r.0.join("tools");
        std::fs::create_dir(&tools).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink("/sbin/md5", tools.join("md5")).map_err(|e| e.to_string())?;
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args([
            "-c",
            &format!("{}\nprintf abc | gitcraft_md5", portable_md5()),
        ])
        .env("PATH", &tools);
        let out = crate::processes::run(cmd, crate::processes::Options::default())?.complete()?;
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"900150983cd24fb0d6963f7d28e17f72");
        Ok(())
    }
}
