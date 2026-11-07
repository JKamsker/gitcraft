use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "Usage:\n  gitcraft [REPOSITORY]\n  gitcraft --diff [--] BEFORE AFTER\n  gitcraft --merge [--] BASE LOCAL REMOTE MERGED\n  gitcraft --mergetool [--] BASE LOCAL REMOTE MERGED\n\nFile tools open their own window and never discover a repository.\nDiff is read-only. Merge writes MERGED only after Save and finish.\nExit: 0 completed, 1 cancelled, 2 usage/read/save failure.\nUse -- before a repository name beginning with -.\n";

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
        Some("--diff") => paths(args).map(Invocation::Diff),
        Some("--merge" | "--mergetool") => paths(args).map(Invocation::Merge),
        Some("--help" | "-h") if args.is_empty() => Ok(Invocation::Help),
        Some("--version" | "-V") if args.is_empty() => Ok(Invocation::Version),
        Some("--") => {
            let [path] = paths_exact(args)?;
            Ok(Invocation::Repository(Some(path)))
        }
        _ if first.as_encoded_bytes().first() == Some(&b'-') => {
            Err(format!("Unknown option: {first:?}\n{USAGE}"))
        }
        _ if args.is_empty() => Ok(Invocation::Repository(Some(first.into()))),
        _ => Err(format!("Expected one repository path\n{USAGE}")),
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
    paths
        .try_into()
        .map_err(|_| format!("Expected {N} file path(s)\n{USAGE}"))
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
