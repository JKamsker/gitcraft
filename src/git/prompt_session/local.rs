use super::*;
fn discover(
    root: &Path,
    dir: &Path,
    depth: usize,
    visited: &mut usize,
    files: &mut Vec<(u64, String)>,
) {
    if depth > 5 || *visited >= 4096 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten().take(4096 - *visited) {
        if *visited >= 4096 {
            break;
        }
        *visited += 1;
        let Ok(m) = entry.file_type() else {
            continue;
        };
        if m.is_symlink() {
            continue;
        }
        let path = entry.path();
        if m.is_dir() {
            discover(root, &path, depth + 1, visited, files);
        } else if m.is_file()
            && path.extension().is_some_and(|e| e == "jsonl")
            && let Some(rel) = path.strip_prefix(root).ok().and_then(|p| p.to_str())
        {
            let time = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |t| t.as_secs());
            files.push((time, rel.replace('\\', "/")));
        }
    }
}
pub(super) fn load(
    root: &Path,
    time: i64,
    options: &Options,
    active: &dyn Fn() -> bool,
    budget: &mut usize,
    out: &mut Evidence,
) {
    let start = time - i64::from(options.hours.clamp(1, 168)) * 3600;
    let end = time + 600;
    for configured in options.roots.iter().take(8) {
        if !active() {
            return;
        }
        let Ok(base) = std::fs::canonicalize(configured) else {
            out.notices.push(format!(
                "Local transcript root unavailable: {}",
                redact(&configured.display().to_string())
            ));
            continue;
        };
        if std::fs::symlink_metadata(configured).is_ok_and(|m| m.file_type().is_symlink()) {
            out.notices.push(
                "A linked transcript root was refused; choose its physical directory explicitly"
                    .into(),
            );
            continue;
        }
        let mut files = Vec::new();
        let mut visited = 0;
        discover(&base, &base, 0, &mut visited, &mut files);
        files.sort_by(|a, b| b.cmp(a));
        if visited >= 4096 || files.len() > 200 {
            out.notices.push("Local discovery is bounded to 4,096 entries, depth five and the 200 newest JSONL files per root; choose a narrower root for older sessions".into());
        }
        for (_, rel) in files.into_iter().take(200) {
            if !active() || *budget == 0 {
                return;
            }
            let Some(bytes) =
                crate::safe_read::read_inside(&base, &rel, FILE_LIMIT.min(*budget) as u64)
            else {
                out.notices.push(format!(
                    "Unreadable, linked or oversized local transcript omitted: {}",
                    redact(&rel)
                ));
                continue;
            };
            *budget -= bytes.len();
            let (steps, notices) = parse::transcript(&bytes, root, start, end, false);
            out.notices.extend(notices);
            if !steps.is_empty() {
                out.sessions.push(Session{source:format!("Local {}",redact(&base.join(&rel).display().to_string())),relation:format!("Worktree matched exactly; records within {} hours before commit through 10 minutes after. Time/path correlation, not an authenticated commit link",options.hours.clamp(1,168)),steps});
            }
            if out.sessions.len() >= 64 {
                out.notices
                    .push("Stopped at 64 candidate sessions; narrow the root/time window".into());
                return;
            }
        }
    }
}
