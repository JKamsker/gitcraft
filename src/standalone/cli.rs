use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "Usage:\n  gitcraft [REPOSITORY]\n  gitcraft --diff [--] BEFORE AFTER\n  gitcraft --merge [--] BASE LOCAL REMOTE MERGED\n  gitcraft --mergetool [--] BASE LOCAL REMOTE MERGED\n\nFile tools open their own window and never discover a repository.\nDiff is read-only. Merge writes MERGED only after Save and finish.\nExit: 0 completed, 1 cancelled, 2 usage/read/save failure.\nUse -- before a repository name beginning with -.\n";

/// Selected-language usage text. The raw USAGE constant remains for compatibility.
pub fn usage() -> &'static str {
    crate::i18n::text("interaction-standalone-usage")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invocation {
    Repository(Option<PathBuf>),
    Diff([PathBuf; 2]),
    Merge([PathBuf; 4]),
    Help,
    Version,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut args: Vec<_> = args.into_iter().collect();
    if args.is_empty() {
        return Ok(Invocation::Repository(None));
    }
    let first = args.remove(0);
    match first.to_str() {
        Some("--diff" | "--difftool") => paths(args).map(Invocation::Diff),
        Some("--merge" | "--mergetool") => paths(args).map(Invocation::Merge),
        Some("--help" | "-h") if args.is_empty() => Ok(Invocation::Help),
        Some("--version" | "-V") if args.is_empty() => Ok(Invocation::Version),
        Some("--") => {
            let [path] = paths_exact(args)?;
            Ok(Invocation::Repository(Some(path)))
        }
        _ if first.as_encoded_bytes().first() == Some(&b'-') => Err(crate::i18n::message(
            "interaction-standalone-error-unknown-option",
            &[
                ("option", (format!("{first:?}")).into()),
                ("usage", (usage()).into()),
            ],
        )),
        _ if args.is_empty() => Ok(Invocation::Repository(Some(first.into()))),
        _ => Err(crate::i18n::message(
            "interaction-standalone-error-repository-arity",
            &[("usage", (usage()).into())],
        )),
    }
}

fn paths<const N: usize>(mut args: Vec<OsString>) -> Result<[PathBuf; N], String> {
    if args.first().is_some_and(|s| s == "--") {
        args.remove(0);
    }
    paths_exact(args)
}
fn paths_exact<const N: usize>(args: Vec<OsString>) -> Result<[PathBuf; N], String> {
    let paths: Vec<_> = args.into_iter().map(PathBuf::from).collect();
    paths.try_into().map_err(|_| {
        crate::i18n::message(
            "interaction-standalone-error-file-arity",
            &[("count", (N).into()), ("usage", (usage()).into())],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn file_modes_have_exact_arity_and_option_looking_names_stay_native() -> Result<(), String> {
        assert_eq!(
            parse(args(&["--diff", "--", "--help", "file with ' $() & ü"]))?,
            Invocation::Diff(["--help".into(), "file with ' $() & ü".into()])
        );
        for command in ["--merge", "--mergetool"] {
            assert_eq!(
                parse(args(&[command, "base", "local", "remote", "merged"]))?,
                Invocation::Merge([
                    "base".into(),
                    "local".into(),
                    "remote".into(),
                    "merged".into()
                ])
            );
        }
        assert!(parse(args(&["--diff", "one"])).is_err());
        assert!(parse(args(&["--merge", "a", "b", "c", "d", "extra"])).is_err());
        assert!(parse(args(&["--not-an-option"])).is_err());
        assert_eq!(
            parse(args(&["--", "--diff"]))?,
            Invocation::Repository(Some("--diff".into()))
        );
        assert_eq!(parse(Vec::<OsString>::new())?, Invocation::Repository(None));
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_arguments_are_not_reconstructed_from_display_strings() -> Result<(), String> {
        use std::os::unix::ffi::OsStringExt;
        let a = OsString::from_vec(b"odd\xff\nfile".to_vec());
        let b = OsString::from_vec(b"odd\xfe\nfile".to_vec());
        assert_eq!(
            parse(["--diff".into(), a.clone(), b.clone()])?,
            Invocation::Diff([a.into(), b.into()])
        );
        Ok(())
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn actual_parser_keeps_native_paths_and_localizes_usage_and_arity() -> Result<(), String> {
        for (locale, unknown, expected) in [
            ("en-US", "Unknown option: ", "Expected 2 file paths\n"),
            (
                "de",
                "Unbekannte Option: ",
                "Genau 2 Dateipfade werden erwartet\n",
            ),
        ] {
            crate::i18n::set_language(locale);
            let hostile = OsString::from("--unknown ' $() & ü\n{}");
            let error = parse([hostile.clone()])
                .err()
                .ok_or("missing usage error")?;
            assert!(error.starts_with(&format!("{unknown}{hostile:?}\n")));
            assert!(error.ends_with(usage()));
            for command in [
                "gitcraft [REPOSITORY]",
                "gitcraft --diff [--] BEFORE AFTER",
                "gitcraft --merge [--] BASE LOCAL REMOTE MERGED",
                "gitcraft --mergetool [--] BASE LOCAL REMOTE MERGED",
            ] {
                assert!(usage().contains(command));
            }
            let error = parse([OsString::from("--diff"), OsString::from("one")])
                .err()
                .ok_or("missing arity error")?;
            assert!(error.starts_with(expected), "{error}");
            assert_eq!(
                parse([
                    OsString::from("--diff"),
                    OsString::from("--"),
                    OsString::from("--help"),
                    OsString::from("native ' $() & ü\npath"),
                ])?,
                Invocation::Diff(["--help".into(), "native ' $() & ü\npath".into()])
            );
        }
        crate::i18n::set_language("en-US");
        assert_eq!(usage(), USAGE);
        Ok(())
    }
}
