//! Stable hunk identity includes its nearby base lines, so matching edits in changed
//! surroundings require a new placement. Line shifts alone preserve identity.
use super::units::{Body, Units};
use std::{collections::HashMap, path::Path};

pub(super) fn add_context(repo: &Path, base: &str, units: &mut Units) -> Result<(), String> {
    let mut files: HashMap<String, Vec<String>> = HashMap::new();
    for unit in &mut units.units {
        let Body::Hunk {
            old_start, old_len, ..
        } = unit.body
        else {
            continue;
        };
        let path = unit.old_path.as_ref().unwrap_or(&unit.path);
        if unit.status == 'A' {
            continue;
        }
        if !files.contains_key(path) {
            let spec = format!("{base}:{path}");
            let size = crate::git::run(Some(repo), &["cat-file", "-s", &spec])?;
            // For very large blobs, invalidate conservatively when the base moves.
            if size.trim().parse::<u64>().unwrap_or(u64::MAX) > 1024 * 1024 {
                unit.key.push_str(&format!(":base:{base}"));
                continue;
            }
            let text = crate::git::run(Some(repo), &["cat-file", "blob", &spec])?;
            files.insert(path.clone(), text.lines().map(str::to_owned).collect());
        }
        let lines = &files[path];
        let start = if old_len == 0 {
            old_start as usize
        } else {
            old_start.saturating_sub(1) as usize
        };
        let before = &lines[start.saturating_sub(3)..start.min(lines.len())];
        let end = (start + old_len as usize).min(lines.len());
        let after = &lines[end..(end + 3).min(lines.len())];
        unit.key.push_str(&format!(
            ":{:x}",
            md5::compute(format!("{before:?}{after:?}"))
        ));
    }
    Ok(())
}
