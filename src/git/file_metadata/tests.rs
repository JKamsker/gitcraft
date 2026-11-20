//! Real-Git tests of the executable bit and case-only renames, run as the operation runner
//! runs them (each op is `git <args>` in the repository), and their undo.

use super::*;
use crate::git::fixture::Repo;

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

fn read(repo: &Repo, path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(repo.dir.join(path)).map_err(|e| e.to_string())
}

#[test]
fn executable_bit_toggles_only_the_index_mode_and_keeps_unstaged_edits() -> Result<(), String> {
    let repo = Repo::new("file-metadata-chmod")?;
    repo.commit("script", &[("tools/run.sh", Some("echo one\n"))])?;
    let edited = b"echo one\r\necho two \xc3\xbc\n";
    std::fs::write(repo.dir.join("tools/run.sh"), edited).map_err(|e| e.to_string())?;
    let blob = repo.rev(":tools/run.sh")?;

    let plan = executable_plan(&snapshot(repo.path(), "tools/run.sh", false)?)?;
    assert_eq!((plan.from.as_str(), plan.to.as_str()), ("100644", "100755"));
    run_all(&repo, &plan.ops)?;
    assert_eq!(mode_of(&repo, "tools/run.sh")?, "100755");
    // The blob in the index is the committed one: the edit wasn't staged with the mode.
    assert_eq!(repo.rev(":tools/run.sh")?, blob);
    assert_eq!(read(&repo, "tools/run.sh")?, edited);
    let staged = repo.git(&["diff", "--cached", "--summary"])?;
    assert!(staged.contains("mode change 100644 => 100755"), "{staged}");
    assert!(repo.git(&["diff", "--name-only"])?.contains("tools/run.sh"));

    run_all(&repo, &plan.undo)?;
    assert_eq!(mode_of(&repo, "tools/run.sh")?, "100644");
    assert_eq!(repo.rev(":tools/run.sh")?, blob);

    // Committed executable: clearing goes the other way.
    run_all(&repo, &plan.ops)?;
    repo.git(&["commit", "-q", "-m", "executable"])?;
    let clear = executable_plan(&snapshot(repo.path(), "tools/run.sh", false)?)?;
    assert!(is_executable(&clear.from));
    run_all(&repo, &clear.ops)?;
    assert_eq!(mode_of(&repo, "tools/run.sh")?, "100644");
    assert_eq!(read(&repo, "tools/run.sh")?, edited);
    Ok(())
}

#[test]
fn executable_bit_is_refused_for_links_untracked_and_conflicted_files() -> Result<(), String> {
    let repo = Repo::new("file-metadata-chmod-refused")?;
    repo.commit("base", &[("a.txt", Some("a\n"))])?;
    let blob = repo.rev(":a.txt")?;
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        "120000",
        &blob,
        "link",
    ])?;
    let link = executable_plan(&snapshot(repo.path(), "link", false)?);
    assert!(link.is_err_and(|e| e.contains("120000")));
    std::fs::write(repo.dir.join("new.txt"), "n").map_err(|e| e.to_string())?;
    assert!(executable_plan(&snapshot(repo.path(), "new.txt", false)?).is_err());
    let conflicted = Snapshot {
        conflicted: true,
        ..snapshot(repo.path(), "a.txt", false)?
    };
    assert!(executable_plan(&conflicted).is_err());
    Ok(())
}

#[test]
fn case_only_rename_keeps_staged_and_unstaged_edits_byte_exact() -> Result<(), String> {
    let repo = Repo::new("file-metadata-case")?;
    repo.commit("base", &[("Docs/Read Më.TXT", Some("one\n"))])?;
    std::fs::write(repo.dir.join("Docs/Read Më.TXT"), "one\ntwo\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "--", "Docs/Read Më.TXT"])?;
    let staged = repo.rev(":Docs/Read Më.TXT")?;
    let edited = b"one\r\ntwo\nthree \xc3\xa9\n\x00tail";
    std::fs::write(repo.dir.join("Docs/Read Më.TXT"), edited).map_err(|e| e.to_string())?;

    let snap = snapshot(repo.path(), "Docs/Read Më.TXT", true)?;
    let plan = case_rename_plan(&snap, "read më.txt")?;
    assert_eq!(plan.to, "Docs/read më.txt");
    run_all(&repo, &plan.ops)?;

    let paths: Vec<String> = index(&repo)?.into_iter().map(|(_, _, p)| p).collect();
    assert_eq!(paths, ["Docs/read më.txt"]);
    assert_eq!(repo.rev(":Docs/read më.txt")?, staged);
    assert_eq!(read(&repo, "Docs/read më.txt")?, edited);
    // The name on disk has the new case, and the temporary name is gone.
    let on_disk: Vec<String> = std::fs::read_dir(repo.dir.join("Docs"))
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(on_disk, ["read më.txt"]);

    run_all(&repo, &plan.undo)?;
    let paths: Vec<String> = index(&repo)?.into_iter().map(|(_, _, p)| p).collect();
    assert_eq!(paths, ["Docs/Read Më.TXT"]);
    assert_eq!(read(&repo, "Docs/Read Më.TXT")?, edited);
    assert_eq!(repo.rev(":Docs/Read Më.TXT")?, staged);
    Ok(())
}

#[test]
fn case_only_rename_refuses_collisions_and_other_renames() -> Result<(), String> {
    let repo = Repo::new("file-metadata-case-refused")?;
    repo.commit(
        "base",
        &[("a/Foo.txt", Some("foo\n")), ("Top.md", Some("t\n"))],
    )?;
    let blob = repo.rev(":a/Foo.txt")?;
    // Another tracked path that differs only in case (as a repository made elsewhere has).
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        "100644",
        &blob,
        "a/FOO.TXT",
    ])?;
    let snap = snapshot(repo.path(), "a/Foo.txt", true)?;
    let collision = case_rename_plan(&snap, "foo.txt");
    assert!(collision.is_err_and(|e| e.contains("a/FOO.TXT")));
    assert!(case_rename_plan(&snap, "Foo.txt").is_err());
    assert!(case_rename_plan(&snap, "Bar.txt").is_err());
    assert!(case_rename_plan(&snap, "b/foo.txt").is_err());
    assert!(case_rename_plan(&snap, "").is_err());

    // A file at the top of the repository, and one deleted in the working tree.
    let top = snapshot(repo.path(), "Top.md", true)?;
    assert!(top.siblings.contains("a") && top.siblings.contains("Top.md"));
    assert_eq!(case_rename_plan(&top, "TOP.md")?.to, "TOP.md");
    std::fs::remove_file(repo.dir.join("Top.md")).map_err(|e| e.to_string())?;
    assert!(case_rename_plan(&snapshot(repo.path(), "Top.md", true)?, "top.md").is_err());
    Ok(())
}

#[test]
fn case_only_rename_steps_around_a_taken_temporary_name() -> Result<(), String> {
    let repo = Repo::new("file-metadata-case-temp")?;
    repo.commit("base", &[("Name.rs", Some("fn main() {}\n"))])?;
    // Untracked (and ignored) files in the way of the first temporary name keep their bytes.
    let taken = ".Name.rs.gitcraft-case-rename";
    std::fs::write(repo.dir.join(".gitignore"), format!("{taken}\n")).map_err(|e| e.to_string())?;
    std::fs::write(repo.dir.join(taken.to_uppercase()), "keep").map_err(|e| e.to_string())?;
    let plan = case_rename_plan(&snapshot(repo.path(), "Name.rs", true)?, "name.rs")?;
    assert!(
        plan.ops[0].contains(&format!("{taken}-1")),
        "{:?}",
        plan.ops
    );
    run_all(&repo, &plan.ops)?;
    assert!(index(&repo)?.iter().any(|(_, _, p)| p == "name.rs"));
    assert_eq!(read(&repo, &taken.to_uppercase())?, b"keep");
    Ok(())
}

#[test]
fn temporary_names_avoid_every_letter_case() {
    let siblings: BTreeSet<String> = [".A.gitcraft-case-rename", ".a.GITCRAFT-CASE-RENAME-1"]
        .map(str::to_owned)
        .into();
    assert_eq!(
        temporary_name("A", &siblings).as_deref(),
        Some(".A.gitcraft-case-rename-2")
    );
    assert_eq!(split("a/b/c.txt"), ("a/b/", "c.txt"));
    assert_eq!(split("c.txt"), ("", "c.txt"));
}
