//! Case-only renames against real Git, on whatever filesystem the tests run on (Windows and
//! macOS: case-insensitive).

use super::*;

fn paths(repo: &Repo) -> Result<Vec<String>, String> {
    Ok(index(repo)?.into_iter().map(|(_, _, p)| p).collect())
}

#[test]
fn case_only_rename_keeps_staged_and_unstaged_edits_byte_exact() -> Result<(), String> {
    let repo = Repo::new("file-metadata-case")?;
    let (old, new) = ("Docs/Read Më.TXT", "Docs/read më.txt");
    repo.commit("base", &[(old, Some("one\n"))])?;
    std::fs::write(repo.dir.join(old), "one\ntwo\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "--", old])?;
    let staged = blob_of(&repo, old)?;
    let edited = b"one\r\ntwo\nthree \xc3\xa9\n\x00tail";
    std::fs::write(repo.dir.join(old), edited).map_err(|e| e.to_string())?;

    let plan = case_rename_plan(&snapshot(repo.path(), old, true)?, "read më.txt")?;
    assert_eq!(plan.to, new);
    run_all(&repo, &plan.ops)?;

    assert_eq!(paths(&repo)?, [new]);
    assert_eq!(blob_of(&repo, new)?, staged);
    assert_eq!(read(&repo, new)?, edited);
    // The name on disk has the new case, and the temporary name is gone.
    let on_disk: Vec<String> = std::fs::read_dir(repo.dir.join("Docs"))
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(on_disk, ["read më.txt"]);

    run_all(&repo, &plan.undo)?;
    assert_eq!(paths(&repo)?, [old]);
    assert_eq!(read(&repo, old)?, edited);
    assert_eq!(blob_of(&repo, old)?, staged);
    Ok(())
}

#[test]
fn case_only_rename_refuses_collisions_and_other_renames() -> Result<(), String> {
    let repo = Repo::new("file-metadata-case-refused")?;
    let files = [("a/Foo.txt", Some("foo\n")), ("Top.md", Some("t\n"))];
    repo.commit("base", &files)?;
    let blob = blob_of(&repo, "a/Foo.txt")?;
    // Another tracked path that differs only in case (as a repository made elsewhere has).
    let other = [
        "update-index",
        "--add",
        "--cacheinfo",
        "100644",
        &blob,
        "a/FOO.TXT",
    ];
    repo.git(&other)?;
    let snap = snapshot(repo.path(), "a/Foo.txt", true)?;
    let collision = case_rename_plan(&snap, "foo.txt");
    assert!(collision.is_err_and(|e| e.contains("a/FOO.TXT")));
    for refused in ["Foo.txt", "Bar.txt", "b/foo.txt", "a\\foo.txt", ""] {
        assert!(case_rename_plan(&snap, refused).is_err(), "{refused}");
    }

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
    let ignore = format!("{taken}\n");
    std::fs::write(repo.dir.join(".gitignore"), ignore).map_err(|e| e.to_string())?;
    std::fs::write(repo.dir.join(taken.to_uppercase()), "keep").map_err(|e| e.to_string())?;
    let plan = case_rename_plan(&snapshot(repo.path(), "Name.rs", true)?, "name.rs")?;
    let second = format!("{taken}-1");
    assert!(plan.ops[0].contains(&second), "{:?}", plan.ops);
    run_all(&repo, &plan.ops)?;
    assert!(paths(&repo)?.contains(&"name.rs".to_owned()));
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
