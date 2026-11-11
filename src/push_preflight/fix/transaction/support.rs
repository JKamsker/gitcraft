//! Bounded local command adapters and immutable index recovery snapshots.
use super::*;

pub(in super::super) fn supported_version(text: &[u8]) -> bool {
    let Some(version) = std::str::from_utf8(text)
        .ok()
        .and_then(|text| text.strip_prefix("git version "))
        .and_then(|text| text.split_whitespace().next())
    else {
        return false;
    };
    let mut parts = version.split('.');
    let parsed = (|| {
        Some((
            parts.next()?.parse::<u64>().ok()?,
            parts.next()?.parse::<u64>().ok()?,
            parts.next()?.parse::<u64>().ok()?,
        ))
    })();
    parsed.is_some_and(|version| version >= (2, 50, 0))
}

pub(in super::super) fn path_line(bytes: Vec<u8>) -> Result<String, String> {
    let text = String::from_utf8(bytes).map_err(|_| "Safe repair requires UTF-8 metadata paths")?;
    let text = text.strip_suffix('\n').unwrap_or(&text);
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err("Safe repair requires metadata paths without control characters".into());
    }
    Ok(text.to_owned())
}

pub(super) fn read(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    git::forecast_bytes(repo, args, &[], &[0], &|| true)
}

pub(super) fn success(output: std::process::Output) -> Result<Vec<u8>, String> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err("The guarded Git repair command was refused; tool diagnostics were withheld.".into())
    }
}

pub(super) fn local(
    repo: &Path,
    args: &[&str],
    input: &[u8],
    env: &[(&'static str, String)],
) -> Result<Vec<u8>, String> {
    success(git::local_repair_input(repo, args, input, env)?)
}

pub(super) fn owned(
    context_repo: &Path,
    directory: &Path,
    args: &[&str],
    input: &[u8],
    env: &[(&'static str, String)],
) -> Result<Vec<u8>, String> {
    // Keep the originating repository's Practice-first/profile context even while Git
    // works in an application-owned scratch directory. The second -C is absolute.
    let directory = remote::git_path(directory);
    let words: Vec<_> = ["-C", directory.as_str()]
        .into_iter()
        .chain(args.iter().copied())
        .collect();
    local(context_repo, &words, input, env)
}

pub(super) fn transaction(old: &str, new: &str) -> String {
    // Updating symbolic HEAD lets Git acquire HEAD and its referent together. An
    // explicit symref-verify plus referent update collides with Git's implicit HEAD
    // reflog update. The private hook separately requires the exact intended branch
    // in native stdin and symbolic HEAD before/after the original prepared hook.
    format!("start\nupdate HEAD {new} {old}\nprepare\ncommit\n")
}

pub(super) fn snapshot(source: &str, destination: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut input =
        std::fs::File::open(source).map_err(|_| "Could not snapshot the original index")?;
    let mut bytes = Vec::new();
    (&mut input)
        .take(MAX_INDEX + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the original index")?;
    if bytes.len() as u64 > MAX_INDEX {
        return Err("Index recovery snapshot exceeds 64 MiB".into());
    }
    std::fs::write(destination, bytes)
        .map_err(|_| "Could not save the owned index recovery snapshot".into())
}

/// The resolver observes only the immutable owned main-index snapshot. If Git cannot
/// load that snapshot's dependency in this repository context, refuse before dispatch.
pub(super) fn snapshot_pair(
    source_index: &str,
    directory: &Path,
    resolve: impl FnOnce(&Path) -> Result<Option<String>, String>,
) -> Result<Option<String>, String> {
    let saved_index = directory.join("before.index");
    snapshot(source_index, &saved_index)?;
    let dependency = resolve(&saved_index)?;
    if let Some(path) = &dependency {
        if !Path::new(path).is_absolute() {
            return Err("Saved index dependency was not an absolute path; repair refused".into());
        }
        snapshot(path, &directory.join("before.shared-index"))?;
    }
    Ok(dependency)
}
