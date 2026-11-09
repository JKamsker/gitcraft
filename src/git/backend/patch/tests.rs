use super::*;
use crate::git::fixture::Repo;

/// Every file of each of `hashes`, with each of `options`: the patch made in-process is the
/// one git prints, whenever one is made. Returns how many were made in-process.
fn same_as_git(repo: &Path, hashes: &[String], options: &[DiffOptions]) -> Result<usize, String> {
    let mut made = 0;
    let mut differing = Vec::new();
    let mut reasons = std::collections::BTreeMap::new();
    for hash in hashes {
        for file in crate::git::diff::commit_files(repo, hash)? {
            for &opts in options {
                let ours = match file_patch(repo, hash, &file.path, opts) {
                    Ok(ours) => ours,
                    Err(reason) => {
                        *reasons.entry(reason).or_insert(0usize) += 1;
                        continue;
                    }
                };
                made += 1;
                let theirs = crate::git::diff::file_diff_by_git(repo, hash, &file.path, opts)?;
                if ours != theirs {
                    differing.push(format!(
                        "{hash} {} {opts:?}\n--- ours\n{ours}\n--- git's\n{theirs}",
                        file.path
                    ));
                }
            }
        }
    }
    println!("handed to git: {reasons:?}");
    assert!(differing.is_empty(), "{}", differing.join("\n\n"));
    Ok(made)
}

fn lines(range: std::ops::Range<usize>, change: impl Fn(usize) -> Option<String>) -> String {
    range
        .map(|i| change(i).unwrap_or_else(|| format!("line {i}\n")))
        .collect()
}

#[test]
fn patches_made_in_process_are_those_git_prints() -> Result<(), String> {
    let repo = Repo::new("native-patches")?;
    let code = |body: &str| format!("fn first() {{\n{body}}}\n\nfn second() {{\n    two\n}}\n");
    let long = lines(0..120, |i| (i % 30 == 0).then(|| format!("section_{i}:\n")));
    let root = repo.commit(
        "root",
        &[
            ("code.rs", Some(&code("    one\n"))),
            ("long.txt", Some(&long)),
            ("bin.dat", Some("\0\u{1}binary\0")),
            ("no-newline.txt", Some("a\nb")),
            ("crlf.txt", Some("one\r\ntwo\r\n")),
            ("with space ü.txt", Some("x\n")),
            ("forced.txt", Some("text\n")),
            ("texty.dat", Some("\0text\n")),
            ("gone.txt", Some("going\n")),
            (".gitattributes", Some("forced.txt -diff\ntexty.dat diff\n")),
        ],
    )?;
    let edited = repo.commit(
        "edit",
        &[
            ("code.rs", Some(&code("    one\n    more\n"))),
            (
                "long.txt",
                Some(&lines(0..121, |i| match i {
                    5 => Some("changed 5\n".into()),
                    12 => Some("changed 12\n".into()),
                    70 => Some("inserted\n".into()),
                    i if i % 30 == 0 && i < 70 => Some(format!("section_{i}:\n")),
                    i if i > 70 && (i - 1) % 30 == 0 => Some(format!("section_{}:\n", i - 1)),
                    i if i > 70 => Some(format!("line {}\n", i - 1)),
                    _ => None,
                })),
            ),
            ("bin.dat", Some("\0\u{2}binary\0")),
            ("no-newline.txt", Some("a\nb\nc")),
            ("crlf.txt", Some("one\r\nTWO\r\n")),
            ("with space ü.txt", Some("y\n")),
            ("forced.txt", Some("changed\n")),
            ("texty.dat", Some("\0changed\n")),
        ],
    )?;
    let deleted = repo.commit("delete", &[("gone.txt", None)])?;
    let added = repo.commit(
        "add",
        &[("new.txt", Some("new\n")), ("empty.txt", Some(""))],
    )?;
    repo.git(&["mv", "new.txt", "moved.txt"])?;
    let moved = repo.commit("move", &[])?;
    repo.git(&["update-index", "--chmod=+x", "code.rs"])?;
    repo.git(&["commit", "-q", "-m", "mode"])?;
    let mode = repo.rev("HEAD")?;
    let all = [root, edited, deleted, added, moved, mode];
    let options = [
        DiffOptions::default(),
        DiffOptions {
            whole_file: true,
            ..Default::default()
        },
        DiffOptions {
            algorithm: Some(DiffAlgorithm::Histogram),
            ..Default::default()
        },
        DiffOptions {
            algorithm: Some(DiffAlgorithm::Minimal),
            ..Default::default()
        },
    ];
    let made = same_as_git(repo.path(), &all, &options)?;
    // All but those needing git: none here.
    let files: usize = all
        .iter()
        .map(|h| crate::git::diff::commit_files(repo.path(), h).map(|f| f.len()))
        .sum::<Result<usize, String>>()?;
    assert_eq!(made, files * options.len());
    for (key, value) in [("diff.context", "0"), ("diff.context", "1")] {
        repo.git(&["config", key, value])?;
        same_as_git(repo.path(), &all[1..2], &options[..1])?;
    }
    Ok(())
}

/// This repository's own recent history (or `GITCRAFT_PARITY_REPO`'s): the files of its last
/// commits, at most `GITCRAFT_PARITY_FILES` (100) of them.
#[test]
fn recent_patches_are_made_as_git_prints_them() -> Result<(), String> {
    let other = std::env::var_os("GITCRAFT_PARITY_REPO");
    let repo = other
        .as_deref()
        .map_or(Path::new(env!("CARGO_MANIFEST_DIR")), Path::new);
    let limit: usize = std::env::var("GITCRAFT_PARITY_FILES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(100);
    let Ok(log) = crate::git::run(Some(repo), &["log", "--format=%H", "-n", "300", "HEAD"]) else {
        return Ok(());
    };
    if repo.join(".git/shallow").exists() {
        return Ok(());
    }
    let mut hashes = Vec::new();
    let mut files = 0;
    for hash in log.lines() {
        if files >= limit {
            break;
        }
        // Those whose files only git lists, it diffs too.
        let listed = super::super::commits::files(repo, &[hash.to_owned()])?;
        if let Some((_, Some(listed))) = listed.first() {
            files += listed.len();
            hashes.push(hash.to_owned());
        }
    }
    let made = same_as_git(repo, &hashes, &[DiffOptions::default()])?;
    println!("{made} of {files} patches made in-process");
    Ok(())
}
