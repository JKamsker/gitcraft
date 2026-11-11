// Hooks regression coverage.

/// A shell script git runs (a hook, a signing program).
#[cfg(unix)]
fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    assert!(std::fs::write(path, format!("#!/bin/sh\n{body}")).is_ok());
    assert!(std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).is_ok());
}

/// Commits signed by default: the fixup commit and the rewritten ones are signed (here by a
/// stand-in for gpg), and the fold goes through.
#[cfg(unix)]
#[test]
fn signed_commits_stay_signed() {
    let dir = repo("signed", 3);
    let gpg = dir.join(".git").join("fake-gpg");
    script(
        &gpg,
        "cat >/dev/null\nprintf '\\n[GNUPG:] SIG_CREATED D 1 8 00 0 X\\n' >&2\n\
         printf -- '-----BEGIN PGP SIGNATURE-----\\n\\nAA==\\n-----END PGP SIGNATURE-----\\n'\n",
    );
    let gpg = gpg.to_string_lossy().into_owned();
    for args in [
        &["config", "commit.gpgsign", "true"][..],
        &["config", "gpg.program", &gpg],
        &["config", "user.signingkey", "X"],
    ] {
        git_ok(&dir, args);
    }
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);

    done(fold(&mut tab, &ctx, "HEAD~1", false));
    assert_eq!(at(&dir, "HEAD~1", "f2.txt"), "2 fixed\n");
    for rev in ["HEAD", "HEAD~1"] {
        assert!(git_ok(&dir, &["cat-file", "commit", rev]).contains("gpgsig"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Commit hooks run, and one that changes the fixup commit's message (a ticket number in front,
/// a trailer after) doesn't stop it melding in: the fixup commit isn't found by its message.
#[cfg(unix)]
#[test]
fn commit_hooks_dont_break_it() {
    let dir = repo("hooks", 3);
    let hooks = dir.join(".git").join("hooks");
    let _ = std::fs::create_dir_all(&hooks);
    let ran = dir.join(".git").join("pre-commit-ran");
    script(
        &hooks.join("pre-commit"),
        &format!("touch '{}'\n", ran.display()),
    );
    script(
        &hooks.join("prepare-commit-msg"),
        "case \"$(head -n 1 \"$1\")\" in fixup!*) \
         printf '[T-1] %s' \"$(cat \"$1\")\" > \"$1.new\" && mv \"$1.new\" \"$1\";; esac\n",
    );
    script(
        &hooks.join("commit-msg"),
        "printf '\\nChange-Id: I123\\n' >> \"$1\"\n",
    );
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);

    done(fold(&mut tab, &ctx, "HEAD~1", false));
    assert!(ran.exists());
    // The hook did rename the fixup commit before it melded in.
    let reflog = git_ok(&dir, &["log", "-g", "--format=%gs"]);
    assert!(reflog.contains("[T-1] fixup! c2"), "{reflog}");
    assert_eq!(subjects(&dir), ["c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~1", "f2.txt"), "2 fixed\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pre-commit hook that refuses: nothing is committed or rewritten, the change stays
/// staged, and no fold is left waiting to be aborted.
#[cfg(unix)]
#[test]
fn a_refusing_pre_commit_hook_changes_nothing() {
    let dir = repo("hook-refuses", 2);
    let hooks = dir.join(".git").join("hooks");
    let _ = std::fs::create_dir_all(&hooks);
    script(
        &hooks.join("pre-commit"),
        "echo 'lint failed' >&2\nexit 1\n",
    );
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let was_staged = staged(&dir);

    let result = fold(&mut tab, &ctx, "HEAD~1", false);
    assert!(
        matches!(&result, Some(Err(e)) if e.contains("lint failed")),
        "{result:?}"
    );
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    assert!(tab.fold.stopped.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
