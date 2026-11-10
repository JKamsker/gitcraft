//! Bounded outgoing history and physical blob enumeration (replacement/lazy reads disabled).
use super::*;

pub(super) fn commits(
    repo: &Path,
    head: &str,
    base: &Option<String>,
    deleting: bool,
    active: &dyn Fn() -> bool,
) -> Result<Vec<String>, String> {
    let mut args = vec![
        "rev-list",
        "--reverse",
        "--topo-order",
        "--max-count=10001",
        head,
    ];
    if let Some(base) = base {
        args.extend(["--not", base]);
    }
    let ids: Vec<String> = if deleting {
        Vec::new()
    } else {
        read(repo, &args, active)?
            .lines()
            .map(str::to_owned)
            .collect()
    };
    if ids.len() > MAX_COMMITS {
        return Err("The outgoing history exceeds 10,000 commits; split the push or fetch its exact target before checking.".into());
    }
    Ok(ids)
}

pub(super) fn inspect(
    repo: &Path,
    head: &str,
    base: &Option<String>,
    commits: &[Commit],
    findings: &mut Vec<Finding>,
    active: &dyn Fn() -> bool,
) -> Result<BTreeSet<String>, String> {
    let mut objects_args = vec!["rev-list", "--objects", "--no-object-names", head];
    if let Some(b) = base {
        objects_args.extend(["--not", b]);
    }
    let objects: BTreeSet<String> = read(repo, &objects_args, active)?
        .lines()
        .map(str::to_owned)
        .collect();
    if objects.len() > MAX_OBJECTS {
        return Err("More than 50,000 outgoing objects need checking. Split the push or fetch the exact target.".into());
    }
    let mut seen = BTreeSet::new();
    let mut lfs_objects = BTreeSet::new();
    for commit in commits {
        let tree = read(
            repo,
            &["ls-tree", "-r", "-l", "-z", "--full-tree", &commit.oid],
            active,
        )?;
        for item in tree.split('\0').filter(|s| !s.is_empty()) {
            let Some((meta, path)) = item.split_once('\t') else {
                continue;
            };
            let words: Vec<_> = meta.split_whitespace().collect();
            if words.len() != 4
                || words[1] != "blob"
                || !objects.contains(words[2])
                || !seen.insert((words[2].to_owned(), path.to_owned()))
            {
                continue;
            }
            let size = words[3]
                .parse::<u64>()
                .map_err(|_| "Unknown outgoing blob size")?;
            if size <= 1024 {
                let text = read(repo, &["cat-file", "blob", words[2]], active)?;
                if let Some(pointer) = git::parse_lfs_pointer(&text) {
                    let oid = pointer
                        .oid
                        .strip_prefix("sha256:")
                        .ok_or("Unsupported LFS object hash")?;
                    if oid.len() != 64 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err("Malformed LFS object hash".into());
                    }
                    lfs_objects.insert(oid.to_owned());
                }
            }
            if size > LARGE {
                findings.push(Finding {
                    kind: Kind::Large,
                    commit: commit.oid.clone(),
                    path: path.into(),
                    detail: format!("{} Git blob; consider Git LFS.", git::human_size(size)),
                });
            }
            for line in blob_secret_lines(repo, words[2], active)? {
                findings.push(Finding {
                    kind: Kind::Secret,
                    commit: commit.oid.clone(),
                    path: path.into(),
                    detail: format!("Likely secret at line {line}; value withheld."),
                });
            }
            if findings.len() > MAX_FINDINGS {
                return Err("More than 2,000 findings need review. Repair or split this history before pushing.".into());
            }
        }
    }
    Ok(lfs_objects)
}
