//! Stable hunk identity includes its nearby base lines, so matching edits in changed
//! surroundings require a new placement. Line shifts alone preserve identity.
use super::units::{Body, Units};
use std::{collections::HashMap, path::Path};

pub(super) fn add_context(repo: &Path, base: &str, units: &mut Units) -> Result<(), String> {
    add_context_with(base, units, |args| crate::git::run(Some(repo), args))
}

fn add_context_with(
    base: &str,
    units: &mut Units,
    mut run: impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let mut files: HashMap<String, Option<Vec<String>>> = HashMap::new();
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
            let size = run(&["cat-file", "-s", &spec])?;
            // For very large blobs, invalidate conservatively when the base moves.
            if size.trim().parse::<u64>().unwrap_or(u64::MAX) > 1024 * 1024 {
                files.insert(path.clone(), None);
            } else {
                let text = run(&["cat-file", "blob", &spec])?;
                files.insert(
                    path.clone(),
                    Some(text.lines().map(str::to_owned).collect()),
                );
            }
        }
        let Some(lines) = &files[path] else {
            unit.key.push_str(&format!(":base:{base}"));
            continue;
        };
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

#[cfg(test)]
mod tests {
    #[test]
    fn oversized_blobs_have_one_size_lookup_per_path() -> Result<(), String> {
        use super::super::tests::{unit, units};
        let mut units = units(
            vec![
                unit("large.txt", 1),
                unit("large.txt", 20),
                unit("other.txt", 1),
            ],
            "head",
        );
        let mut calls = Vec::new();
        super::add_context_with("base", &mut units, |args| {
            calls.push(args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
            Ok("1048577".into())
        })?;
        assert_eq!(
            calls,
            vec![
                vec!["cat-file", "-s", "base:large.txt"],
                vec!["cat-file", "-s", "base:other.txt"]
            ]
        );
        assert!(units.units.iter().all(|u| u.key.ends_with(":base:base")));
        Ok(())
    }
}
