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
        return Err(crate::i18n::text(
            "backend-the-selected-patch-no-longer-belongs-to-this-file-4bf31f",
        )
        .into());
    }
    let a = format!("--- {}", quote_path("a/", path));
    let b = format!("+++ {}", quote_path("b/", path));
    if !patch.lines().any(|l| l == a || l == "--- /dev/null")
        || !patch.lines().any(|l| l == b || l == "+++ /dev/null")
    {
        return Err(crate::i18n::text("backend-invalid-staging-boundary-header-2ef0ea").into());
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
        return Err(crate::i18n::text(
            "backend-this-file-type-is-read-only-in-the-staging-lens-f7da04",
        )
        .into());
    }
    if lens.staging_warning.is_some() {
        return Err(crate::i18n::text(
            "backend-use-normal-git-staging-for-files-with-conversions-b9ba1f",
        )
        .into());
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
        return Err(crate::i18n::text(
            "backend-the-staging-lens-is-stale-refresh-before-staging-or-unstaging-3bc2ce",
        )
        .into());
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
    let parent = path
        .parent()
        .ok_or(crate::i18n::text("backend-index-has-no-parent-b1c595"))?;
    let directory =
        crate::standalone::directory::Directory::open(parent).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .ok_or(crate::i18n::text("backend-index-has-no-name-a4f6d6"))?;
    let file = match directory.read(name) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(crate::i18n::message(
                "backend-cannot-read-a-no-follow-regular-git-index-e-782528",
                &[("detail", (e).to_string().into())],
            ));
        }
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(crate::i18n::text("backend-git-index-is-not-a-regular-file-3e68c7").into());
    }
    let mut bytes = Vec::new();
    file.take(INDEX_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > INDEX_LIMIT {
        return Err(crate::i18n::text(
            "backend-the-index-exceeds-the-16-mib-staging-transaction-limit-f64011",
        )
        .into());
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
            let builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err(crate::i18n::text("backend-could-not-create-a-private-comparison-folder-bea1e3").into())
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
        return Err(crate::i18n::text(
            "backend-this-working-version-cannot-be-staged-safely-6424e7",
        )
        .into());
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
            if String::from_utf8_lossy(&out.stderr).contains("Safe index reads require Perl") {
                return Err(crate::i18n::text("backend-safe-remote-index-reads-require-perl-with-fcntl-no-follow-support-a25491").into());
            }
            if String::from_utf8_lossy(&out.stderr).contains("File comparison requires md5sum") {
                return Err(crate::i18n::text("backend-file-comparison-requires-md5sum-linux-wsl-or-md5-macos-on-the-rep-b6009f").into());
            }
            return Err(crate::i18n::text(
                "backend-could-not-fingerprint-the-git-index-safely-16-mib-limit-b7c078",
            )
            .into());
        }
        let stamp = String::from_utf8(out.stdout)
            .map_err(|_| crate::i18n::text("backend-invalid-index-fingerprint-ac6c56"))?;
        if stamp != "missing"
            && !(stamp.len() == 32 && stamp.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(crate::i18n::text("backend-invalid-index-fingerprint-ac6c56").into());
        }
        return Ok(stamp);
    }
    let index = lens::index_path(repo, active)?;
    Ok(read_index(&index)?
        .map(|b| format!("{:x}", md5::compute(b)))
        .unwrap_or_else(|| "missing".into()))
}
fn remote_stamp_read() -> String {
    // Stock macOS has Perl/Fcntl and md5; Linux/WSL commonly supply Perl and md5sum.
    // sysopen holds a no-follow regular descriptor across the entire bounded read.
    let read = format!(
        r#"use Fcntl qw(O_RDONLY O_NOFOLLOW O_NONBLOCK); use Errno qw(ENOENT);
my $f; sysopen($f,$ARGV[0],O_RDONLY|O_NOFOLLOW|O_NONBLOCK) or exit($! == ENOENT ? 2 : 1);
-f $f or exit 1; binmode $f; binmode STDOUT;
my $total=0; while ($total < {limit}) {{ my $n=sysread($f,my $b,{limit}-$total); defined($n) or exit 1; last if $n==0; print STDOUT $b or exit 1; $total+=$n; }}
exit($total > {max} ? 1 : 0);"#,
        limit = INDEX_LIMIT + 1,
        max = INDEX_LIMIT
    );
    format!(
        "{}\ncommand -v perl >/dev/null 2>&1 || {{ echo 'Safe index reads require Perl with Fcntl no-follow support on the repository machine' >&2; exit 1; }}\nperl -e {} -- \"$idx\" >\"$d/index-bytes\"; rc=$?\ncase \"$rc\" in 0) actual=$(gitcraft_md5 <\"$d/index-bytes\") || exit 1;; 2) actual=missing;; *) exit 1;; esac\n",
        portable_md5(),
        crate::remote::sh_quote(&read)
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
mod correction_tests;

#[cfg(all(test, unix))]
mod owned_remote_correction_tests;
