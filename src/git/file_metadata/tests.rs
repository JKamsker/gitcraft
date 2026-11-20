//! Real-Git tests of the executable bit and case-only renames, run as the operation runner
//! runs them (each op is `git <args>` in the repository), and their undo.

use super::*;
use crate::git::fixture::Repo;

mod case;

fn run_all(repo: &Repo, ops: &[Vec<String>]) -> Result<(), String> {
    for op in ops {
        let args: Vec<&str> = op.iter().map(String::as_str).collect();
        repo.git(&args)?;
    }
    Ok(())
}

/// `ls-files -s` of the whole index: `(mode, blob, path)` of each stage-0 entry.
fn index(repo: &Repo) -> Result<Vec<(String, String, String)>, String> {
    let listed = repo.git(&["ls-files", "-s", "-z"])?;
    Ok(listed
        .split('\0')
        .filter_map(|line| {
            let (meta, path) = line.split_once('\t')?;
            let mut m = meta.split(' ');
            Some((m.next()?.into(), m.next()?.into(), path.into()))
        })
        .collect())
}

fn mode_of(repo: &Repo, path: &str) -> Result<String, String> {
    index(repo)?
        .into_iter()
        .find(|(_, _, p)| p == path)
        .map(|(mode, _, _)| mode)
        .ok_or_else(|| format!("{path} not in the index"))
}

fn blob_of(repo: &Repo, path: &str) -> Result<String, String> {
    repo.rev(&format!(":{path}"))
}

fn read(repo: &Repo, path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(repo.dir.join(path)).map_err(|e| e.to_string())
}

/// The executable edit of `path` as the dialog makes it: read, patch saved, planned.
fn executable(repo: &Repo, path: &str) -> Result<Plan, String> {
    let snap = snapshot(repo.path(), path, false)?;
    let patch = save_executable_patch(repo.path(), &snap)?;
    executable_plan(&snap, &patch)
}

#[test]
fn executable_bit_toggles_only_the_index_mode_and_keeps_unstaged_edits() -> Result<(), String> {
    let repo = Repo::new("file-metadata-chmod")?;
    let path = "tools/rün me.sh";
    repo.commit("script", &[(path, Some("echo one\n"))])?;
    let edited = b"echo one\r\necho two \xc3\xbc\n";
    std::fs::write(repo.dir.join(path), edited).map_err(|e| e.to_string())?;
    let blob = blob_of(&repo, path)?;

    let plan = executable(&repo, path)?;
    assert_eq!((plan.from.as_str(), plan.to.as_str()), ("100644", "100755"));
    run_all(&repo, &plan.ops)?;
    assert_eq!(mode_of(&repo, path)?, "100755");
    // The blob in the index is the committed one: the edit wasn't staged with the mode.
    assert_eq!(blob_of(&repo, path)?, blob);
    assert_eq!(read(&repo, path)?, edited);
    let staged = repo.git(&["diff", "--cached", "--summary"])?;
    assert!(staged.contains("mode change 100644 => 100755"), "{staged}");
    assert!(!repo.git(&["diff", "--name-only"])?.trim().is_empty());

    run_all(&repo, &plan.undo)?;
    assert_eq!(mode_of(&repo, path)?, "100644");
    assert_eq!(blob_of(&repo, path)?, blob);

    // Committed executable: clearing goes the other way.
    run_all(&repo, &plan.ops)?;
    repo.git(&["commit", "-q", "-m", "executable"])?;
    let clear = executable(&repo, path)?;
    assert!(plan.executable && !clear.executable);
    run_all(&repo, &clear.ops)?;
    assert_eq!(mode_of(&repo, path)?, "100644");
    assert_eq!(read(&repo, path)?, edited);
    Ok(())
}

/// Undo and redo set the mode of what the index holds then: contents staged after the edit
/// stay staged, rather than the blob the edit saw coming back.
#[test]
fn executable_bit_undo_and_redo_keep_contents_staged_since() -> Result<(), String> {
    let repo = Repo::new("file-metadata-chmod-undo")?;
    repo.commit("script", &[("run.sh", Some("one\n"))])?;
    let plan = executable(&repo, "run.sh")?;
    run_all(&repo, &plan.ops)?;
    std::fs::write(repo.dir.join("run.sh"), "two\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "--", "run.sh"])?;
    let since = blob_of(&repo, "run.sh")?;
    run_all(&repo, &plan.undo)?;
    assert_eq!(mode_of(&repo, "run.sh")?, "100644");
    assert_eq!(blob_of(&repo, "run.sh")?, since);
    run_all(&repo, &plan.ops)?;
    assert_eq!(mode_of(&repo, "run.sh")?, "100755");
    assert_eq!(blob_of(&repo, "run.sh")?, since);
    Ok(())
}

#[test]
fn executable_bit_is_refused_for_links_untracked_and_conflicted_files() -> Result<(), String> {
    let repo = Repo::new("file-metadata-chmod-refused")?;
    repo.commit("base", &[("a.txt", Some("a\n"))])?;
    let blob = blob_of(&repo, "a.txt")?;
    let link = [
        "update-index",
        "--add",
        "--cacheinfo",
        "120000",
        &blob,
        "link",
    ];
    repo.git(&link)?;
    assert!(executable(&repo, "link").is_err_and(|e| e.contains("120000")));
    std::fs::write(repo.dir.join("new.txt"), "n").map_err(|e| e.to_string())?;
    assert!(executable(&repo, "new.txt").is_err());
    let conflicted = Snapshot {
        conflicted: true,
        ..snapshot(repo.path(), "a.txt", false)?
    };
    assert!(executable_plan(&conflicted, "unused.patch").is_err());
    Ok(())
}

#[test]
fn mode_patches_quote_paths_as_git_does() {
    assert_eq!(quote("a \"b\"\\c\tü"), r#""a \"b\"\\c\tü""#);
    assert_eq!(quote("bell\u{7}"), r#""bell\007""#);
    assert_eq!(
        mode_patch("x y", "100644", "100755"),
        "diff --git \"a/x y\" \"b/x y\"\nold mode 100644\nnew mode 100755\n"
    );
}
