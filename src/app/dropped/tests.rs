use super::*;

#[test]
fn dropped_items_are_told_apart() {
    let p = |s: &str| PathBuf::from(s);
    assert_eq!(classify(&p("/x/repo"), true), DropKind::Folder);
    // A folder named like a patch is still a folder.
    assert_eq!(classify(&p("/x/fix.patch"), true), DropKind::Folder);
    assert_eq!(classify(&p("/x/fix.patch"), false), DropKind::Patch);
    assert_eq!(classify(&p("/x/FIX.DIFF"), false), DropKind::Patch);
    assert_eq!(classify(&p("/x/mail.Eml"), false), DropKind::Patch);
    assert_eq!(classify(&p("/x/all.mbox"), false), DropKind::Patch);
    assert_eq!(
        classify(&p("/home/me/my patches/0001-Größe ändern.patch"), false),
        DropKind::Patch
    );
    assert_eq!(classify(&p("/x/notes.txt"), false), DropKind::Other);
    assert_eq!(classify(&p("/x/Makefile"), false), DropKind::Other);
    assert_eq!(classify(&p("/x/.patch"), false), DropKind::Other);
    assert_eq!(classify(&p("/x/fix.patch.txt"), false), DropKind::Other);
    assert_eq!(classify(&p("/x/日本語.diff"), false), DropKind::Patch);
}

#[test]
fn a_folder_outside_a_repository_says_so() {
    let msg = folder_error(
        Path::new("/home/me/My Stuff"),
        "fatal: not a git repository (or any of the parent directories): .git",
    );
    assert_eq!(
        msg,
        "My Stuff isn't in a git repository: drop a repository's folder to open it"
    );
    assert_eq!(
        folder_error(Path::new("/x/y"), "detected dubious ownership"),
        "Could not open y: detected dubious ownership"
    );
}

/// One on another machine is told by its name, without asking that machine.
#[test]
fn remote_drops_are_told_by_their_name() {
    assert_eq!(
        kind_of(Path::new("ssh://box/home/me/fix.PATCH")),
        DropKind::Patch
    );
    assert_eq!(
        kind_of(Path::new("ssh://box/home/me/repo")),
        DropKind::Folder
    );
    #[cfg(windows)]
    assert_eq!(
        kind_of(Path::new(r"\\wsl.localhost\Ubuntu\home\me\repo")),
        DropKind::Folder
    );
}

/// The message a dropped folder outside any repository gets, through what opening it
/// really says.
#[test]
fn a_folder_outside_a_repository_says_so_through_repo_root() {
    let dir = std::env::temp_dir().join(format!("gitcraft-drop plain Ü {}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let err = repo_root(&dir).err().unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let name = name_of(&dir);
    assert_eq!(
        folder_error(&dir, &err),
        format!("{name} isn't in a git repository: drop a repository's folder to open it"),
        "{err}"
    );
}

fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-dropq-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(crate::git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join("f.txt"), "one\n");
    assert!(crate::git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(crate::git::run(Some(&dir), &["commit", "-q", "-m", "one"]).is_ok());
    dir
}

/// Runs the queue until it is empty (or it took too long).
fn drain(queue: &mut PatchQueue, tabs: &mut [Tab], ctx: &egui::Context) -> Vec<String> {
    let mut said = Vec::new();
    for _ in 0..400 {
        for tab in tabs.iter_mut() {
            if let Tab::Repo(r) = tab {
                r.poll(ctx);
            }
        }
        said.extend(queue.advance(tabs, ctx));
        if queue.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    said
}

/// Patches go to the tab they were dropped on, whichever tab is shown meanwhile; one that
/// fails leaves out the ones dropped after it on the same tab, by its own outcome (not the
/// tab's status line); a closed tab's patch says so.
#[test]
fn dropped_patches_go_to_their_own_tab_and_stop_after_a_failure() {
    let ctx = egui::Context::default();
    let a = repo("a");
    let b = repo("b");
    let patches = std::env::temp_dir().join(format!("gitcraft-dropq-p-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&patches);
    let good = patches.join("good.patch");
    let _ = std::fs::write(
        &good,
        "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\n one\n+two\n",
    );
    let bad = patches.join("bad.diff");
    let _ = std::fs::write(
        &bad,
        "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-nothing\n+else\n",
    );
    let mut tabs = vec![
        Tab::Repo(Box::new(RepoTab::new_deferred(a.clone(), &ctx))),
        Tab::Repo(Box::new(RepoTab::new_deferred(b.clone(), &ctx))),
    ];
    let mut queue = PatchQueue::default();
    // Dropped on A: good, bad, then good again (left out after bad fails).
    queue.push(good.clone(), a.clone());
    queue.push(bad.clone(), a.clone());
    queue.push(good.clone(), a.clone());
    let said = drain(&mut queue, &mut tabs, &ctx);
    // Git on Windows may check files out with CRLF (core.autocrlf): compare the lines.
    let read = |p: PathBuf| {
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .replace("\r\n", "\n")
    };
    let in_a = read(a.join("f.txt"));
    let in_b = read(b.join("f.txt"));
    // Dropped on B, whose tab is then closed.
    queue.push(good, b.clone());
    tabs.pop();
    let closed_said = drain(&mut queue, &mut tabs, &ctx);
    for dir in [&a, &b, &patches] {
        let _ = std::fs::remove_dir_all(dir);
    }
    assert_eq!(in_a, "one\ntwo\n");
    assert_eq!(in_b, "one\n");
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].starts_with("bad.diff failed:"), "{said:?}");
    assert!(said[0].ends_with("(so the 1 patch dropped after it was not applied)"));
    assert_eq!(
        closed_said,
        vec!["good.patch wasn't applied: its repository's tab was closed".to_owned()]
    );
}
