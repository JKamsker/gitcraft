//! Derive review signals for files and hunks.
use super::*;

/// The signals of each file and hunk of `files`, from what the version seen says (`seen`) and
/// the threads (`None`: not read).
pub fn derive(files: Vec<FileDiff>, seen: &Seen, threads: Option<&[ThreadAt]>) -> Vec<FileHeat> {
    files
        .into_iter()
        .map(|mut file| {
            let more_hunks = file.hunks.len() > MAX_HUNKS;
            file.hunks.truncate(MAX_HUNKS);
            let on_file: Vec<usize> = threads
                .unwrap_or(&[])
                .iter()
                .enumerate()
                .filter(|(_, t)| t.path == file.path)
                .map(|(i, _)| i)
                .collect();
            let (seen_file, since_file) = file_signal(&file, seen);
            let hunks = file
                .hunks
                .iter()
                .map(|h| {
                    let (seen, since) = hunk_signal(&file.path, h, seen, seen_file);
                    let threads = on_file
                        .iter()
                        .copied()
                        .filter(|&i| threads.is_some_and(|all| all[i].in_hunk(h)))
                        .collect();
                    HunkHeat {
                        hunk: *h,
                        seen,
                        since,
                        threads,
                    }
                })
                .collect();
            FileHeat {
                file,
                seen: seen_file,
                since: since_file,
                threads: on_file,
                hunks,
                more_hunks,
            }
        })
        .collect()
}

fn file_signal(file: &FileDiff, seen: &Seen) -> (SeenMark, Option<u32>) {
    match seen {
        Seen::Unknown(_) => (SeenMark::Unknown, None),
        Seen::Never => (SeenMark::NotSeen, None),
        Seen::Fresh { since, .. } => {
            let lines: u32 = since
                .get(&file.path)
                .map_or(0, |hunks| hunks.iter().map(Hunk::size).sum());
            let mark = if since.contains_key(&file.path) {
                SeenMark::Changed
            } else {
                SeenMark::Same
            };
            (mark, Some(lines))
        }
        Seen::Stale { differing, .. } => {
            if differing.contains(&file.path) {
                (SeenMark::Stale, None)
            } else {
                (SeenMark::SameAfterRewrite, None)
            }
        }
    }
}

fn hunk_signal(path: &str, hunk: &Hunk, seen: &Seen, file: SeenMark) -> (SeenMark, Option<u32>) {
    match seen {
        Seen::Fresh { since, .. } => {
            let touching: u32 = since
                .get(path)
                .map(|list| {
                    list.iter()
                        .filter(|s| overlaps(s.new_range(), hunk.new_range()))
                        .map(Hunk::size)
                        .sum()
                })
                .unwrap_or(0);
            let touched = since.get(path).is_some_and(|list| {
                list.iter()
                    .any(|s| overlaps(s.new_range(), hunk.new_range()))
            });
            let mark = if touched {
                SeenMark::Changed
            } else {
                SeenMark::Same
            };
            (mark, Some(touching))
        }
        _ => (file, None),
    }
}
