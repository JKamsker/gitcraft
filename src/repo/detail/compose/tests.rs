//! The commit box's helpers against real repositories, through the calls the box makes: the
//! template and hook it starts from, the co-author trailer and the authors it offers, the
//! subjects it completes from.

use std::path::{Path, PathBuf};

use super::{join_message, prefill, prefix, split_message};
use crate::git;
#[cfg(unix)]
use crate::git::MessageSource;

/// A fresh repository with one commit, in a folder whose name has a space and an umlaut.
fn repo(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gitcraft compose ü-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Tester"],
        &["config", "user.email", "t@example.org"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "feat(core): base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "{args:?}: {out:?}");
}

#[cfg(unix)]
fn hook(dir: &Path, name: &str, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    let hooks = dir.join(".git").join("hooks");
    let _ = std::fs::create_dir_all(&hooks);
    let file = hooks.join(name);
    assert!(std::fs::write(&file, script).is_ok());
    assert!(std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).is_ok());
}

#[test]
fn no_template_and_no_hook_leave_the_box_empty() {
    let dir = repo("nothing");
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.message, None);
    assert_eq!(found.template, None);
    assert!(found.problems.is_empty(), "{:?}", found.problems);
}

#[test]
fn the_template_is_read_as_git_reads_it() {
    let dir = repo("template");
    // A relative path is from the top of the working tree; comment lines go.
    let text = "feat: \n\n# Why this change?\nRefs: ünïcode\n\n\n";
    assert!(std::fs::write(dir.join("my template.txt"), text).is_ok());
    git_ok(&dir, &["config", "commit.template", "my template.txt"]);
    let found = prefill(&dir, true);
    assert_eq!(found.message.as_deref(), Some("feat:\n\nRefs: ünïcode\n"));
    assert_eq!(found.template, found.message);
    // Another comment character.
    git_ok(&dir, &["config", "core.commentChar", ";"]);
    assert!(std::fs::write(dir.join("my template.txt"), "x\n; gone\n# kept\n").is_ok());
    let found = prefill(&dir, true);
    assert_eq!(found.message.as_deref(), Some("x\n# kept\n"));
    // An absolute path.
    let elsewhere = dir.join("sub dir");
    let _ = std::fs::create_dir_all(&elsewhere);
    let abs = elsewhere.join("t.txt");
    assert!(std::fs::write(&abs, "docs: abs\n").is_ok());
    git_ok(&dir, &["config", "commit.template", &abs.to_string_lossy()]);
    assert_eq!(prefill(&dir, true).message.as_deref(), Some("docs: abs\n"));
    // A template of comments only gives nothing.
    assert!(std::fs::write(&abs, "; only a comment\n\n").is_ok());
    assert_eq!(prefill(&dir, true).message, None);
    // A file that isn't there is said, not in the way.
    git_ok(&dir, &["config", "commit.template", "missing.txt"]);
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.message, None);
    assert!(
        found.problems.iter().any(|p| p.contains("missing.txt")),
        "{:?}",
        found.problems
    );
}

#[test]
fn a_home_template_is_expanded() {
    let dir = repo("home");
    git_ok(
        &dir,
        &[
            "config",
            "commit.template",
            "~/gitcraft-no-such-template.txt",
        ],
    );
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    // git expanded `~` before it was read: the error names the home folder, not `~`.
    let problem = found.problems.join(" ");
    assert!(!problem.contains("~/"), "{problem}");
    assert!(
        problem.contains("gitcraft-no-such-template.txt"),
        "{problem}"
    );
}

#[test]
fn a_huge_template_is_read_whole() {
    let dir = repo("huge");
    let text = format!("chore: big\n\n{}", "line of text ✓\n".repeat(50_000));
    assert!(std::fs::write(dir.join("t.txt"), &text).is_ok());
    git_ok(&dir, &["config", "commit.template", "t.txt"]);
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.message.as_deref(), Some(text.as_str()));
}

#[cfg(unix)]
#[test]
fn the_hook_prefills_with_the_arguments_git_passes() {
    let dir = repo("hook");
    hook(
        &dir,
        "prepare-commit-msg",
        "#!/bin/sh\nprintf 'feat(ü): from hook [%s] [%s]\\n\\n# a comment\\n' \"$2\" \"$3\" > \"$1.new\"\n\
         cat \"$1\" >> \"$1.new\"\nmv \"$1.new\" \"$1\"\n",
    );
    let found = prefill(&dir, true);
    assert_eq!(found.message.as_deref(), Some("feat(ü): from hook [] []\n"));
    // With a template: told so, and given it.
    assert!(std::fs::write(dir.join("t.txt"), "Body from template\n").is_ok());
    git_ok(&dir, &["config", "commit.template", "t.txt"]);
    let found = prefill(&dir, true);
    assert_eq!(
        found.message.as_deref(),
        Some("feat(ü): from hook [template] []\n\nBody from template\n")
    );
    // Amending: the commit whose message is reused.
    let amend = git::prepare_commit_msg(&dir, "x", &MessageSource::Commit("HEAD".into()));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        amend.ok().flatten().as_deref(),
        Some("feat(ü): from hook [commit] [HEAD]\n\nx\n")
    );
}

#[cfg(unix)]
#[test]
fn a_failing_hook_is_said_and_the_template_still_used() {
    let dir = repo("hook-fails");
    hook(
        &dir,
        "prepare-commit-msg",
        "#!/bin/sh\necho 'no ticket in branch name' >&2\nexit 3\n",
    );
    assert!(std::fs::write(dir.join("t.txt"), "fix: \n").is_ok());
    git_ok(&dir, &["config", "commit.template", "t.txt"]);
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.message.as_deref(), Some("fix:\n"));
    let problem = found.problems.join(" ");
    assert!(problem.contains("prepare-commit-msg failed"), "{problem}");
    assert!(problem.contains("no ticket in branch name"), "{problem}");
}

#[cfg(unix)]
#[test]
fn a_hook_that_hangs_is_stopped() {
    let dir = repo("hook-hangs");
    hook(&dir, "prepare-commit-msg", "#!/bin/sh\nsleep 60\n");
    let started = std::time::Instant::now();
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(started.elapsed() < git::HOOK_LIMIT + std::time::Duration::from_secs(5));
    assert_eq!(found.message, None);
    assert!(
        found.problems.iter().any(|p| p.contains("didn't finish")),
        "{:?}",
        found.problems
    );
}

#[cfg(unix)]
#[test]
fn the_hook_is_found_where_core_hooks_path_says() {
    use std::os::unix::fs::PermissionsExt;
    let dir = repo("hooks-path");
    let hooks = dir.join("my hooks");
    let _ = std::fs::create_dir_all(&hooks);
    let file = hooks.join("prepare-commit-msg");
    assert!(std::fs::write(&file, "#!/bin/sh\necho 'chore: hooks path' > \"$1\"\n").is_ok());
    assert!(std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).is_ok());
    git_ok(&dir, &["config", "core.hooksPath", "my hooks"]);
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.message.as_deref(), Some("chore: hooks path\n"));
}

#[test]
fn co_authors_go_into_a_trailer_once() -> Result<(), String> {
    let dir = repo("trailer");
    let ana = "Ana Müller <ana@example.org>";
    let add = |message: &str| git::add_trailer(&dir, message, "Co-authored-by", ana);
    let one = add("feat: x")?;
    assert_eq!(one, format!("feat: x\n\nCo-authored-by: {ana}"));
    // Not twice, and after the trailers already there.
    assert_eq!(add(&one)?, one);
    let with_body = add("fix: y\n\nBody text.\n\nSigned-off-by: T <t@example.org>")?;
    assert_eq!(
        with_body,
        format!("fix: y\n\nBody text.\n\nSigned-off-by: T <t@example.org>\nCo-authored-by: {ana}")
    );
    // A summary that looks like a trailer stays the summary.
    let (summary, body) = split_message(&add("Refs: thing")?);
    assert_eq!(summary, "Refs: thing");
    assert_eq!(body, format!("Co-authored-by: {ana}"));
    // An empty message: only the trailer, in the description.
    let (summary, body) = split_message(&add("")?);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        (summary.as_str(), body.as_str()),
        ("", format!("Co-authored-by: {ana}").as_str())
    );
    Ok(())
}

#[test]
fn the_box_commits_what_the_trailer_wrote() -> Result<(), String> {
    let dir = repo("trailer-commit");
    let message = git::add_trailer(&dir, "feat: pair", "Co-authored-by", "Bo <bo@example.org>")?;
    let (summary, body) = split_message(&message);
    // As the commit box commits: `-m summary -m body`.
    git_ok(
        &dir,
        &["commit", "-q", "--allow-empty", "-m", &summary, "-m", &body],
    );
    let trailers = git::run(
        Some(&dir),
        &[
            "log",
            "-1",
            "--format=%(trailers:key=Co-authored-by,valueonly)",
        ],
    )?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(trailers.trim(), "Bo <bo@example.org>");
    assert_eq!(join_message(&summary, &body), message);
    Ok(())
}

#[test]
fn authors_follow_the_mailmap() -> Result<(), String> {
    let dir = repo("mailmap");
    for (name, email) in [
        ("ana", "ana@old.example"),
        ("Ana M.", "ana@example.org"),
        ("Bob", "bob@example.org"),
        ("Ana M.", "ana@example.org"),
    ] {
        let author = format!("{name} <{email}>");
        git_ok(
            &dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
                "--author",
                &author,
            ],
        );
    }
    assert!(
        std::fs::write(
            dir.join(".mailmap"),
            "Ana Müller <ana@example.org> <ana@old.example>\nAna Müller <ana@example.org>\n"
        )
        .is_ok()
    );
    let authors = git::authors(&dir)?;
    let _ = std::fs::remove_dir_all(&dir);
    let seen: Vec<(String, usize)> = authors.iter().map(|a| (a.ident(), a.commits)).collect();
    assert_eq!(
        seen,
        [
            ("Ana Müller <ana@example.org>".to_owned(), 3),
            ("Bob <bob@example.org>".to_owned(), 1),
            ("Tester <t@example.org>".to_owned(), 1),
        ]
    );
    Ok(())
}

#[test]
fn earlier_prefixes_are_offered() {
    let dir = repo("subjects");
    for subject in ["fix(ui): a", "fix(git): b", "docs: c", "plain"] {
        git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", subject]);
    }
    let (kinds, scopes) = prefix::used_prefixes(&git::recent_subjects(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(kinds, ["fix", "docs", "feat"]);
    // The latest first among those used as often.
    assert_eq!(scopes, ["git", "ui", "core"]);
}

#[test]
fn a_repository_with_no_commit_yet() {
    let dir = std::env::temp_dir().join(format!("gitcraft-compose-unborn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    git_ok(&dir, &["init", "-q"]);
    let subjects = git::recent_subjects(&dir);
    let authors = git::authors(&dir);
    let found = prefill(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(subjects.is_empty());
    assert_eq!(authors, Ok(Vec::new()));
    assert!(found.problems.is_empty(), "{:?}", found.problems);
}

#[test]
fn messages_split_and_join_as_the_box_holds_them() {
    assert_eq!(split_message("a\n\nb\nc\n"), ("a".into(), "b\nc".into()));
    assert_eq!(
        split_message("feat(ui):\n"),
        ("feat(ui): ".into(), String::new())
    );
    // An empty summary stays empty (a message of only a trailer).
    assert_eq!(split_message("\nonly"), (String::new(), "only".into()));
    assert_eq!(split_message(""), (String::new(), String::new()));
    assert_eq!(join_message(" a ", ""), "a");
    assert_eq!(join_message("a", "\nb\n"), "a\n\nb");
}

#[cfg(unix)]
#[test]
fn the_hook_can_be_left_out_of_the_prefill() {
    let dir = repo("hook-off");
    hook(
        &dir,
        "prepare-commit-msg",
        "#!/bin/sh\necho 'from hook' > \"$1\"\n",
    );
    assert!(std::fs::write(dir.join("t.txt"), "from template\n").is_ok());
    git_ok(&dir, &["config", "commit.template", "t.txt"]);
    let found = prefill(&dir, false);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.message.as_deref(), Some("from template\n"));
}

/// git's own COMMIT_EDITMSG (what a failed `git commit` was given) is left alone.
#[cfg(unix)]
#[test]
fn commit_editmsg_is_left_alone() {
    let dir = repo("editmsg");
    hook(
        &dir,
        "prepare-commit-msg",
        "#!/bin/sh\necho 'from hook' > \"$1\"\n",
    );
    let editmsg = dir.join(".git").join("COMMIT_EDITMSG");
    assert!(std::fs::write(&editmsg, "my long lost message\n").is_ok());
    let found = prefill(&dir, true);
    let kept = std::fs::read_to_string(&editmsg).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.message.as_deref(), Some("from hook\n"));
    assert_eq!(kept, "my long lost message\n");
}

#[test]
fn a_template_that_starts_blank_leaves_the_summary_empty() {
    let dir = repo("blank-start");
    assert!(std::fs::write(dir.join("t.txt"), "\n\nRefs: \n# hint\n").is_ok());
    git_ok(&dir, &["config", "commit.template", "t.txt"]);
    let found = prefill(&dir, false);
    let _ = std::fs::remove_dir_all(&dir);
    let (summary, body) = split_message(found.message.as_deref().unwrap_or_default());
    assert_eq!((summary.as_str(), body.as_str()), ("", "Refs:"));
}

/// `#` lines and `---` in a description are text (the box commits with `-m`): the trailer
/// still goes last, where forges read it.
#[test]
fn the_trailer_goes_last_whatever_the_description_holds() -> Result<(), String> {
    let dir = repo("trailer-comments");
    let ana = "Ana <ana@example.org>";
    let out = git::add_trailer(
        &dir,
        "feat: x\n\n#123 fixed\n---\nmore",
        "Co-authored-by",
        ana,
    )?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        out,
        format!("feat: x\n\n#123 fixed\n---\nmore\n\nCo-authored-by: {ana}")
    );
    Ok(())
}

/// A tab whose box the prefill filled with `text` (from a template when `template`).
fn prefilled_tab(dir: &Path, text: &str, template: bool) -> crate::repo::RepoTab {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred(dir.to_path_buf(), &ctx);
    tab.set_message(text);
    tab.compose.prefilled = Some((tab.current_message(), template));
    tab
}

#[test]
fn amending_or_a_merge_replaces_an_unedited_prefill_only() {
    let dir = repo("take-message");
    let mut tab = prefilled_tab(&dir, "feat: \n\nRefs: ", true);
    assert!(tab.unedited_prefill() && tab.unedited_template());
    // The message being amended (or a merge's) takes its place.
    tab.take_message("fix: the real one\n\nbody");
    assert_eq!(tab.commit_summary, "fix: the real one");
    assert!(!tab.unedited_prefill());
    // But not over what was typed.
    let mut typed = prefilled_tab(&dir, "feat: ", true);
    typed.commit_summary.push_str("ünïcode work");
    typed.take_message("Merge branch 'x'");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(typed.commit_summary, "feat: ünïcode work");
    assert!(!typed.unedited_template());
}

#[test]
fn a_replaced_message_can_be_brought_back() {
    let dir = repo("undo-replace");
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred(dir.clone(), &ctx);
    tab.commit_summary = "half written".into();
    tab.commit_body = "with a body".into();
    tab.set_message("chore: from a template");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(tab.commit_summary, "chore: from a template");
    assert_eq!(
        tab.compose.replaced.as_deref(),
        Some("half written\n\nwith a body")
    );
    // A prefill nobody touched isn't worth an Undo.
    let mut fresh = prefilled_tab(&dir, "feat: ", true);
    fresh.set_message("docs: x");
    assert_eq!(fresh.compose.replaced, None);
}
