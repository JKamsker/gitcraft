use super::*;
use crate::forge::ToolReport;

#[test]
fn the_report_says_what_it_should_and_nothing_personal() {
    let facts = Facts {
        channel: "dev".into(),
        adapter: Some("llvmpipe (Vulkan)".into()),
        repo: None,
    };
    let all = config::parse_origins(
        "global\0file:/home/zoë/.gitconfig\0user.email\nzoe@example.com\0\
             global\0file:/home/zoë/.gitconfig\0user.name\nZoë\0\
             global\0file:/home/zoë/.gitconfig\0pull.rebase\ntrue\0\
             local\0file:.git/config\0pull.rebase\nfalse\0\
             local\0file:.git/config\0credential.helper\nstore\0\
             global\0file:/home/zoë/.gitconfig\0core.editor\n/home/zoë/bin/ed\0",
    );
    let rows = settings_rows(&all);
    assert_eq!(
        rows,
        [
            "pull.rebase = false · Repository · config",
            "core.editor = (configured; value withheld) · Global · .gitconfig"
        ]
    );
    let tools = [
        ToolReport {
            tool: "gh",
            version: Some("gh version 2.89.0".into()),
            signed_in: Some(true),
        },
        ToolReport {
            tool: "glab",
            version: Some("glab 1.50".into()),
            signed_in: Some(false),
        },
        ToolReport {
            tool: "tea",
            version: None,
            signed_in: None,
        },
    ];
    let settings = Ok(vec![
        "pull.rebase = true · Global · .gitconfig".to_owned(),
        r"# a WSL path /mnt/c/Users/Ana/x and C:\Users\Ana\y".to_owned(),
    ]);
    let text = report(
        &facts,
        "git version 2.43.0",
        &tools,
        Some(&settings),
        "/home/zoë",
    );
    assert!(text.contains("dev channel"));
    assert!(text.contains("llvmpipe (Vulkan)"));
    assert!(text.contains("gh version 2.89.0, signed in"));
    assert!(text.contains("glab 1.50, auth status failed"));
    assert!(text.contains("**tea:** not found"));
    assert!(text.contains("pull.rebase = true · Global · .gitconfig"));
    assert!(text.contains("a WSL path"), "{text}");
    assert!(!text.contains("zoë") && !text.contains("example.com") && !text.contains("Ana"));
    let none = report(&facts, "git", &[], None, "");
    assert!(none.contains("No repository open"));
    let failed = report(
        &facts,
        "git",
        &[],
        Some(&Err("ssh://bob@x/home/bob".into())),
        "",
    );
    assert!(failed.contains("could not be read.") && !failed.contains("bob"));
}

#[test]
fn home_and_users_are_redacted() {
    assert_eq!(
        redact_home(r"C:\Users\ana\x and c:/users/ana/y", r"C:\Users\ana"),
        "~\\x and ~/y"
    );
    // Only a whole folder: /home/al isn't the start of /home/alice.
    assert_eq!(
        redact_home("/home/alice/x /rootless", "/home/al"),
        "/home/alice/x /rootless"
    );
    assert_eq!(redact_home("/home/al/x", "/home/al"), "~/x");
    // Too short to be a home folder: nothing replaced.
    assert_eq!(redact_home("/a/b", "/"), "/a/b");
    // Any user's folder, here or on another machine, in every form.
    assert_eq!(
        redact_users(
            "/home/bob/.gitconfig /mnt/c/Users/Ana/gpg.exe /c/Users/ana C:/Users/x/y /root/z"
        ),
        "~/.gitconfig ~/gpg.exe ~ ~/y ~/z"
    );
    assert_eq!(redact_users("no paths /rootless"), "no paths /rootless");
    assert_eq!(
        pretty_name("NAME=x\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n"),
        Some("Ubuntu 24.04 LTS".into())
    );
    assert_eq!(pretty_name(""), None);
}

/// Against the real git and tools of this machine: collected without a panic, and the
/// repository's settings read.
#[test]
fn collecting_works_here() -> Result<(), String> {
    let dir = crate::git::doctor::tests::repo("diagnostics", &[])?;
    crate::git::doctor::tests::git(&dir, &["config", "pull.ff", "only"])?;
    let facts = Facts {
        channel: "main".into(),
        adapter: None,
        repo: Some(dir.clone()),
    };
    let text = collect(&facts);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(text.contains("**git:** git version"), "{text}");
    assert!(
        text.contains("pull.ff = only · Repository · config"),
        "{text}"
    );
    assert!(text.contains("repository on this machine"));
    Ok(())
}
