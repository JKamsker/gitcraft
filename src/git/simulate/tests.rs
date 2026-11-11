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
    // Two commits that replay cleanly, so that an old git's replay makes its scratch commits.
    repo.write("own1.txt", b"1\n")?;
    let own1 = repo.commit("own 1")?;
    repo.write("own2.txt", b"2\n")?;
    let own2 = repo.commit("own 2")?;
    repo.write("staged.txt", b"staged\n")?;
    repo.git(&["add", "staged.txt"])?;
    repo.write("f.txt", b"a\nunsaved\nc\n")?;
    repo.write("untracked.txt", b"new\n")?;
    let before = snapshot(&repo)?;
    let read = |name: &str| std::fs::read(repo.path().join(name)).map_err(|e| e.to_string());
    let files_before = (read("f.txt")?, read("untracked.txt")?);
    merge(repo.path(), &side, &main)?;
    rebase(repo.path(), features, &side, &main)?;
    rebase(repo.path(), Features::of((2, 40, 0)), &side, &main)?;
    cherry_pick(repo.path(), features, &side, &[&main])?;
    let old_git = Features::of((2, 40, 0));
    let clean = cherry_pick(repo.path(), old_git, &main, &[&own1, &own2])?;
    assert_eq!(clean.outcome, Outcome::Clean { changes: true });
    assert_eq!(snapshot(&repo)?, before);
    assert_eq!((read("f.txt")?, read("untracked.txt")?), files_before);
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
    repo.git(&["config", "remote.other.promisor", "false"])?;
    assert_eq!(partial_clone(repo.path()), Ok(false));
    repo.git(&["config", "remote.other.promisor", "true"])?;
    assert_eq!(partial_clone(repo.path()), Ok(true));
    repo.git(&["config", "--unset", "remote.other.promisor"])?;
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

mod parsing;

mod pull;
