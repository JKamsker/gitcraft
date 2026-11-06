//! Custom commands: names, the repository's file and its trust, what menus offer, and what the
//! commands get, run for real through git's shell in temporary repositories.

use std::path::{Path, PathBuf};

use super::*;
use crate::git;

/// A repository with one commit, in a fresh folder named after `name` (with a space, a quote
/// and a non-ASCII letter in its path, which the commands must survive too).
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft cc '{name}' ü-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

fn command(name: &str, line: &str) -> CustomCommand {
    CustomCommand {
        name: name.to_owned(),
        command: line.to_owned(),
        ..CustomCommand::default()
    }
}

fn scope(sha: &str, branch: &str, file: &str) -> Scope {
    Scope {
        sha: sha.to_owned(),
        branch: branch.to_owned(),
        remote: "origin".to_owned(),
        file: file.to_owned(),
    }
}

/// Runs `line` in `dir` as a toast command would be, with these variables.
fn run(dir: &Path, vars: &[(String, String)], line: &str) -> Result<git::ShellRun, String> {
    git::run_shell(dir, &captured_program(vars, line))
}

/// Values that would do something else if pasted into a command line.
const NASTY: [&str; 10] = [
    "plain",
    "with spaces  and\ttabs",
    "it's \"quoted\"",
    "a; touch PWNED",
    "$(touch PWNED)",
    "`touch PWNED`",
    "line one\nline two\n",
    "ünïcødé ✓ 日本語",
    "",
    "--option -x * ?",
];

#[test]
fn every_value_reaches_the_command_as_it_is() -> Result<(), String> {
    let dir = repo("values");
    for value in NASTY {
        for name in ["SHA", "BRANCH", "FILE", "REMOTE"] {
            let mut s = scope("", "", "");
            match name {
                "SHA" => s.sha = value.to_owned(),
                "BRANCH" => s.branch = value.to_owned(),
                "FILE" => s.file = value.to_owned(),
                _ => s.remote = value.to_owned(),
            }
            let vars = variables(&s, "/r", &[]);
            let ran = run(&dir, &vars, &format!("printf '%s' \"${name}\""))?;
            assert_eq!(ran.code, 0, "{}", String::from_utf8_lossy(&ran.stderr));
            assert_eq!(String::from_utf8_lossy(&ran.stdout), value, "${name}");
        }
    }
    let pwned = dir.join("PWNED").exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!pwned, "a value was run as a command");
    Ok(())
}

#[test]
fn a_prompt_answer_is_one_word_too() -> Result<(), String> {
    let dir = repo("answers");
    let answers = vec![("MESSAGE".to_owned(), "x; touch PWNED $(id)".to_owned())];
    let vars = variables(&scope("abc", "main", ""), "/r", &answers);
    // Even unquoted, a value is only split, never run.
    let ran = run(
        &dir,
        &vars,
        "printf '[%s]' \"$MESSAGE\"; printf '<%s>' $MESSAGE",
    )?;
    let pwned = dir.join("PWNED").exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[x; touch PWNED $(id)]<x;><touch><PWNED><$(id)>"
    );
    assert!(!pwned);
    Ok(())
}

#[test]
fn the_commands_variables_are_set_from_the_scope() {
    let vars = variables(
        &scope("0123456789abcdef", "feat/ü", "a b.txt"),
        "/home/me/r",
        &[],
    );
    let get = |n: &str| vars.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
    assert_eq!(get("SHA"), Some("0123456789abcdef"));
    assert_eq!(get("SHORT_SHA"), Some("01234567"));
    assert_eq!(get("BRANCH"), Some("feat/ü"));
    assert_eq!(get("REMOTE"), Some("origin"));
    assert_eq!(get("FILE"), Some("a b.txt"));
    assert_eq!(get("REPO"), Some("/home/me/r"));
    // A hash shorter than a short one stays whole; none gives none.
    let vars = variables(&scope("abc", "", ""), "", &[]);
    assert!(vars.contains(&("SHORT_SHA".to_owned(), "abc".to_owned())));
    let vars = variables(&Scope::default(), "", &[]);
    assert!(vars.contains(&("SHORT_SHA".to_owned(), String::new())));
}

#[test]
fn a_command_runs_in_the_repository_and_sees_its_path() -> Result<(), String> {
    let dir = repo("where");
    let there = dir.join("sub dir");
    let _ = std::fs::create_dir_all(&there);
    let head = git::head(&dir);
    let vars = variables(&scope(&head, "", ""), &dir.display().to_string(), &[]);
    let ran = run(
        &dir,
        &vars,
        "git rev-parse HEAD; test \"$(pwd -P)\" = \"$(cd \"$REPO\" && pwd -P)\" && echo here",
    )?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.code, 0, "{}", String::from_utf8_lossy(&ran.stderr));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        format!("{head}\nhere\n")
    );
    Ok(())
}

#[test]
fn a_huge_value_and_a_huge_output_get_through() -> Result<(), String> {
    let dir = repo("huge");
    let big = "ü".repeat(50_000);
    let vars = variables(&scope("", "", &big), "/r", &[]);
    let ran = run(
        &dir,
        &vars,
        "printf '%s' \"$FILE\"; i=0; while [ $i -lt 3000 ]; do echo line $i; i=$((i+1)); done",
    )?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.code, 0);
    let out = String::from_utf8_lossy(&ran.stdout);
    assert!(out.starts_with(&big));
    let (text, failed) = toast_text("Big", ran.code, &ran.stdout, &ran.stderr);
    assert!(!failed);
    assert!(text.ends_with("line 2999"), "{text}");
    assert!(text.lines().count() <= 9, "{text}");
    Ok(())
}

#[test]
fn a_failing_command_says_so_with_its_error() -> Result<(), String> {
    let dir = repo("fails");
    let vars = variables(&Scope::default(), "/r", &[]);
    let ran = run(
        &dir,
        &vars,
        "echo working; echo 'no such thing' >&2; exit 3",
    )?;
    let (text, failed) = toast_text("Deploy", ran.code, &ran.stdout, &ran.stderr);
    assert_eq!(ran.code, 3);
    assert!(failed);
    assert_eq!(
        text,
        "Deploy failed (exit status 3)\nworking\nno such thing"
    );
    // A program that isn't there is a failure too, with the shell's message.
    let ran = run(&dir, &vars, "gitcraft-no-such-program-x")?;
    let (text, failed) = toast_text("Missing", ran.code, &ran.stdout, &ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.code, 127);
    assert!(failed);
    assert!(text.contains("not found"), "{text}");
    Ok(())
}

#[test]
fn an_exit_in_the_command_ends_only_the_command() -> Result<(), String> {
    let dir = repo("exit");
    let vars = variables(&Scope::default(), "/r", &[]);
    let script = terminal_program(&vars, "echo hi; exit 4", "Name 'q'");
    // Enter, as the terminal would get it.
    let ran = run_with_input(&dir, &script, "\n")?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.0, Some(4));
    assert_eq!(
        ran.1,
        "hi\n\nName 'q': exit status 4. Press Enter to close.\n"
    );
    Ok(())
}

/// Runs `script` the way the terminal does for a repository here, `input` on its input:
/// (exit code, output).
fn run_with_input(dir: &Path, script: &str, input: &str) -> Result<(Option<i32>, String), String> {
    use std::io::Write;
    let (program, args) = git::shell_command_here(dir, script);
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    Ok((
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

#[test]
fn a_terminal_command_sees_the_same_values() -> Result<(), String> {
    let dir = repo("terminal");
    for value in NASTY {
        let vars = variables(&scope("", value, ""), "/r", &[]);
        let script = terminal_program(&vars, "printf '%s|' \"$BRANCH\"", "N");
        let (code, out) = run_with_input(&dir, &script, "\n")?;
        assert_eq!(code, Some(0));
        assert!(out.starts_with(&format!("{value}|")), "{out:?}");
    }
    let pwned = dir.join("PWNED").exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!pwned);
    Ok(())
}

/// What ssh is told to run reaches the folder and the script whatever is in them: run here with
/// `sh` standing in for the login shell on the other side.
#[cfg(unix)]
#[test]
fn the_remote_terminal_runs_the_script_in_the_folder() -> Result<(), String> {
    let dir = repo("remote-term");
    let folder = dir.join("it's a \"dir\" $(x) ü");
    let _ = std::fs::create_dir_all(&folder);
    let target = crate::remote::Target {
        host: crate::remote::Host::Ssh("host".to_owned()),
        path: folder.display().to_string(),
    };
    let vars = variables(&scope("", "a'b $(touch PWNED)", ""), "/r", &[]);
    let script = captured_program(&vars, "printf '%s|' \"$BRANCH\"; pwd");
    let (program, args, _) = crate::remote::terminal_running(&target, &script);
    assert_eq!(program, "ssh");
    assert_eq!(&args[..3], ["-t", "--", "host"]);
    let remote_line = args.get(3).cloned().unwrap_or_default();
    // Nothing csh or fish would read differently, as for the plain terminal.
    let quoted = remote_line.split('\'').nth(1).unwrap_or_default();
    assert!(!quoted.contains(['\\', '!']), "{quoted}");
    let out = std::process::Command::new("sh")
        .args(["-c", &remote_line])
        .current_dir(std::env::temp_dir())
        .output();
    let pwned = dir.join("PWNED").exists() || folder.join("PWNED").exists();
    let _ = std::fs::remove_dir_all(&dir);
    let out = String::from_utf8_lossy(&out.map_err(|e| e.to_string())?.stdout).into_owned();
    assert_eq!(out, format!("a'b $(touch PWNED)|{}\n", folder.display()));
    assert!(!pwned);
    Ok(())
}

#[test]
fn a_wsl_terminal_gets_the_folder_and_script_as_plain_words() {
    let target = crate::remote::Target {
        host: crate::remote::Host::Wsl("Ubuntu".to_owned()),
        path: "/home/me/r".to_owned(),
    };
    let (program, args, label) = crate::remote::terminal_running(&target, "echo 'x'");
    assert_eq!((program.as_str(), label.as_str()), ("wsl.exe", "Ubuntu"));
    assert_eq!(&args[..5], ["-d", "Ubuntu", "--exec", "sh", "-c"]);
    let b64 = |w: &str| {
        w.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
    };
    assert!(args[7..].iter().all(|w| b64(w)), "{args:?}");
}

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
    assert!(!is_trusted(&trusted, "/r", REPO_JSON));
    trust(&mut trusted, "/r", REPO_JSON);
    assert!(is_trusted(&trusted, "/r", REPO_JSON));
    // Another repository with the same file isn't trusted.
    assert!(!is_trusted(&trusted, "/other", REPO_JSON));
    // The file changed: asked again, even for a space.
    let changed = REPO_JSON.replace("ci.example", "evil.example");
    assert!(!is_trusted(&trusted, "/r", &changed));
    assert!(!is_trusted(&trusted, "/r", &format!("{REPO_JSON} ")));
    // Trusting the new one replaces the old one.
    trust(&mut trusted, "/r", &changed);
    assert_eq!(trusted.len(), 1);
    assert!(is_trusted(&trusted, "/r", &changed));
    assert!(!is_trusted(&trusted, "/r", REPO_JSON));
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
            text: REPO_JSON.to_owned(),
        }],
        ..Default::default()
    };
    let text = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    let back: crate::settings::Settings = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.custom_commands, vec![c]);
    assert_eq!(back.trusted_command_files, settings.trusted_command_files);
    Ok(())
}
