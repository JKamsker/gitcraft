//! Reading gitcraft's command line.

use super::*;

fn parse_strs(args: &[&str]) -> Parsed {
    parse(args.iter().map(OsString::from))
}

/// What runs; anything else parsed comes back as a `Show` naming it, equal to nothing expected.
fn run(args: &[&str]) -> Option<Command> {
    match parse_strs(args) {
        Parsed::Run(cmd) => cmd,
        other => Some(Command::Show(format!("not run: {other:?}"))),
    }
}

/// Why it is refused, or what was parsed instead.
fn fails(args: &[&str]) -> String {
    match parse_strs(args) {
        Parsed::Fail(e) => e,
        other => format!("not refused: {other:?}"),
    }
}

#[test]
fn a_folder_or_nothing() {
    assert_eq!(run(&[]), None);
    assert_eq!(run(&["."]), Some(Command::Open(".".into())));
    assert_eq!(
        run(&["C:\\Users\\Zoë\\my repo"]),
        Some(Command::Open("C:\\Users\\Zoë\\my repo".into()))
    );
    assert_eq!(
        run(&["\\\\wsl.localhost\\Ubuntu\\home\\me\\repo"]),
        Some(Command::Open(
            "\\\\wsl.localhost\\Ubuntu\\home\\me\\repo".into()
        ))
    );
    // A folder named like a command, after `--`; a lone `-` is a name too.
    assert_eq!(run(&["--", "log"]), Some(Command::Open("log".into())));
    assert_eq!(run(&["-"]), Some(Command::Open("-".into())));
}

#[test]
fn the_commands() {
    assert_eq!(
        run(&["log", "src/main.rs"]),
        Some(Command::Log("src/main.rs".into()))
    );
    assert_eq!(
        run(&["blame", "src/my file.rs:40"]),
        Some(Command::Blame {
            file: "src/my file.rs".into(),
            line: Some(40)
        })
    );
    assert_eq!(
        run(&["blame", "naïve.txt"]),
        Some(Command::Blame {
            file: "naïve.txt".into(),
            line: None
        })
    );
    assert_eq!(
        run(&["show", "v1.0^{commit}"]),
        Some(Command::Show("v1.0^{commit}".into()))
    );
    assert_eq!(
        run(&["compare", "main..feature/x"]),
        Some(Command::Compare(Range {
            base: "main".into(),
            target: "feature/x".into(),
            since_fork: false
        }))
    );
    assert_eq!(
        run(&["compare", "main...HEAD~2"]),
        Some(Command::Compare(Range {
            base: "main".into(),
            target: "HEAD~2".into(),
            since_fork: true
        }))
    );
    // An open end is HEAD, as git reads it.
    assert_eq!(
        run(&["compare", "main.."]),
        Some(Command::Compare(Range {
            base: "main".into(),
            target: "HEAD".into(),
            since_fork: false
        }))
    );
}

#[test]
fn only_a_number_after_the_last_colon_is_a_line() {
    let blame = |arg: &str| run(&["blame", arg]);
    assert_eq!(
        blame("C:\\repo\\a.rs"),
        Some(Command::Blame {
            file: "C:\\repo\\a.rs".into(),
            line: None
        })
    );
    assert_eq!(
        blame("C:\\repo\\a.rs:7"),
        Some(Command::Blame {
            file: "C:\\repo\\a.rs".into(),
            line: Some(7)
        })
    );
    assert_eq!(
        blame("notes:draft.txt"),
        Some(Command::Blame {
            file: "notes:draft.txt".into(),
            line: None
        })
    );
    assert_eq!(
        blame("a.rs:"),
        Some(Command::Blame {
            file: "a.rs:".into(),
            line: None
        })
    );
    // After `--`, a name that ends like a line (Linux allows `:` in names) is kept whole.
    assert_eq!(
        run(&["blame", "--", "notes:12"]),
        Some(Command::Blame {
            file: "notes:12".into(),
            line: None
        })
    );
    assert!(fails(&["blame", "a.rs:0"]).contains("not a line number"));
    assert!(fails(&["blame", "a.rs:99999999999"]).contains("not a line number"));
}

#[test]
fn what_is_refused() {
    assert!(fails(&["log"]).starts_with("log takes a file"));
    assert!(fails(&["show", "a", "b"]).starts_with("show takes a commit"));
    assert!(fails(&["--frobnicate"]).starts_with("unknown option --frobnicate"));
    assert!(fails(&["a", "b"]).starts_with("too many arguments"));
    // What git would take for an option, or no name at all.
    assert!(fails(&["show", "--output=/tmp/x"]).contains("starts with -"));
    assert!(fails(&["compare", "main..--all"]).contains("starts with -"));
    assert!(fails(&["show", "a b"]).contains("not a commit name"));
    assert!(fails(&["show", "a\nb"]).contains("not a commit name"));
    assert!(fails(&["show", ""]).contains("No commit given"));
    assert!(fails(&["show", &"x".repeat(300)]).contains("not a commit name"));
    assert!(fails(&["compare", "main"]).contains("a..b"));
}

#[test]
fn what_macos_adds_is_left_out() {
    let args = from_system(["-psn_0_12345", "."].map(OsString::from));
    assert_eq!(args, [OsString::from(".")]);
}

#[test]
fn help_and_version() {
    assert!(matches!(parse_strs(&["--help"]), Parsed::Print(u) if u.contains("gitcraft log")));
    assert!(matches!(parse_strs(&["-V"]), Parsed::Print(v) if v.starts_with("gitcraft ")));
}

#[cfg(unix)]
#[test]
fn a_name_that_is_not_text_is_kept_as_it_is() {
    use std::os::unix::ffi::OsStringExt;
    let name = OsString::from_vec(b"caf\xe9:12".to_vec());
    let parsed = parse([OsString::from("blame"), name.clone()]);
    assert_eq!(
        parsed,
        Parsed::Run(Some(Command::Blame {
            file: PathBuf::from(name),
            line: None
        }))
    );
}
