use std::process::Command;

use super::config::{entry, global_files_from};
use super::runtime::{profile_file, write_file};
use super::*;
use std::ffi::OsString;
use std::path::PathBuf;

#[test]
fn global_files_are_found_as_git_finds_them() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        }
    };
    assert_eq!(
        global_files_from(env(&[("HOME", "/home/me")])),
        [
            PathBuf::from("/home/me/.config/git/config"),
            PathBuf::from("/home/me/.gitconfig")
        ]
    );
    assert_eq!(
        global_files_from(env(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "/x")])),
        [
            PathBuf::from("/x/git/config"),
            PathBuf::from("/h/.gitconfig")
        ]
    );
    assert_eq!(
        global_files_from(env(&[("HOME", ""), ("USERPROFILE", "C:/Users/me")])),
        [
            PathBuf::from("C:/Users/me/.config/git/config"),
            PathBuf::from("C:/Users/me/.gitconfig")
        ]
    );
    // Started with one of its own: only that one, as git would.
    assert_eq!(
        global_files_from(env(&[("HOME", "/h"), ("GIT_CONFIG_GLOBAL", "/etc/mine")])),
        [PathBuf::from("/etc/mine")]
    );
}

#[test]
fn initials_and_titles() {
    let named = |name: &str| UserProfile {
        name: name.to_owned(),
        ..UserProfile::default()
    };
    assert_eq!(named("Work").initials(), "W");
    assert_eq!(named("open source stuff").initials(), "OS");
    assert_eq!(named("  ").initials(), "UP");
    assert_eq!(named("  ").title(), "Unnamed profile");
    assert_eq!(named("(x) ümlaut").initials(), "XÜ");
}

#[test]
fn a_profile_sets_only_what_it_has() {
    let plain = UserProfile::default();
    assert!(!plain.sets_git());
    let text = config_text(&plain, &[]);
    assert!(
        !text.contains("[user]") && !text.contains("[include]"),
        "{text}"
    );

    let work = UserProfile {
        name: "Work \"A\"".to_owned(),
        author_name: " Ann O'Neil ".to_owned(),
        author_email: "ann@corp.example".to_owned(),
        own_signing: true,
        signing: Signing {
            format: "ssh".to_owned(),
            key: "C:\\keys\\id.pub".to_owned(),
            program: String::new(),
            sign_commits: true,
            sign_tags: false,
        },
        ..UserProfile::default()
    };
    assert!(work.sets_git());
    let text = config_text(&work, &[PathBuf::from("C:\\Users\\ann\\.gitconfig")]);
    assert!(
        text.contains("\tpath = \"C:\\\\Users\\\\ann\\\\.gitconfig\"\n"),
        "{text}"
    );
    assert!(text.contains("[user]\n\tname = \"Ann O'Neil\"\n"), "{text}");
    assert!(
        text.contains("\tsigningkey = \"C:\\\\keys\\\\id.pub\"\n"),
        "{text}"
    );
    assert!(text.contains("[gpg]\n\tformat = \"ssh\"\n"), "{text}");
    assert!(text.contains("[commit]\n\tgpgSign = \"true\"\n"), "{text}");
    assert!(text.contains("[tag]\n\tgpgSign = \"false\"\n"), "{text}");
    // No program: whatever the user's own config says.
    assert!(!text.contains("program"), "{text}");
    assert!(text.starts_with("# Written by gitcraft for its profile \"Work \\\"A\\\"\""));
    assert_eq!(
        entry("gpg.ssh.program", "/usr/bin/ssh-keygen"),
        "[gpg \"ssh\"]\n\tprogram = \"/usr/bin/ssh-keygen\"\n"
    );
}

#[test]
fn git_versions_that_read_profiles() {
    assert!(git_reads_profiles("git version 2.43.0"));
    assert!(git_reads_profiles("git version 2.32.0.windows.1"));
    assert!(git_reads_profiles("git version 3.0"));
    assert!(!git_reads_profiles("git version 2.31.8"));
    assert!(!git_reads_profiles("git version 1.9.5.msysgit.0"));
    assert!(git_reads_profiles("something else"));
}

/// The profile wins over the user's own global config, which it includes, and a
/// repository's own config wins over the profile. Against scratch files, never the user's.
#[test]
fn a_repository_wins_over_the_profile_which_wins_over_the_global_config() -> Result<(), String> {
    let text = |e: std::io::Error| e.to_string();
    let dir = std::env::temp_dir().join(format!("gitcraft-profile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).map_err(text)?;
    let own = dir.join("own.gitconfig");
    std::fs::write(
        &own,
        "[user]\n\tname = Own Name\n\temail = own@example.com\n[core]\n\tfrom-own = yes\n",
    )
    .map_err(text)?;
    let profile = UserProfile {
        name: "Work".to_owned(),
        author_email: "work@example.com".to_owned(),
        ..UserProfile::default()
    };
    let file = profile_file(&dir, profile.id);
    let missing = dir.join("not-there");
    write_file(&file, &config_text(&profile, &[missing, own.clone()]))?;
    let git = |args: &[&str]| -> Result<String, String> {
        let out = Command::new("git")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &file)
            .current_dir(&repo)
            .args(args)
            .output()
            .map_err(text)?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
        }
    };
    let result = (|| {
        git(&["init", "-q"])?;
        let before = (
            git(&["config", "user.name"])?,
            git(&["config", "user.email"])?,
            git(&["config", "core.from-own"])?,
        );
        git(&["config", "--local", "user.email", "repo@example.com"])?;
        let after = git(&["config", "user.email"])?;
        Ok::<_, String>((before, after))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let (before, after) = result?;
    // The name the profile leaves alone, and the rest of the user's own config, stay.
    assert_eq!(
        before,
        (
            "Own Name".to_owned(),
            "work@example.com".to_owned(),
            "yes".to_owned()
        )
    );
    assert_eq!(after, "repo@example.com");
    Ok(())
}
