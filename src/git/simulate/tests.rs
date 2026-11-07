//! Tests of the merge simulation: parsing `merge-tree`'s answer, and predictions checked
//! against real repositories, which they must leave as they were.

use std::path::{Path, PathBuf};

use super::*;

/// A scratch repository with `main` checked out, removed when dropped.
struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-sim-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Self { dir };
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        run(Some(&self.dir), args).map(|s| s.trim().to_owned())
    }

    fn write(&self, path: &str, contents: &[u8]) -> Result<(), String> {
        let file = self.dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(file, contents).map_err(|e| e.to_string())
    }

    /// Commits everything as it is, and returns the commit.
    fn commit(&self, message: &str) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--allow-empty", "-m", message])?;
        self.git(&["rev-parse", "HEAD"])
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The installed git's features, or `None` (the test then has nothing to check) when it is
/// too old for replays.
fn features_here(repo: &Repo) -> Result<Option<Features>, String> {
    let features = features(repo.path())?;
    Ok(features.replay.then_some(features))
}

/// The paths of a prediction's conflicts, and the replay step it stopped at.
fn conflicts(prediction: &Prediction) -> (Vec<&str>, Option<usize>) {
    match &prediction.outcome {
        Outcome::Conflicts { files, stop } => (
            files.iter().map(|f| f.path.as_str()).collect(),
            stop.as_ref().map(|s| s.index),
        ),
        _ => (Vec::new(), None),
    }
}

/// `main` and `side` both from a `base` commit with `f.txt` = `a b c`, each line on its own.
fn forked(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(name)?;
    repo.write("f.txt", b"a\nb\nc\n")?;
    repo.commit("base")?;
    repo.git(&["branch", "side"])?;
    Ok(repo)
}

#[test]
fn versions_are_read_whatever_the_build() {
    for (out, version) in [
        ("git version 2.43.0\n", Some((2, 43, 0))),
        ("git version 2.45.1.windows.1", Some((2, 45, 1))),
        ("git version 2.39.3 (Apple Git-145)", Some((2, 39, 3))),
        ("git version 2.50.0-rc1", Some((2, 50, 0))),
        ("git version 3.0", Some((3, 0, 0))),
        ("not git", None),
        ("git version x.y", None),
    ] {
        assert_eq!(parse_version(out), version, "{out}");
    }
}

#[test]
fn what_each_version_can_check() {
    let old = Features::of((2, 37, 9));
    assert!(!old.merge && !old.replay && !old.trees);
    assert_eq!(
        old.merge_unavailable().as_deref(),
        Some("Conflict check needs git 2.38 or newer (this is 2.37.9)")
    );
    let merges_only = Features::of((2, 39, 0));
    assert!(merges_only.merge && !merges_only.replay);
    assert!(merges_only.merge_unavailable().is_none());
    assert!(merges_only.replay_unavailable().is_some());
    let commits = Features::of((2, 44, 0));
    assert!(commits.replay && !commits.trees);
    assert!(Features::of((2, 45, 0)).trees);
    assert!(Features::of((3, 0, 0)).trees);
}

#[test]
fn a_clean_answer_is_only_the_tree() -> Result<(), String> {
    let tree = "ec4eff38302efdf54b2a168fa3698fb13371e19c";
    let merged = parse_merge_tree(format!("{tree}\0").as_bytes(), true)?;
    assert_eq!(merged.tree, tree);
    assert!(merged.conflicts.is_empty());
    Ok(())
}

/// As git 2.43 prints it for a binary file, a modify/delete and an add/add of a name with a
/// space and an `ü`.
#[test]
fn conflicts_are_read_with_their_kinds() -> Result<(), String> {
    let out = "1e85aa1cf91bcb17f0d6f1233c1fce7b7868c946\0bin.dat\0del.txt\0sp ace ü.txt\0\0\
        1\0bin.dat\0CONFLICT (binary)\0warning: Cannot merge binary files: bin.dat\n\0\
        1\0bin.dat\0Auto-merging\0Auto-merging bin.dat\n\0\
        1\0bin.dat\0CONFLICT (contents)\0CONFLICT (content): Merge conflict in bin.dat\n\0\
        1\0del.txt\0CONFLICT (modify/delete)\0CONFLICT (modify/delete): del.txt deleted\n\0\
        1\0sp ace ü.txt\0CONFLICT (add/add)\0CONFLICT (add/add): Merge conflict\n\0\
        1\0sp ace ü.txt\0CONFLICT (contents)\0CONFLICT (content): Merge conflict\n\0";
    let merged = parse_merge_tree(out.as_bytes(), false)?;
    let kinds: Vec<(&str, Vec<&str>)> = merged
        .conflicts
        .iter()
        .map(|f| {
            (
                f.path.as_str(),
                f.kinds.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("bin.dat", vec!["binary", "contents"]),
            ("del.txt", vec!["modify/delete"]),
            ("sp ace ü.txt", vec!["add/add", "contents"]),
        ]
    );
    let text_only: Vec<bool> = merged.conflicts.iter().map(|f| f.text_only()).collect();
    assert_eq!(text_only, [false, false, true]);
    Ok(())
}

/// A rename/delete message names both paths; only the conflicted one gets the kind.
#[test]
fn a_message_about_two_paths_marks_the_conflicted_one() -> Result<(), String> {
    let out = "aaaa\0new.txt\0\0\
        2\0old.txt\0new.txt\0CONFLICT (rename/delete)\0old.txt renamed to new.txt\n\0";
    let merged = parse_merge_tree(out.as_bytes(), false)?;
    assert_eq!(merged.conflicts.len(), 1);
    assert_eq!(merged.conflicts[0].kinds, ["rename/delete"]);
    // Without messages (`--no-messages`, a git that prints none) the files are still there.
    let bare = parse_merge_tree(b"aaaa\0x.txt\0\0", false)?;
    assert_eq!(bare.conflicts[0].path, "x.txt");
    assert!(!bare.conflicts[0].text_only());
    Ok(())
}

#[test]
fn a_garbled_answer_is_an_error() {
    assert!(parse_merge_tree(b"", true).is_err());
    assert!(parse_merge_tree(b"fatal: no\0", true).is_err());
    assert!(parse_merge_tree(b"abcd\0\0", false).is_err());
}

#[test]
fn picks_are_read_from_log_records() {
    let out = "c1\0p1 p2\0first\0c2\0\0root commit\0\n";
    let picks = parse_picks(out);
    assert_eq!(
        picks,
        [
            Pick {
                hash: "c1".into(),
                parent: Some("p1".into()),
                summary: "first".into(),
            },
            Pick {
                hash: "c2".into(),
                parent: None,
                summary: "root commit".into(),
            },
        ]
    );
}

#[test]
fn a_clean_merge() -> Result<(), String> {
    let repo = forked("clean")?;
    if features_here(&repo)?.is_none() {
        return Ok(());
    }
    repo.write("f.txt", b"A\nb\nc\n")?;
    let main = repo.commit("main")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("f.txt", b"a\nb\nC\n")?;
    let side = repo.commit("side")?;
    let prediction = merge(repo.path(), &main, &side)?;
    assert_eq!(prediction.outcome, Outcome::Clean { changes: true });
    // Merging what is already in: nothing to do, still no conflict.
    let base = repo.git(&["rev-parse", "side~1"])?;
    let nothing = merge(repo.path(), &side, &base)?;
    assert_eq!(nothing.outcome, Outcome::Clean { changes: false });
    Ok(())
}

#[test]
fn a_content_conflict() -> Result<(), String> {
    let repo = forked("content")?;
    if features_here(&repo)?.is_none() {
        return Ok(());
    }
    repo.write("f.txt", b"a\nmain\nc\n")?;
    let main = repo.commit("main")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("f.txt", b"a\nside\nc\n")?;
    let side = repo.commit("side")?;
    let prediction = merge(repo.path(), &main, &side)?;
    assert_eq!(conflicts(&prediction), (vec!["f.txt"], None));
    let Outcome::Conflicts { files, .. } = &prediction.outcome else {
        return Err("no conflict".into());
    };
    assert!(files[0].text_only(), "{files:?}");
    Ok(())
}

#[test]
fn a_rename_delete_conflict() -> Result<(), String> {
    let repo = forked("rename-delete")?;
    if features_here(&repo)?.is_none() {
        return Ok(());
    }
    repo.git(&["rm", "-q", "f.txt"])?;
    let main = repo.commit("delete")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.git(&["mv", "f.txt", "g.txt"])?;
    let side = repo.commit("rename")?;
    let prediction = merge(repo.path(), &main, &side)?;
    let Outcome::Conflicts { files, .. } = &prediction.outcome else {
        return Err(format!("no conflict: {prediction:?}"));
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].path, "g.txt");
    assert_eq!(files[0].kinds, ["rename/delete"]);
    assert!(!files[0].text_only());
    Ok(())
}

#[test]
fn a_binary_conflict() -> Result<(), String> {
    let repo = forked("binary")?;
    if features_here(&repo)?.is_none() {
        return Ok(());
    }
    repo.write("img.bin", b"\0\x01base")?;
    repo.commit("add a binary")?;
    repo.git(&["branch", "-f", "side"])?;
    repo.write("img.bin", b"\0\x01main")?;
    let main = repo.commit("main")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("img.bin", b"\0\x01side")?;
    let side = repo.commit("side")?;
    let prediction = merge(repo.path(), &main, &side)?;
    let Outcome::Conflicts { files, .. } = &prediction.outcome else {
        return Err(format!("no conflict: {prediction:?}"));
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].kinds.iter().any(|k| k == "binary"), "{files:?}");
    assert!(!files[0].text_only());
    Ok(())
}

/// Five commits to rebase, of which only the third touches what `main` changed: the replay
/// stops there, which merging the branches as a whole would not show as such.
#[test]
fn a_rebase_stops_at_the_commit_that_conflicts() -> Result<(), String> {
    let repo = forked("rebase")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.write("f.txt", b"a\nmain\nc\n")?;
    let main = repo.commit("main")?;
    repo.git(&["checkout", "-q", "side"])?;
    for n in 1..=5 {
        if n == 3 {
            repo.write("f.txt", b"a\nside\nc\n")?;
        } else {
            repo.write(&format!("own{n}.txt"), format!("{n}\n").as_bytes())?;
        }
        repo.commit(&format!("side {n}"))?;
    }
    let third = repo.git(&["rev-parse", "side~2"])?;
    for features in [features, Features::of((2, 40, 0))] {
        let prediction = rebase(repo.path(), features, "side", &main)?;
        assert_eq!(prediction.commits, 5);
        assert_eq!(conflicts(&prediction), (vec!["f.txt"], Some(3)));
        let Outcome::Conflicts {
            stop: Some(stop), ..
        } = &prediction.outcome
        else {
            return Err("no stop".into());
        };
        assert_eq!(stop.commit.hash, third);
        assert_eq!(stop.commit.summary, "side 3");
    }
    // Without that commit's change on main, all five apply, on either kind of git.
    let base = repo.git(&["rev-parse", "main~1"])?;
    repo.git(&["checkout", "-q", "-b", "other", &base])?;
    repo.write("elsewhere.txt", b"x\n")?;
    let other = repo.commit("elsewhere")?;
    for features in [features, Features::of((2, 40, 0))] {
        let prediction = rebase(repo.path(), features, "side", &other)?;
        assert_eq!(prediction.outcome, Outcome::Clean { changes: true });
    }
    Ok(())
}

/// What `main` already has (cherry-picked before) is left out of the rebase, as git does.
#[test]
fn a_rebase_skips_what_is_already_there() -> Result<(), String> {
    let repo = forked("rebase-skip")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("f.txt", b"a\nside\nc\n")?;
    let picked = repo.commit("side")?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.git(&["cherry-pick", &picked])?;
    // Then main changes that line again: replaying `side` would conflict, were it replayed.
    repo.write("f.txt", b"a\nmain\nc\n")?;
    let main = repo.commit("main")?;
    let prediction = rebase(repo.path(), features, "side", &main)?;
    assert_eq!(prediction.commits, 0);
    assert!(matches!(prediction.outcome, Outcome::Clean { .. }));
    Ok(())
}

#[test]
fn a_cherry_pick() -> Result<(), String> {
    let repo = forked("cherry-pick")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.write("f.txt", b"a\nmain\nc\n")?;
    let main = repo.commit("main")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("own.txt", b"own\n")?;
    let clean = repo.commit("own file")?;
    repo.write("f.txt", b"a\nside\nc\n")?;
    let conflicting = repo.commit("same line")?;
    for features in [features, Features::of((2, 40, 0))] {
        let ok = cherry_pick(repo.path(), features, &main, &[&clean])?;
        assert_eq!(ok.outcome, Outcome::Clean { changes: true });
        // Only its own change: the commit before it is not picked along.
        let bad = cherry_pick(repo.path(), features, &main, &[&conflicting])?;
        assert_eq!(conflicts(&bad), (vec!["f.txt"], Some(1)));
        let both = cherry_pick(repo.path(), features, &main, &[&clean, &conflicting])?;
        assert_eq!(conflicts(&both), (vec!["f.txt"], Some(2)));
    }
    // Picking what is already there changes nothing.
    let again = cherry_pick(repo.path(), features, &conflicting, &[&clean])?;
    assert_eq!(again.outcome, Outcome::Clean { changes: false });
    Ok(())
}

/// A root commit has no parent to replay from: git 2.45 merges from the empty tree, an older
/// one can't, which is said rather than guessed.
#[test]
fn a_root_commit() -> Result<(), String> {
    let repo = Repo::new("root")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.write("root.txt", b"root\n")?;
    let root = repo.commit("root")?;
    repo.git(&["checkout", "-q", "--orphan", "other"])?;
    repo.git(&["rm", "-q", "-r", "--cached", "."])?;
    std::fs::remove_file(repo.path().join("root.txt")).map_err(|e| e.to_string())?;
    repo.write("other.txt", b"other\n")?;
    let other = repo.commit("other")?;
    let old = cherry_pick(repo.path(), Features::of((2, 40, 0)), &other, &[&root])?;
    assert!(matches!(old.outcome, Outcome::Unsure(_)), "{old:?}");
    if features.trees {
        let new = cherry_pick(repo.path(), features, &other, &[&root])?;
        assert_eq!(new.outcome, Outcome::Clean { changes: true });
    }
    Ok(())
}

#[test]
fn file_names_with_spaces_and_unicode() -> Result<(), String> {
    let repo = Repo::new("names")?;
    if features_here(&repo)?.is_none() {
        return Ok(());
    }
    let names = ["with space.txt", "ünïcödé/日本語.txt", "tab\tname.txt"];
    // Windows allows no tab in a file name.
    let names = if cfg!(windows) {
        &names[..2]
    } else {
        &names[..]
    };
    for name in names {
        repo.write(name, b"base\n")?;
    }
    repo.commit("base")?;
    repo.git(&["branch", "side"])?;
    for name in names {
        repo.write(name, b"main\n")?;
    }
    let main = repo.commit("main")?;
    repo.git(&["checkout", "-q", "side"])?;
    for name in names {
        repo.write(name, b"side\n")?;
    }
    let side = repo.commit("side")?;
    let prediction = merge(repo.path(), &main, &side)?;
    let (mut found, _) = conflicts(&prediction);
    found.sort_unstable();
    let mut expected = names.to_vec();
    expected.sort_unstable();
    assert_eq!(found, expected);
    Ok(())
}

/// Everything git would look at, as text: HEAD, every ref, the index, the working tree.
fn snapshot(repo: &Repo) -> Result<(String, String, Vec<u8>, String), String> {
    let index = std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?;
    Ok((
        repo.git(&["symbolic-ref", "-q", "HEAD"])?,
        repo.git(&["for-each-ref", "--format=%(refname) %(objectname)"])?,
        index,
        repo.git(&["status", "--porcelain=v2", "--untracked-files=all"])?,
    ))
}

/// Checking leaves the repository as it was: same branch, refs, index and files, uncommitted
/// changes (staged and not) included, and no merge or rebase in progress.
#[test]
fn checking_leaves_the_working_tree_and_index_alone() -> Result<(), String> {
    let repo = forked("untouched")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.write("f.txt", b"a\nmain\nc\n")?;
    let main = repo.commit("main")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("f.txt", b"a\nside\nc\n")?;
    let side = repo.commit("side")?;
    repo.write("staged.txt", b"staged\n")?;
    repo.git(&["add", "staged.txt"])?;
    repo.write("f.txt", b"a\nunsaved\nc\n")?;
    repo.write("untracked.txt", b"new\n")?;
    let before = snapshot(&repo)?;
    let files_before = std::fs::read(repo.path().join("f.txt")).map_err(|e| e.to_string())?;
    merge(repo.path(), &side, &main)?;
    rebase(repo.path(), features, &side, &main)?;
    rebase(repo.path(), Features::of((2, 40, 0)), &side, &main)?;
    cherry_pick(repo.path(), features, &side, &[&main])?;
    assert_eq!(snapshot(&repo)?, before);
    assert_eq!(
        std::fs::read(repo.path().join("f.txt")).map_err(|e| e.to_string())?,
        files_before
    );
    for state in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "rebase-merge",
        "rebase-apply",
    ] {
        assert!(!repo.path().join(".git").join(state).exists(), "{state}");
    }
    Ok(())
}

/// Without the git it needs, a replay says why instead of guessing.
#[test]
fn an_old_git_says_why() -> Result<(), String> {
    let repo = forked("old-git")?;
    let head = repo.git(&["rev-parse", "HEAD"])?;
    let err = rebase(repo.path(), Features::of((2, 39, 1)), "side", &head);
    assert_eq!(
        err,
        Err("Conflict check needs git 2.40 or newer (this is 2.39.1)".to_owned())
    );
    Ok(())
}

/// A branch that changes a line and then changes it back merges cleanly with an upstream that
/// changed that line too, yet rebasing it stops at its first commit: why the toolbar checks a
/// rebase when Pull rebases.
#[test]
fn a_clean_merge_can_still_be_a_conflicting_rebase() -> Result<(), String> {
    let repo = forked("merge-vs-rebase")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.write("f.txt", b"a\nupstream\nc\n")?;
    let upstream = repo.commit("upstream")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("f.txt", b"a\nmine\nc\n")?;
    repo.commit("change it")?;
    repo.write("f.txt", b"a\nb\nc\n")?;
    let head = repo.commit("change it back")?;
    let merged = merge(repo.path(), &head, &upstream)?;
    assert_eq!(merged.outcome, Outcome::Clean { changes: true });
    let rebased = rebase(repo.path(), features, &head, &upstream)?;
    assert_eq!(conflicts(&rebased), (vec!["f.txt"], Some(1)));
    Ok(())
}

/// A partial clone is told apart: merging there would download what it lacks.
#[test]
fn a_partial_clone_is_recognised() -> Result<(), String> {
    let repo = forked("partial")?;
    assert_eq!(partial_clone(repo.path()), Ok(false));
    repo.git(&["config", "extensions.partialClone", "origin"])?;
    assert_eq!(partial_clone(repo.path()), Ok(true));
    // Not knowing (no repository there) is an error, not a "no".
    let gone = repo.path().join("not-a-folder");
    assert!(partial_clone(&gone).is_err());
    Ok(())
}

/// Two branches with the same files but their own commits still merge into a merge commit:
/// not "already merged".
#[test]
fn same_files_on_another_history_is_not_already_merged() -> Result<(), String> {
    let repo = forked("same-tree")?;
    if features_here(&repo)?.is_none() {
        return Ok(());
    }
    repo.write("g.txt", b"same\n")?;
    let main = repo.commit("main adds g")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("g.txt", b"same\n")?;
    let side = repo.commit("side adds g too")?;
    let merged = merge(repo.path(), &main, &side)?;
    assert_eq!(merged.outcome, Outcome::Clean { changes: true });
    Ok(())
}

/// When the user's git recreates merges in a rebase, a branch with merges isn't predicted
/// commit by commit: it says it can't be sure.
#[test]
fn a_rebase_that_recreates_merges_is_unsure() -> Result<(), String> {
    let repo = forked("rebase-merges")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.write("up.txt", b"up\n")?;
    let main = repo.commit("upstream")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.git(&["checkout", "-q", "-b", "inner"])?;
    repo.write("inner.txt", b"x\n")?;
    repo.commit("inner")?;
    repo.git(&["checkout", "-q", "side"])?;
    repo.write("own.txt", b"y\n")?;
    repo.commit("own")?;
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge inner", "inner"])?;
    let head = repo.git(&["rev-parse", "HEAD"])?;
    let plain = rebase(repo.path(), features, &head, &main)?;
    assert!(matches!(plain.outcome, Outcome::Clean { .. }), "{plain:?}");
    repo.git(&["config", "rebase.rebaseMerges", "rebase-cousins"])?;
    let recreated = rebase(repo.path(), features, &head, &main)?;
    assert!(
        matches!(recreated.outcome, Outcome::Unsure(_)),
        "{recreated:?}"
    );
    Ok(())
}

/// After the upstream was rewritten (a commit the branch started on dropped and replaced),
/// a pull replays only the branch's own commits, from the upstream's fork point; a plain
/// rebase would replay the dropped commit too, and conflict on it.
#[test]
fn a_pull_rebase_starts_from_the_fork_point() -> Result<(), String> {
    let repo = forked("fork-point")?;
    let Some(features) = features_here(&repo)? else {
        return Ok(());
    };
    repo.git(&["config", "core.logAllRefUpdates", "true"])?;
    repo.write("f.txt", b"a\nold\nc\n")?;
    repo.commit("upstream, before the rewrite")?;
    repo.git(&["checkout", "-q", "-b", "topic"])?;
    repo.write("own.txt", b"mine\n")?;
    let head = repo.commit("own work")?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.git(&["reset", "-q", "--hard", "HEAD~1"])?;
    repo.write("f.txt", b"a\nnew\nc\n")?;
    let upstream = repo.commit("upstream, rewritten")?;
    let plain = rebase(repo.path(), features, &head, &upstream)?;
    assert_eq!(conflicts(&plain).0, ["f.txt"]);
    let pulled = pull_rebase(repo.path(), features, &head, "main", &upstream)?;
    assert_eq!(pulled.commits, 1);
    assert_eq!(pulled.outcome, Outcome::Clean { changes: true });
    Ok(())
}
