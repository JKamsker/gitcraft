//! Git LFS storage: how much disk the local LFS objects take, which files of the checkout are
//! only pointers (their objects never downloaded), what to fetch (`lfs.fetchinclude` /
//! `lfs.fetchexclude`), pruning old objects, and tracking a file type in LFS.

use std::path::Path;

use serde::Deserialize;

use super::run;

/// A file of the checkout kept in LFS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfsFile {
    /// From the repository's root, with `/` between folders.
    pub path: String,
    pub size: u64,
    /// Its object is in the local LFS storage.
    pub downloaded: bool,
    /// The file on disk is its object, not the pointer (or it was changed: git-lfs can't say).
    pub checked_out: bool,
}

impl LfsFile {
    /// Only its pointer is on disk: the object was never downloaded, or it was (`git lfs
    /// fetch`) but never checked out, unless the file has changes (`changed`), which also makes
    /// git-lfs say "not checked out".
    pub fn only_pointer(&self, changed: bool) -> bool {
        !self.downloaded || (!self.checked_out && !changed)
    }
}

/// `a\\b` as `a/b` where git-lfs may print Windows paths; elsewhere a backslash is part of a name.
fn slashes(path: &str) -> String {
    if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    }
}

#[derive(Deserialize)]
struct RawFiles {
    /// `null` when nothing is in LFS yet.
    #[serde(default)]
    files: Option<Vec<RawFile>>,
}

#[derive(Deserialize)]
struct RawFile {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    downloaded: Option<bool>,
    #[serde(default)]
    checkout: Option<bool>,
}

/// The files of the checkout (HEAD) kept in LFS: `git lfs ls-files --json`, or with a git-lfs
/// too old for it (before 3.3), its text listing.
pub fn lfs_files(repo: &Path) -> Result<Vec<LfsFile>, String> {
    match run(Some(repo), &["lfs", "ls-files", "--json"]) {
        Ok(json) => parse_files_json(&json),
        Err(e) if e.contains("unknown flag") => {
            let text = run(Some(repo), &["lfs", "ls-files", "--long", "--size"])?;
            Ok(parse_files_text(&text))
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn parse_files_json(json: &str) -> Result<Vec<LfsFile>, String> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let raw: RawFiles = serde_json::from_str(json.trim())
        .map_err(|e| format!("git-lfs listed its files in a way gitcraft can't read: {e}"))?;
    Ok(raw
        .files
        .unwrap_or_default()
        .into_iter()
        .map(|f| {
            let checked_out = f.checkout.unwrap_or(false);
            LfsFile {
                path: slashes(&f.name),
                size: f.size,
                // Checked out implies downloaded; with neither said, it isn't known to be there.
                downloaded: f.downloaded.unwrap_or(checked_out),
                checked_out,
            }
        })
        .collect())
}

/// `<oid> <*|-> <path> (<size>)`: `*` when the file is checked out from its object, `-` when it
/// is only its pointer.
pub(crate) fn parse_files_text(text: &str) -> Vec<LfsFile> {
    text.lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(' ')?;
            let (mark, rest) = rest.split_once(' ')?;
            let (path, size) = match rest.rsplit_once(" (") {
                Some((path, size)) => (path, size.strip_suffix(')').map(parse_human_size)),
                None => (rest, None),
            };
            // `-` can't tell "never downloaded" from "downloaded, not checked out".
            Some(LfsFile {
                path: slashes(path),
                size: size.flatten().unwrap_or(0),
                downloaded: mark == "*",
                checked_out: mark == "*",
            })
        })
        .collect()
}

/// "5.0 KB", "300 B", "1.2 GB" (as git-lfs prints sizes, in powers of 1000 or 1024 alike) as
/// bytes, near enough.
pub(crate) fn parse_human_size(s: &str) -> Option<u64> {
    let (n, unit) = s.trim().split_once(' ')?;
    let n: f64 = n.parse().ok()?;
    let factor = match unit {
        "B" => 1.0,
        "KB" | "kB" | "KiB" => 1024.0,
        "MB" | "MiB" => 1024.0 * 1024.0,
        "GB" | "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TB" | "TiB" => 1024.0_f64.powi(4),
        _ => return None,
    };
    (n >= 0.0).then(|| (n * factor).round() as u64)
}

/// How much disk the local LFS objects take.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LfsUsage {
    /// Where they are, as a path on the repository's machine.
    pub dir: String,
    pub bytes: u64,
    pub objects: u64,
}

/// The local LFS objects: where they are (`git lfs env`'s `LocalMediaDir`, which follows
/// `lfs.storage`) and their size, counted where the repository is.
pub fn lfs_usage(repo: &Path) -> Result<LfsUsage, String> {
    let env = run(Some(repo), &["lfs", "env"])?;
    let dir = media_dir(&env).ok_or("git lfs env doesn't say where the LFS objects are")?;
    let (bytes, objects) = match crate::remote::parse(repo) {
        None => count_here(Path::new(&dir)),
        Some(target) => count_there(&target, &dir)?,
    };
    Ok(LfsUsage {
        dir,
        bytes,
        objects,
    })
}

pub(crate) fn media_dir(env: &str) -> Option<String> {
    env.lines()
        .find_map(|l| l.trim().strip_prefix("LocalMediaDir="))
        .map(str::to_owned)
        .filter(|d| !d.is_empty())
}

/// Bytes and files under `dir`, here; nothing when it isn't there yet.
fn count_here(dir: &Path) -> (u64, u64) {
    let (mut bytes, mut files) = (0, 0);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                files += 1;
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    (bytes, files)
}

/// [`count_here`] on the repository's machine (WSL, SSH), with `du` and `find` there.
fn count_there(target: &crate::remote::Target, dir: &str) -> Result<(u64, u64), String> {
    let d = crate::remote::sh_quote(dir);
    let script = format!(
        "[ -d {d} ] || {{ echo '0 0'; exit 0; }}\n\
         k=$(du -sk {d} 2>/dev/null | cut -f1)\n\
         n=$(find {d} -type f 2>/dev/null | wc -l)\n\
         echo \"${{k:-0}} $n\""
    );
    let req = crate::remote::Request {
        script,
        ..Default::default()
    };
    let out = crate::remote::exec(&target.host, &req)?;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_du(&text).ok_or_else(|| {
        let e = String::from_utf8_lossy(&out.stderr);
        format!(
            "Could not measure the LFS objects on {}: {}",
            target.host.label(),
            e.trim()
        )
    })
}

/// `<kilobytes> <files>` as (bytes, files).
pub(crate) fn parse_du(text: &str) -> Option<(u64, u64)> {
    let mut words = text.split_whitespace();
    let kb: u64 = words.next()?.parse().ok()?;
    let files: u64 = words.next()?.parse().ok()?;
    Some((kb * 1024, files))
}

/// What `git lfs prune` would do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrunePlan {
    /// Objects in the local storage.
    pub local: u64,
    /// Kept: what recent commits, the checkout and unpushed work need.
    pub retained: u64,
    /// Would go, and the space they free.
    pub objects: u64,
    pub bytes: u64,
}

/// What pruning would remove, asking nothing to be removed (`--dry-run`).
pub fn lfs_prune_plan(repo: &Path) -> Result<PrunePlan, String> {
    let out = run(Some(repo), &["lfs", "prune", "--dry-run"])?;
    parse_prune(&out).ok_or_else(|| {
        format!(
            "git lfs prune said something gitcraft can't read: {}",
            out.trim()
        )
    })
}

/// `prune: 3 local objects, 2 retained, done.`, then `prune: 1 file would be pruned (300 B),
/// done.` when anything would go. (`--verbose` lists the objects only on a terminal.)
pub(crate) fn parse_prune(out: &str) -> Option<PrunePlan> {
    let mut plan = PrunePlan::default();
    let mut summary = false;
    // Some builds (Windows) print the lines without the `prune: ` prefix, and progress
    // updates in place with carriage returns.
    let lines = out.split(['\n', '\r']).map(str::trim);
    for line in lines.map(|l| l.strip_prefix("prune: ").unwrap_or(l)) {
        if line.contains("local object") {
            summary = true;
            for part in line.split(',') {
                let part = part.trim();
                let n = part.split_whitespace().next().and_then(|n| n.parse().ok());
                if part.contains("local object") {
                    plan.local = n?;
                } else if part.contains("retained") {
                    plan.retained = n.unwrap_or(0);
                }
            }
        } else if line.contains("would be pruned") {
            plan.objects = line.split_whitespace().next()?.parse().ok()?;
            plan.bytes = line
                .split_once(" (")
                .and_then(|(_, s)| s.split(')').next())
                .and_then(parse_human_size)
                .unwrap_or(0);
        }
    }
    if summary && plan.objects == 0 {
        plan.objects = plan.local.saturating_sub(plan.retained);
    }
    summary.then_some(plan)
}

/// The folders `git lfs fetch` / `pull` limit themselves to, or leave out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchFilters {
    /// As this repository's own config sets them, which gitcraft edits.
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// As the repository's committed `.lfsconfig` sets them, which its own config overrides.
    pub shared_include: Vec<String>,
    pub shared_exclude: Vec<String>,
}

/// The fetch filters of the repository (`lfs.fetchinclude`, `lfs.fetchexclude`), its own and
/// its `.lfsconfig`'s.
pub fn fetch_filters(repo: &Path) -> Result<FetchFilters, String> {
    let get = |file: Option<&str>, key: &str| -> Result<Vec<String>, String> {
        let mut args = vec!["config"];
        match file {
            Some(f) => args.extend(["-f", f]),
            None => args.push("--local"),
        }
        args.extend(["--get", key]);
        // Exit 1: not set (or no such file).
        super::run_ok_codes(Some(repo), &args, &[0, 1]).map(|v| split_list(&v))
    };
    let lfsconfig = crate::remote::git_path(&crate::remote::join(repo, ".lfsconfig"));
    let shared = crate::remote::is_file(&crate::remote::join(repo, ".lfsconfig"));
    let shared_get = |key| {
        if shared {
            get(Some(&lfsconfig), key)
        } else {
            Ok(Vec::new())
        }
    };
    Ok(FetchFilters {
        include: get(None, "lfs.fetchinclude")?,
        exclude: get(None, "lfs.fetchexclude")?,
        shared_include: shared_get("lfs.fetchinclude")?,
        shared_exclude: shared_get("lfs.fetchexclude")?,
    })
}

/// `a,b, c` as its items.
pub(crate) fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A folder of the repository as a fetch filter pattern, from the root: `/art/chars` (no
/// `./`, no trailing `/`, glob characters escaped, so a nested `x/art/chars` isn't matched);
/// `None` for the root, or with a `,` (which would split it) or spaces at its ends (which
/// git-lfs trims).
pub fn folder_pattern(folder: &str) -> Option<String> {
    let p = folder.replace('\\', "/");
    let p = p.trim_start_matches("./").trim_matches('/');
    if p.is_empty() || p.trim().is_empty() || p.contains(',') || p.trim() != p {
        return None;
    }
    Some(format!("/{}", escape_glob(p)))
}

/// A path as a pattern matching it alone: `[ ] * ? \\` escaped.
pub fn escape_glob(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A fetch filter pattern as people read it: `/art/x\\[1\\]` as `art/x[1]`.
pub fn pattern_label(pattern: &str) -> String {
    let p = pattern.strip_prefix('/').unwrap_or(pattern);
    let mut out = String::new();
    let mut escaped = false;
    for c in p.chars() {
        if c == '\\' && !escaped {
            escaped = true;
            continue;
        }
        escaped = false;
        out.push(c);
    }
    out
}

/// Sets the repository's own `lfs.fetchinclude` / `lfs.fetchexclude` (`exclude`) to `list`,
/// or takes it out when the list is empty.
pub fn set_fetch_filter(repo: &Path, exclude: bool, list: &[String]) -> Result<(), String> {
    let key = if exclude {
        "lfs.fetchexclude"
    } else {
        "lfs.fetchinclude"
    };
    if list.is_empty() {
        // Exit 5: it wasn't set.
        super::run_ok_codes(
            Some(repo),
            &["config", "--local", "--unset-all", key],
            &[0, 5],
        )?;
        return Ok(());
    }
    if let Some(bad) = list.iter().find(|p| p.contains(',')) {
        return Err(format!(
            "{bad} has a comma, which git-lfs would read as two patterns"
        ));
    }
    run(Some(repo), &["config", "--local", key, &list.join(",")]).map(|_| ())
}

/// The pattern for files like `path` in `.gitattributes`: `*.psd` for its extension, or the
/// file itself (glob characters escaped, `[[:space:]]` for spaces, which separate attributes)
/// when it has none. None for dotfiles (`.gitattributes` itself in LFS would break LFS).
pub fn track_pattern(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.is_empty() || name.starts_with('.') {
        return None;
    }
    match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty() && !ext.is_empty() && ext.chars().all(|c| c.is_alphanumeric()) =>
        {
            Some(format!("*.{ext}"))
        }
        _ => Some(escape_glob(path).replace(' ', "[[:space:]]")),
    }
}

/// The patterns the root `.gitattributes` sends through LFS (`filter=lfs`).
pub fn lfs_patterns(repo: &Path) -> Vec<String> {
    crate::remote::read_to_string(&crate::remote::join(repo, ".gitattributes"))
        .map(|text| lfs_patterns_in(&text))
        .unwrap_or_default()
}

pub(crate) fn lfs_patterns_in(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let pattern = words.next()?;
            words.any(|a| a == "filter=lfs").then(|| pattern.to_owned())
        })
        .collect()
}

#[cfg(test)]
#[path = "lfs_storage_tests.rs"]
mod tests;
