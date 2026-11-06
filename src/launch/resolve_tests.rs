//! Making sense of the command line on real repositories: subfolders, worktrees, names with
//! spaces and letters outside ASCII, files that are gone; and paths of other machines.

use super::*;
use crate::launch::args::Command;

/// A repository with a commit touching `sub dir/ünï file.txt`, in a fresh folder.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-launch {name} é-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub dir/deeper")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("sub dir/ünï file.txt"), "a\n").map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["add", "-A"],
        &["commit", "-q", "-m", "one"],
        &["commit", "-q", "--allow-empty", "-m", "two"],
    ] {
        git::run(Some(&dir), args)?;
    }
    // The form tabs use (`/tmp` is a link on macOS).
    Ok(crate::app::strip_verbatim(
        std::fs::canonicalize(&dir).map_err(|e| e.to_string())?,
    ))
}

#[test]
fn a_subfolder_opens_its_repository() -> Result<(), String> {
    let dir = repo("subfolder")?;
    let deeper = dir.join("sub dir/deeper");
    let dot = resolve(&Command::Open(".".into()), &deeper);
    let up = resolve(&Command::Open("..".into()), &deeper);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        dot?,
        Resolved {
            root: dir.clone(),
            then: Then::Nothing
        }
    );
    assert_eq!(up?.root, dir);
    Ok(())
}

#[test]
fn a_file_s_history_and_blame_from_anywhere() -> Result<(), String> {
    let dir = repo("file")?;
    let sub = dir.join("sub dir");
    let from_sub = resolve(&Command::Log("ünï file.txt".into()), &sub);
    let from_elsewhere = resolve(
        &Command::Blame {
            file: sub.join("ünï file.txt"),
            line: Some(3),
        },
        Path::new("/"),
    );
    // Through `./` and `deeper/..`.
    let roundabout = resolve(&Command::Log("./deeper/../ünï file.txt".into()), &sub);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        from_sub?,
        Resolved {
            root: dir.clone(),
            then: Then::History("sub dir/ünï file.txt".into())
        }
    );
    assert_eq!(
        from_elsewhere?.then,
        Then::Blame {
            path: "sub dir/ünï file.txt".into(),
            line: Some(3)
        }
    );
    assert_eq!(
        roundabout?.then,
        Then::History("sub dir/ünï file.txt".into())
    );
    Ok(())
}

#[test]
fn a_deleted_file_still_has_a_history_but_a_folder_is_not_a_file() -> Result<(), String> {
    let dir = repo("deleted")?;
    git::run(Some(&dir), &["rm", "-q", "sub dir/ünï file.txt"])?;
    git::run(Some(&dir), &["commit", "-q", "-m", "gone"])?;
    let gone = resolve(&Command::Log("sub dir/ünï file.txt".into()), &dir);
    let folder = resolve(&Command::Log("sub dir".into()), &dir);
    let nowhere = resolve(&Command::Log("no such/x.txt".into()), &dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(gone?.then, Then::History("sub dir/ünï file.txt".into()));
    assert!(folder.err().is_some_and(|e| e.contains("is a folder")));
    assert!(
        nowhere
            .err()
            .is_some_and(|e| e.starts_with("No such folder"))
    );
    Ok(())
}

#[test]
fn a_linked_worktree_is_its_own_repository() -> Result<(), String> {
    let dir = repo("worktree")?;
    let linked = dir.with_file_name(format!(
        "{}-linked wt",
        dir.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&linked);
    let linked_arg = linked.to_string_lossy().into_owned();
    git::run(
        Some(&dir),
        &["worktree", "add", "-q", "-b", "side", &linked_arg],
    )?;
    let linked =
        crate::app::strip_verbatim(std::fs::canonicalize(&linked).map_err(|e| e.to_string())?);
    let open = resolve(&Command::Open(".".into()), &linked.join("sub dir"));
    let log = resolve(&Command::Log("sub dir/ünï file.txt".into()), &linked);
    let _ = std::fs::remove_dir_all(&linked);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(open?.root, linked);
    assert_eq!(log?.root, linked);
    Ok(())
}

#[test]
fn commits_and_ranges_by_their_full_hash() -> Result<(), String> {
    let dir = repo("revs")?;
    let head = git::run(Some(&dir), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let first = git::run(Some(&dir), &["rev-parse", "HEAD~1"])?
        .trim()
        .to_owned();
    let show = resolve(&Command::Show("HEAD~1".into()), &dir.join("sub dir"));
    let compare = resolve(
        &Command::Compare(Range {
            base: "main~1".into(),
            target: "main".into(),
            since_fork: false,
        }),
        &dir,
    );
    let missing = resolve(&Command::Show("no-such-branch".into()), &dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(show?.then, Then::Commit(first.clone()));
    assert_eq!(
        compare?.then,
        Then::Compare {
            base: first,
            base_label: "main~1".into(),
            target: head,
            target_label: "main".into(),
            since_fork: false
        }
    );
    assert_eq!(
        missing.err().as_deref(),
        Some("no-such-branch is not a commit")
    );
    Ok(())
}

#[test]
fn outside_any_repository() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-launch-bare-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("x.txt"), "").map_err(|e| e.to_string())?;
    // A repository above the temp folder (a developer's home) would make this one inside it.
    let above = git::toplevel(&dir).is_ok();
    let open = resolve(&Command::Open(".".into()), &dir);
    let log = resolve(&Command::Log("x.txt".into()), &dir);
    let _ = std::fs::remove_dir_all(&dir);
    if above {
        return Ok(());
    }
    assert!(
        open.err()
            .is_some_and(|e| e.contains("is not inside a git repository"))
    );
    assert!(
        log.err()
            .is_some_and(|e| e.contains("is not inside a git repository"))
    );
    Ok(())
}

#[test]
fn paths_inside_the_repository() {
    let root = Path::new("/r/my repo");
    assert_eq!(
        relative(root, Path::new("/r/my repo/a/b c.rs")).as_deref(),
        Some("a/b c.rs")
    );
    assert_eq!(relative(root, Path::new("/r/my repo")), None);
    assert_eq!(relative(root, Path::new("/r/other/a.rs")), None);
    assert_eq!(relative(root, Path::new("/r/my repo/../x")), None);
}

#[test]
fn paths_on_other_machines() {
    let ssh_root = Path::new("ssh://build/home/me/app");
    assert_eq!(
        remote_relative(ssh_root, Path::new("ssh://build/home/me/app/src/main.rs")).as_deref(),
        Some("src/main.rs")
    );
    assert_eq!(
        remote_relative(ssh_root, Path::new("ssh://other/home/me/app/src/main.rs")),
        None
    );
    assert_eq!(
        remote_relative(ssh_root, Path::new("ssh://build/home/me/app2/x.rs")),
        None
    );
    // WSL folders are paths of Windows only.
    if !cfg!(windows) {
        return;
    }
    let wsl_root = Path::new(r"\\wsl.localhost\Ubuntu\home\me\repo");
    assert_eq!(
        remote_relative(
            wsl_root,
            Path::new(r"\\wsl.localhost\Ubuntu\home\me\repo\src\a b.rs")
        )
        .as_deref(),
        Some("src/a b.rs")
    );
    // `\\wsl$\…` names the same machine.
    assert_eq!(
        remote_relative(wsl_root, Path::new(r"\\wsl$\Ubuntu\home\me\repo\x.rs")).as_deref(),
        Some("x.rs")
    );
    // Another distribution, or a folder beside the repository that starts the same.
    assert_eq!(
        remote_relative(
            wsl_root,
            Path::new(r"\\wsl.localhost\Debian\home\me\repo\x.rs")
        ),
        None
    );
    assert_eq!(
        remote_relative(
            wsl_root,
            Path::new(r"\\wsl.localhost\Ubuntu\home\me\repo2\x.rs")
        ),
        None
    );
    // A relative path typed in a WSL folder is joined to it, and stays on that machine.
    let joined = absolute(wsl_root, Path::new("src/a.rs"));
    assert!(crate::remote::is_remote(&joined));
    assert_eq!(
        remote_relative(wsl_root, &joined).as_deref(),
        Some("src/a.rs")
    );
}
