use super::*;
use crate::git::fixture::Repo;

type Described = Vec<(char, String, Option<String>, Option<u32>, Option<u32>)>;

fn describe(files: &[ChangedFile]) -> Described {
    files
        .iter()
        .map(|f| {
            (
                f.status,
                f.path.clone(),
                f.old_path.clone(),
                f.added,
                f.removed,
            )
        })
        .collect()
}

/// Each of `hashes` lists the same files, with the same counts, as git lists them: read
/// in-process unless it is one of `by_git`. So does the batch, whichever reads them.
fn same_as_git(repo: &Path, hashes: &[String], by_git: &[&String]) -> Result<(), String> {
    let native = files(repo, hashes)?;
    let git = crate::git::diff::commit_files_batch_by_git(repo, hashes)?;
    assert_eq!(native.len(), hashes.len());
    for ((hash, ours), (git_hash, theirs)) in native.iter().zip(&git) {
        assert_eq!(hash, git_hash);
        assert_eq!(ours.is_none(), by_git.contains(&hash), "who lists {hash}");
        if let Some(ours) = ours {
            assert_eq!(describe(ours), describe(theirs), "the files of {hash}");
        }
    }
    let batch = crate::git::diff::commit_files_batch(repo, hashes)?;
    for ((hash, ours), (_, theirs)) in batch.iter().zip(&git) {
        assert_eq!(
            describe(ours),
            describe(theirs),
            "the batch's files of {hash}"
        );
    }
    for hash in hashes {
        let ours = detail(repo, hash)?;
        let theirs = crate::git::commits::commit_detail_by_git(repo, hash)?;
        assert_eq!(
            format!("{ours:?}"),
            format!("{theirs:?}"),
            "the detail of {hash}"
        );
    }
    Ok(())
}

fn blob(repo: &Repo, name: &str, text: &str) -> Result<String, String> {
    std::fs::write(repo.path().join(name), text).map_err(|e| e.to_string())?;
    let id = repo.git(&["hash-object", "-w", name])?;
    std::fs::remove_file(repo.path().join(name)).map_err(|e| e.to_string())?;
    Ok(id.trim().to_owned())
}

#[test]
fn files_and_details_read_in_process_are_those_git_reads() -> Result<(), String> {
    let repo = Repo::new("native-commit-files")?;
    repo.git(&["config", "diff.up.binary", "true"])?;
    let root = repo.commit(
        "root\n\nWith a body.",
        &[
            ("a.txt", Some("one\ntwo\nthree\nfour\n")),
            ("bin.dat", Some("\0\u{1}binary\0")),
            ("no-newline.txt", Some("x")),
            ("empty.txt", Some("")),
            ("dir/nested.txt", Some("nested\n")),
            ("forced.txt", Some("text\n")),
            ("driven.up", Some("driven\n")),
            ("crlf.txt", Some("one\ntwo\n")),
            (".gitattributes", Some("forced.txt -diff\n*.up diff=up\n")),
        ],
    )?;
    repo.git(&["switch", "-q", "-c", "side"])?;
    let side = repo.commit("side", &[("side.txt", Some("side\n"))])?;
    repo.git(&["switch", "-q", "main"])?;
    repo.git(&["mv", "a.txt", "b.txt"])?;
    let renamed = repo.commit(
        "rename as it was, and change others",
        &[
            ("bin.dat", Some("\0\u{2}binary\0")),
            ("no-newline.txt", Some("x\n")),
            ("forced.txt", Some("changed\n")),
            ("driven.up", Some("changed\n")),
            ("crlf.txt", Some("one\r\ntwo\r\n")),
        ],
    )?;
    let deleted = repo.commit("delete", &[("dir/nested.txt", None)])?;
    let added = repo.commit("add", &[("ünïcode name.txt", Some("ü\r\nwindows\r\n"))])?;
    repo.git(&["mv", "b.txt", "c.txt"])?;
    let changed_rename = repo.commit(
        "rename with a change",
        &[("c.txt", Some("one\ntwo\nthree\nfive\n"))],
    )?;
    repo.git(&["update-index", "--chmod=+x", "empty.txt"])?;
    repo.git(&["commit", "-q", "-m", "mode"])?;
    let mode = repo.rev("HEAD")?;
    // update-index changes only the index; materialize the mode before switching branches.
    repo.git(&["checkout-index", "-f", "--", "empty.txt"])?;
    let link = blob(&repo, "link-target", "b.txt")?;
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("120000,{link},link"),
    ])?;
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{root},sub"),
    ])?;
    repo.git(&["commit", "-q", "-m", "link and submodule"])?;
    let linked = repo.rev("HEAD")?;
    repo.git(&["update-index", "--cacheinfo", &format!("160000,{side},sub")])?;
    repo.git(&["commit", "-q", "-m", "submodule moved"])?;
    let moved = repo.rev("HEAD")?;
    repo.git(&["merge", "-q", "--no-ff", "--no-edit", "side"])?;
    let merge = repo.rev("HEAD")?;
    repo.git(&["switch", "-q", "-c", "unmerged", &root])?;
    let unmerged = repo.commit("unmerged", &[("other.txt", Some("other\n"))])?;
    repo.git(&["switch", "-q", "main"])?;
    let all = [
        root,
        side,
        renamed,
        deleted,
        added,
        changed_rename.clone(),
        mode,
        linked,
        moved,
        merge,
        unmerged,
    ];
    same_as_git(repo.path(), &all, &[&changed_rename])
}

/// This repository's own recent history, when it is there to read (not a shallow clone).
#[test]
fn recent_history_reads_in_process_as_git_reads_it() -> Result<(), String> {
    // `GITCRAFT_PARITY_REPO` compares another repository's instead.
    let other = std::env::var_os("GITCRAFT_PARITY_REPO");
    let repo = other
        .as_deref()
        .map_or(Path::new(env!("CARGO_MANIFEST_DIR")), Path::new);
    let Ok(log) = crate::git::run(Some(repo), &["log", "--format=%H", "-n", "300", "HEAD"]) else {
        return Ok(());
    };
    let hashes: Vec<String> = log.lines().map(str::to_owned).collect();
    if hashes.len() < 300 || repo.join(".git/shallow").exists() {
        return Ok(());
    }
    let native = files(repo, &hashes)?;
    let git = crate::git::diff::commit_files_batch_by_git(repo, &hashes)?;
    let differing: Vec<String> = native
        .iter()
        .zip(&git)
        .filter_map(|((hash, ours), (_, theirs))| {
            let (ours, theirs) = (describe(ours.as_ref()?), describe(theirs));
            if tie(&ours) == tie(&theirs) {
                return None;
            }
            let only_ours: Vec<_> = ours.iter().filter(|f| !theirs.contains(f)).collect();
            let only_theirs: Vec<_> = theirs.iter().filter(|f| !ours.contains(f)).collect();
            Some(format!("{hash}: ours {only_ours:?}, git's {only_theirs:?}"))
        })
        .collect();
    assert!(differing.is_empty(), "{}", differing.join("\n"));
    let by_git = native.iter().filter(|(_, files)| files.is_none()).count();
    println!("{by_git} of {} commits listed by git", hashes.len());
    Ok(())
}

type Tie<'a> = (
    Vec<&'a String>,
    Vec<(char, Option<&'a String>, Option<u32>, Option<u32>)>,
);

/// What two listings of a commit's files have in common when they are the same but for a tie:
/// of equally similar sources, git and gitoxide may take another for a rename (two identical
/// files deleted, one of them moved). The same files, counted the same.
fn tie(files: &Described) -> Tie<'_> {
    let mut paths: Vec<_> = files
        .iter()
        .flat_map(|f| [Some(&f.1), f.2.as_ref()])
        .flatten()
        .collect();
    paths.sort();
    let mut counts: Vec<_> = files
        .iter()
        .map(|f| (f.0, (f.0 != 'D').then_some(&f.1), f.3, f.4))
        .collect();
    counts.sort();
    (paths, counts)
}
