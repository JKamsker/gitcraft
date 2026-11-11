//! A durable copy plan binds each permitted write to its exact before/after bytes.
use super::*;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
enum Image {
    Missing,
    File(Vec<u8>),
    Link(PathBuf),
}
#[derive(Serialize, Deserialize)]
struct Entry {
    git: bool,
    path: String,
    before: Image,
    after: Image,
    #[serde(default)]
    before_attributes: Option<u32>,
    #[serde(default)]
    after_attributes: Option<u32>,
}
#[derive(Serialize, Deserialize)]
struct Plan {
    version: u32,
    source: Parked,
    entries: Vec<Entry>,
}

pub(super) fn identity(dir: &Path, parked: &Parked) -> Result<(), String> {
    let stored: Parked =
        serde_json::from_slice(&std::fs::read(dir.join("parked.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if &stored != parked {
        return Err("Recovery source identity changed; archive retained".into());
    }
    Ok(())
}

/// Required sources are regular files, never an implicit deletion image.
pub(super) fn validate_required(dir: &Path, parked: &Parked) -> Result<(), String> {
    identity(dir, parked)?;
    for path in parked
        .raw
        .iter()
        .map(|p| ("raw", p.as_str()))
        .chain(std::iter::once(("", "index")))
        .chain(parked.state_files.iter().map(|p| ("", p.as_str())))
    {
        let relative = if path.0.is_empty() {
            path.1.to_owned()
        } else {
            format!("{}/{}", path.0, path.1)
        };
        required(&target(dir, &relative)?)?;
    }
    Ok(())
}
fn required(path: &Path) -> Result<Image, String> {
    match image(path)? {
        file @ Image::File(_) => Ok(file),
        _ => Err(format!(
            "Required recovery source missing or invalid: {}",
            path.display()
        )),
    }
}

pub(super) fn guidance(dir: &Path) -> String {
    format!(
        "Interrupted recovery at {}. Save current worktree files, index and operation metadata elsewhere before any manual reset or removal of a stale index.lock (only after confirming Git has stopped). Retry Bring back if the original preconditions are restored, or Drop to branch to recover the snapshot; the byte-exact archive remains at this path. No automatic rollback overwrites uncertain work.",
        dir.display()
    )
}

fn image(path: &Path) -> Result<Image, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Image::Missing),
        Err(e) => Err(e.to_string()),
        Ok(m) if m.is_symlink() => std::fs::read_link(path)
            .map(Image::Link)
            .map_err(|e| e.to_string()),
        Ok(m) if m.is_file() => std::fs::read(path)
            .map(Image::File)
            .map_err(|e| e.to_string()),
        _ => Err(format!("Unsupported recovery target {}", path.display())),
    }
}
fn target(base: &Path, path: &str) -> Result<PathBuf, String> {
    crate::mcp::args::repo_path(path)?;
    let full = base.join(path);
    let mut parent = full.parent();
    while let Some(p) = parent {
        if p == base {
            break;
        }
        if std::fs::symlink_metadata(p).is_ok_and(|m| !m.is_dir() || m.is_symlink()) {
            return Err(format!("Recovery parent changed: {}", p.display()));
        }
        parent = p.parent();
    }
    Ok(full)
}

pub(super) fn prepare(repo: &Path, gd: &Path, dir: &Path, parked: &Parked) -> Result<(), String> {
    validate_required(dir, parked)?;
    if dir.join("restore-plan.json").exists() {
        return Err(guidance(dir));
    }
    let mut entries = Vec::new();
    let tracked = run(repo, &["ls-files", "--cached", "-z"])?;
    let mut paths: Vec<String> = tracked
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    paths.extend(parked.raw.iter().cloned());
    paths.sort();
    paths.dedup();
    for path in paths {
        let before = image(&target(repo, &path)?)?;
        let after = if parked.raw.contains(&path) {
            required(&target(dir, &format!("raw/{path}"))?)?
        } else {
            image(&target(repo, &path)?)?
        };
        entries.push(Entry {
            git: false,
            before_attributes: durability::attributes(&target(repo, &path)?).ok(),
            after_attributes: if parked.raw.contains(&path) {
                Some(durability::attributes(&target(
                    dir,
                    &format!("raw/{path}"),
                )?)?)
            } else {
                durability::attributes(&target(repo, &path)?).ok()
            },
            before,
            after,
            path,
        });
    }
    for name in std::iter::once("index").chain(STATE_FILES) {
        let before = image(&target(gd, name)?)?;
        let after = if name == "index" || parked.state_files.iter().any(|p| p == name) {
            required(&target(dir, name)?)?
        } else {
            Image::Missing
        };
        entries.push(Entry {
            git: true,
            path: name.into(),
            before_attributes: durability::attributes(&target(gd, name)?).ok(),
            after_attributes: if matches!(after, Image::File(_)) {
                Some(durability::attributes(&target(dir, name)?)?)
            } else {
                None
            },
            before,
            after,
        });
    }
    let bytes = serde_json::to_vec(&Plan {
        version: 1,
        source: parked.clone(),
        entries,
    })
    .map_err(|e| e.to_string())?;
    journal::atomic_write(&dir.join("restore-plan.json"), &bytes)?;
    effects::boundary("restore-plan-durable")
}

pub(super) fn resume(repo: &Path, gd: &Path, dir: &Path, parked: &Parked) -> Result<(), String> {
    identity(dir, parked)?;
    let plan: Plan = serde_json::from_slice(
        &std::fs::read(dir.join("restore-plan.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if plan.version != 1 || plan.source != *parked {
        return Err(guidance(dir));
    }
    if sparse(repo)
        || unkeepable(repo).is_some()
        || !super::restore::original_checkout(repo, parked)?
        || gd.join("index.lock").exists()
        || ["rebase-merge", "rebase-apply", "sequencer", "BISECT_LOG"]
            .iter()
            .any(|p| gd.join(p).exists())
    {
        return Err(guidance(dir));
    }
    // Validate the complete captured set before any write, including unchanged tracked
    // files. Newly edited bytes cannot be mistaken for a partially applied copy.
    for entry in &plan.entries {
        let base = if entry.git { gd } else { repo };
        let actual = image(&target(base, &entry.path)?)?;
        let attrs = durability::attributes(&target(base, &entry.path)?).ok();
        let before_matches =
            actual == entry.before && entry.before_attributes.is_none_or(|a| Some(a) == attrs);
        let after_matches =
            actual == entry.after && entry.after_attributes.is_none_or(|a| Some(a) == attrs);
        if !before_matches && !after_matches {
            return Err(guidance(dir));
        }
        if entry.git && (entry.path == "index" || parked.state_files.contains(&entry.path))
            || !entry.git && parked.raw.contains(&entry.path)
        {
            let source = if entry.git {
                entry.path.clone()
            } else {
                format!("raw/{}", entry.path)
            };
            if image(&target(dir, &source)?)? != entry.after
                || entry
                    .after_attributes
                    .is_some_and(|a| durability::attributes(&dir.join(&source)).ok() != Some(a))
            {
                return Err("Recovery source bytes changed".into());
            }
        }
    }
    for entry in &plan.entries {
        let path = target(if entry.git { gd } else { repo }, &entry.path)?;
        if image(&path)? == entry.after
            && entry
                .after_attributes
                .is_none_or(|a| durability::attributes(&path).ok() == Some(a))
        {
            continue;
        }
        if image(&path)? != entry.before {
            return Err(guidance(dir));
        }
        let Image::File(bytes) = &entry.after else {
            return Err(guidance(dir));
        };
        // Acquire Git's index lock rather than adopting/deleting an existing lock.
        if entry.git && entry.path == "index" {
            let lock = gd.join("index.lock");
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&lock)
                .map_err(|e| e.to_string())?;
            file.write_all(bytes)
                .and_then(|()| {
                    file.set_permissions(std::fs::metadata(dir.join("index"))?.permissions())
                })
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
            effects::boundary("index-lock-synced")?;
            drop(file);
            if image(&target(gd, &entry.path)?)? != entry.before {
                return Err(guidance(dir));
            }
            std::fs::rename(lock, &path).map_err(|e| e.to_string())?;
            journal::sync_dir(gd)?;
        } else {
            let source = if entry.git {
                dir.join(&entry.path)
            } else {
                dir.join("raw").join(&entry.path)
            };
            let key = format!("live/{}/{}", entry.git, entry.path);
            super::publication::publish(&path, bytes, &source, (dir, &key), || {
                if image(&target(if entry.git { gd } else { repo }, &entry.path)?)? != entry.before
                {
                    return Err(guidance(dir));
                }
                Ok(())
            })?;
        }
        effects::boundary("restore-copy-applied")?;
    }
    Ok(())
}
