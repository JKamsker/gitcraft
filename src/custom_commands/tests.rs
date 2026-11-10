//! Custom commands: names, the repository's file and its trust, what menus offer, and what the
//! commands get, run for real through git's shell in temporary repositories.

use std::path::{Path, PathBuf};

use super::*;
use crate::git;

include!("tests/execution.rs");

#[test]
fn prompt_variables_must_be_plain_and_not_taken() {
    for ok in ["MESSAGE", "TAG_NAME", "X1", "A"] {
        assert_eq!(variable_error(ok), None, "{ok}");
    }
    for bad in [
        "",
        "message",
        "1X",
        "_X",
        "MY-VAR",
        "A B",
        "ÜBER",
        "SHA",
        "FILE",
        "REPO",
        "PATH",
        "IFS",
        "HOME",
        "GIT_DIR",
        "LD_PRELOAD",
        "GITCRAFT_COMMAND",
        "LC_ALL",
    ] {
        assert!(variable_error(bad).is_some(), "{bad}");
    }
    assert!(variable_error(&"A".repeat(33)).is_some());
}

#[test]
fn a_command_with_a_problem_says_what_it_is() {
    assert_eq!(command("X", "echo").problem(), None);
    assert!(command(" ", "echo").problem().is_some());
    assert!(command("X", "  \n").problem().is_some());
    assert!(command("X", "echo \0").problem().is_some());
    let mut twice = command("X", "echo");
    twice.prompts = vec![
        Prompt {
            variable: "A".to_owned(),
            ..Prompt::default()
        };
        2
    ];
    assert_eq!(twice.problem().as_deref(), Some("$A is asked for twice"));
}

#[test]
fn menus_offer_only_their_own_commands_that_can_run() {
    let mut on_file = command("On file", "echo");
    on_file.placement = Placement::File;
    let broken = command("", "echo");
    let mut palette = command("P", "echo");
    palette.placement = Placement::Palette;
    let global = vec![command("On commit", "echo"), on_file, broken, palette];
    let mut from_repo = command("Repo's", "echo");
    from_repo.placement = Placement::File;
    let repo = vec![from_repo];
    let names = |p: Placement| -> Vec<(String, Source)> {
        offered_at(p, &global, &repo)
            .map(|o| (o.command.name, o.source))
            .collect()
    };
    assert_eq!(
        names(Placement::Commit),
        [("On commit".to_owned(), Source::Global)]
    );
    assert_eq!(
        names(Placement::File),
        [
            ("On file".to_owned(), Source::Global),
            ("Repo's".to_owned(), Source::Repository)
        ]
    );
    assert!(names(Placement::Branch).is_empty());
    assert_eq!(names(Placement::Palette).len(), 1);
}

#[test]
fn an_entry_says_when_it_asks_first() {
    let mut c = command(" Tag ", "echo");
    c.confirm = false;
    assert_eq!(c.entry_label(), "Tag");
    c.confirm = true;
    assert_eq!(c.entry_label(), "Tag…");
    c.confirm = false;
    c.prompts.push(Prompt {
        variable: "X".to_owned(),
        ..Prompt::default()
    });
    assert_eq!(c.entry_label(), "Tag…");
}

const REPO_JSON: &str = r#"{
  "commands": [
    {
      "name": "Open on CI",
      "where": "commit",
      "command": "xdg-open \"https://ci.example/$SHA\"",
      "output": "toast",
      "confirm": false
    },
    {
      "name": "Tag",
      "where": "branch",
      "command": "git tag \"$NAME\" \"$SHA\"",
      "prompts": [{ "variable": "NAME", "label": "Tag name", "kind": "text" },
                  { "variable": "ONTO", "kind": "branch", "default": "main" }],
      "output": "terminal"
    }
  ]
}"#;

#[test]
fn a_repository_file_is_read() -> Result<(), String> {
    let list = parse_repo_file(REPO_JSON)?;
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].placement, Placement::Commit);
    assert!(!list[0].confirm);
    assert_eq!(list[1].output, OutputTo::Terminal);
    // Not said: asked first.
    assert!(list[1].confirm);
    assert_eq!(list[1].prompts[1].kind, PromptKind::Branch);
    assert_eq!(list[1].prompts[1].default, "main");
    Ok(())
}

#[test]
fn a_bad_repository_file_says_what_is_wrong() {
    let cases = [
        ("", "can't be read"),
        ("[]", "can't be read"),
        (r#"{"commands": [], "extra": 1}"#, "unknown field"),
        (
            r#"{"commands": [{"name": "X", "where": "nowhere", "command": "x"}]}"#,
            "unknown variant",
        ),
        (
            r#"{"commands": [{"name": "", "command": "x"}]}"#,
            "Command 1: It has no name",
        ),
        (
            r#"{"commands": [{"name": "X", "command": "x", "prompts": [{"variable": "PATH"}]}]}"#,
            "“X”: $PATH is used by the shell",
        ),
    ];
    for (text, said) in cases {
        let e = parse_repo_file(text).err().unwrap_or_default();
        assert!(e.contains(said), "{text}: {e}");
    }
    let huge = format!(
        r#"{{"commands": [], "x": "{}"}}"#,
        "a".repeat(REPO_FILE_MAX)
    );
    assert!(parse_repo_file(&huge).is_err_and(|e| e.contains("larger than")));
    assert_eq!(parse_repo_file(r#"{"commands": []}"#), Ok(Vec::new()));
}

#[test]
fn trust_is_for_one_repository_and_its_exact_file() {
    let mut trusted = Vec::new();
    let file = digest(REPO_JSON);
    assert!(!is_trusted(&trusted, "/r", &file));
    trust(&mut trusted, "/r", &file);
    assert!(is_trusted(&trusted, "/r", &file));
    // Another repository with the same file isn't trusted.
    assert!(!is_trusted(&trusted, "/other", &file));
    // The file changed: asked again, even for a space.
    let changed = digest(&REPO_JSON.replace("ci.example", "evil.example"));
    assert!(!is_trusted(&trusted, "/r", &changed));
    assert!(!is_trusted(
        &trusted,
        "/r",
        &digest(&format!("{REPO_JSON} "))
    ));
    // Trusting the new one replaces the old one.
    trust(&mut trusted, "/r", &changed);
    assert_eq!(trusted.len(), 1);
    assert!(is_trusted(&trusted, "/r", &changed));
    assert!(!is_trusted(&trusted, "/r", &file));
}

#[test]
fn a_digest_is_sha256_of_the_text() {
    assert_eq!(
        digest(""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        digest("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_ne!(digest("ü"), digest("u"));
}

#[test]
fn a_toast_shows_the_end_of_the_output_without_colours() {
    let out = b"\x1b[32mgreen\x1b[0m done\nprogress 10%\rprogress 100%\r\n\n";
    assert_eq!(
        toast_text("Build", 0, out, b""),
        (
            "Build finished\ngreen done\nprogress 100%".to_owned(),
            false
        )
    );
    assert_eq!(
        toast_text("Quiet", 0, b"", b"  \n"),
        ("Quiet finished".to_owned(), false)
    );
    let long = "é".repeat(2000);
    let (text, _) = toast_text("Long", 0, long.as_bytes(), b"");
    assert!(text.chars().count() < 700);
    assert!(text.contains('…'));
    // Not UTF-8: shown as well as it can be.
    let (text, failed) = toast_text("Bytes", 1, &[0xff, b'a'], b"");
    assert!(failed);
    assert!(text.ends_with("\u{fffd}a"));
}

#[test]
fn commands_round_trip_through_the_settings() -> Result<(), String> {
    let mut c = command("Name", "echo \"$SHA\"");
    c.placement = Placement::Palette;
    c.output = OutputTo::Terminal;
    c.prompts.push(Prompt {
        variable: "X".to_owned(),
        label: "Ask".to_owned(),
        kind: PromptKind::Branch,
        default: "main".to_owned(),
    });
    let settings = crate::settings::Settings {
        custom_commands: vec![c.clone()],
        trusted_command_files: vec![TrustedFile {
            repo: "/r".to_owned(),
            digest: digest(REPO_JSON),
        }],
        ..Default::default()
    };
    let text = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    let back: crate::settings::Settings = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.custom_commands, vec![c]);
    assert_eq!(back.trusted_command_files, settings.trusted_command_files);
    Ok(())
}

#[test]
fn german_validation_display_keeps_raw_errors_and_eligibility_stable() -> Result<(), String> {
    crate::i18n::set_language("de");
    let mut c = command("X", "echo \"$A\"");
    c.prompts = vec![
        Prompt {
            variable: "A".into(),
            ..Prompt::default()
        };
        2
    ];
    let raw = c.problem().ok_or("missing test value")?;
    assert_eq!(raw, "$A is asked for twice");
    assert_eq!(display_problem(&raw), "$A wird zweimal abgefragt");
    assert_eq!(c.problem().as_deref(), Some("$A is asked for twice"));
    assert!(offered_at(Placement::Commit, &[c], &[]).next().is_none());
    for (variable, expected) in [
        ("SHA", "$SHA wird bereits von gitcraft gesetzt"),
        (
            "PATH",
            "$PATH wird von der Shell oder Git verwendet; wähle einen anderen Namen",
        ),
        (
            "bad name",
            "„bad name“ ist kein Variablenname: Großbuchstaben, Ziffern und _, beginnend mit einem Buchstaben",
        ),
    ] {
        let raw = variable_error(variable).ok_or("missing test value")?;
        assert_eq!(display_problem(&raw), expected);
        assert_eq!(variable_error(variable).as_deref(), Some(raw.as_str()));
    }
    assert_eq!(
        display_problem("unknown provider failure 日本語"),
        "unknown provider failure 日本語"
    );
    assert_eq!(BUILT_IN[1], ("SHORT_SHA", "its first 8 characters"));
    assert_eq!(
        variable_description(BUILT_IN[1].0, BUILT_IN[1].1),
        "die ersten 8 Zeichen davon"
    );
    crate::i18n::set_language("en-US");
    assert_eq!(display_problem(&raw), raw);

    Ok(())
}
