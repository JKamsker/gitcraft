//! Discover coverage artifacts and assemble coverage for repository changes.
use super::*;

/// When the file at `path` was last modified, if that can be told here.
fn modified(path: &Path) -> Option<i64> {
    if crate::remote::is_remote(path) {
        return None;
    }
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

/// Where a report named `name` (relative to the repository, or absolute on the machine the
/// repository is on) is.
fn report_path(repo: &Path, name: &str) -> PathBuf {
    if crate::remote::is_remote(repo) {
        crate::remote::resolve(repo, name)
    } else if Path::new(name).is_absolute() {
        PathBuf::from(name)
    } else {
        repo.join(name)
    }
}

/// An untracked file to count the lines of: a file here (not a link) no bigger than this.
const MAX_UNTRACKED_BYTES: u64 = 4 << 20;

/// Whether the untracked file at `path` is read: on another machine it is, here only a file
/// that isn't a link and isn't too big.
fn readable_untracked(repo: &Path, path: &Path) -> bool {
    if crate::remote::is_remote(repo) {
        return true;
    }
    std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_file() && m.len() <= MAX_UNTRACKED_BYTES)
}

/// The reports in the conventional places, in order.
pub fn find_reports(repo: &Path) -> Vec<String> {
    let mut found: Vec<String> = CONVENTIONAL
        .iter()
        .filter(|name| crate::remote::try_is_file(&report_path(repo, name)).unwrap_or(false))
        .map(|name| (*name).to_owned())
        .collect();
    // `*.lcov` is looked for only here: listing a folder elsewhere is a round trip per folder.
    if !crate::remote::is_remote(repo) {
        for dir in ["", "coverage"] {
            let Ok(entries) = std::fs::read_dir(repo.join(dir)) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".lcov"))
                .map(|n| {
                    if dir.is_empty() {
                        n
                    } else {
                        format!("{dir}/{n}")
                    }
                })
                .collect();
            names.sort();
            found.extend(names);
        }
    }
    found
}

/// What a report's sidecar records: the commit id it starts with, if it is one.
pub fn read_sidecar(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?;
    (first.len() >= 7 && first.len() <= 64 && first.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| first.to_ascii_lowercase())
}

/// The changed lines of `ask` and what the report `chosen` (or the first found in the
/// conventional places) says of them.
pub fn analyze(repo: &Path, ask: &Ask, chosen: Option<&str>) -> Result<Coverage, String> {
    let mut partial = Vec::new();
    let (sha, base, label, mut files) = match ask {
        Ask::WorkTree => {
            let head = git::resolve(repo, "HEAD")?;
            let mut files = git::changed_lines::read(repo, &head, None, &[])?;
            let untracked = git::run(
                Some(repo),
                &["ls-files", "-z", "--others", "--exclude-standard"],
            )?;
            let untracked: Vec<&str> = untracked.split('\0').filter(|p| !p.is_empty()).collect();
            if untracked.len() > MAX_UNTRACKED {
                partial.push(format!(
                    "only {MAX_UNTRACKED} of {} untracked files are counted",
                    untracked.len()
                ));
            }
            for path in untracked.into_iter().take(MAX_UNTRACKED) {
                let at = crate::remote::join(repo, path);
                if !readable_untracked(repo, &at) {
                    continue;
                }
                let Ok(bytes) = crate::remote::read(&at) else {
                    continue;
                };
                if bytes.contains(&0) {
                    continue;
                }
                let lines = bytes.split(|&b| b == b'\n').count() as u32
                    - u32::from(bytes.ends_with(b"\n") || bytes.is_empty());
                files.push(git::changed_lines::FileHunks {
                    path: path.to_owned(),
                    added: true,
                    hunks: vec![Hunk {
                        old_start: 0,
                        old_count: 0,
                        new_start: 1,
                        new_count: lines,
                    }]
                    .into_iter()
                    .filter(|h| h.new_count > 0)
                    .collect(),
                    ..Default::default()
                });
            }
            let short = &head[..head.len().min(8)];
            (
                head.clone(),
                head.clone(),
                format!("the working tree against HEAD ({short})"),
                files,
            )
        }
        Ask::Commit(rev) => {
            let head = git::resolve(repo, rev)?;
            let base = match git::resolve(repo, &format!("{head}^1")) {
                Ok(parent) => parent,
                Err(_) => {
                    git::run_input(Some(repo), &["hash-object", "-t", "tree", "--stdin"], "")?
                        .trim()
                        .to_owned()
                }
            };
            let files = git::changed_lines::read(repo, &base, Some(&head), &[])?;
            let short = &head[..head.len().min(8)];
            (
                head.clone(),
                base,
                format!("commit {short} against its parent"),
                files,
            )
        }
    };
    let found = find_reports(repo);
    let name = chosen.map(str::to_owned).or_else(|| found.first().cloned());
    // The reports and their `.sha` files aren't the change being looked at.
    let reports: HashSet<String> = found
        .iter()
        .chain(name.iter())
        .flat_map(|r| [r.clone(), format!("{r}{SIDECAR}")])
        .collect();
    files.retain(|f| !f.binary && !reports.contains(&f.path));
    if files.len() > MAX_FILES {
        partial.push(format!(
            "only {MAX_FILES} of {} changed files are mapped",
            files.len()
        ));
        files.truncate(MAX_FILES);
    }

    let searched = if name.is_none() {
        let mut s: Vec<String> = CONVENTIONAL.iter().map(|n| (*n).to_owned()).collect();
        s.push("*.lcov".to_owned());
        s
    } else {
        Vec::new()
    };
    let others = found
        .iter()
        .filter(|f| Some(*f) != name.as_ref())
        .cloned()
        .collect();
    let root = git::run(Some(repo), &["rev-parse", "--show-toplevel"])?
        .trim()
        .to_owned();
    let committed: i64 = git::run(Some(repo), &["log", "-1", "--format=%ct", &sha])?
        .trim()
        .parse()
        .unwrap_or(0);

    let mut report = None;
    let mut found_report = None;
    if let Some(name) = &name {
        let path = report_path(repo, name);
        let too_big = |len: u64| {
            format!(
                "{name} is {} MB, more than the {} MB read",
                len >> 20,
                MAX_REPORT >> 20
            )
        };
        if !crate::remote::is_remote(repo)
            && let Ok(meta) = std::fs::metadata(&path)
            && meta.len() > MAX_REPORT
        {
            return Err(too_big(meta.len()));
        }
        let bytes = crate::remote::read(&path).map_err(|e| format!("{name}: {e}"))?;
        if bytes.len() as u64 > MAX_REPORT {
            return Err(too_big(bytes.len() as u64));
        }
        let parsed =
            parse(name, &String::from_utf8_lossy(&bytes)).map_err(|e| format!("{name}: {e}"))?;
        let sidecar = path.with_file_name(format!(
            "{}{SIDECAR}",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let recorded = crate::remote::read_to_string(&sidecar)
            .ok()
            .and_then(|t| read_sidecar(&t));
        let written = modified(&path);
        let edited_after = if *ask == Ask::WorkTree {
            written.and_then(|w| {
                let times: Option<Vec<i64>> = files
                    .iter()
                    .filter(|f| !f.deleted)
                    .map(|f| modified(&repo.join(&f.path)))
                    .collect();
                times.map(|t| t.into_iter().any(|m| m > w))
            })
        } else {
            None
        };
        let freshness = judge(
            recorded.as_deref(),
            ask,
            &sha,
            written,
            committed,
            edited_after,
        );
        found_report = Some(Found {
            path: name.clone(),
            format: parsed.format,
            tests: parsed.tests.clone(),
            per_test: parsed.per_test(),
            files: parsed.files.len(),
            recorded,
            freshness,
            written,
        });
        report = Some(parsed);
    }
    let evidence = found_report
        .as_ref()
        .map_or(Evidence::Unknown, |f| f.freshness.evidence);
    let files = files
        .iter()
        .map(|f| {
            let mut mapped = map_file(&f.path, &f.hunks, report.as_ref(), &root, evidence);
            mapped.added = f.added;
            mapped.deleted = f.deleted;
            mapped
        })
        .collect();

    let tree = git::run(Some(repo), &["rev-parse", &format!("{sha}^{{tree}}")])?
        .trim()
        .to_owned();
    let lane = lane_verdicts(repo, &tree)?;
    Ok(Coverage {
        ask: ask.clone(),
        label,
        sha,
        base,
        report: found_report,
        searched,
        others,
        files,
        lane,
        tree,
        partial,
        collected: git::now(),
    })
}

/// What the test lane's tests said of `tree`.
fn lane_verdicts(repo: &Path, tree: &str) -> Result<Vec<LaneVerdict>, String> {
    let refs = git::run(
        Some(repo),
        &["for-each-ref", "--format=%(refname)", "refs/notes/tests/"],
    )?;
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for name in refs
        .lines()
        .filter_map(|r| r.strip_prefix("refs/notes/tests/"))
        .take(MAX_LANE_TESTS)
    {
        if !seen.insert(name.to_owned()) {
            continue;
        }
        if let Some(&verdict) = git::read_results(repo, name)?.get(tree) {
            out.push(LaneVerdict {
                name: name.to_owned(),
                verdict,
            });
        }
    }
    Ok(out)
}
