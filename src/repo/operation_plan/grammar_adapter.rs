//! Compatibility error mapping over the shared Git global-option grammar.
use crate::git;

pub(crate) fn command_envelope(args: &[String]) -> Result<(usize, &str), String> {
    git::parse_command_envelope(args).map_err(|error| match error {
        git::GlobalOptionError::MissingValue { .. } => {
            "Saved Git command has an incomplete global option; run the operation again".into()
        }
        git::GlobalOptionError::NoCommand => {
            "Saved Git command has no subcommand; run the operation again".into()
        }
        git::GlobalOptionError::UnknownOption { .. }
        | git::GlobalOptionError::InternalMarker { .. } => {
            "Saved Git command has unsupported global options; run the operation again".into()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(argv: &[&str]) -> Result<(usize, String), String> {
        let args: Vec<_> = argv.iter().map(|s| s.to_string()).collect();
        command_envelope(&args).map(|(at, name)| (at, name.to_owned()))
    }
    #[test]
    fn separate_and_attached_values_do_not_become_commands() -> Result<(), String> {
        for (argv, at) in [
            (&["--git-dir", "/tmp/r.git", "fetch"][..], 2),
            (&["--git-dir=push", "fetch"], 1),
            (&["--work-tree", "fetch", "status"], 2),
            (&["-c", "x=push", "status"], 2),
            (&["-Cx", "status"], 1),
            (&["--config-env", "x=VALUE", "push"], 2),
        ] {
            let result = parse(argv).map_err(|error| format!("{error:?}"))?;
            assert_eq!(result.0, at);
            assert_eq!(result.1, argv[at]);
        }
        Ok(())
    }
    #[test]
    fn strict_replay_refuses_unknown_incomplete_and_missing_commands() -> Result<(), String> {
        for argv in [
            &["--unknown", "fetch"][..],
            &["--git-dir"],
            &["-c"],
            &["--bare"],
            &[],
        ] {
            assert!(parse(argv).is_err(), "{argv:?}");
        }
        Ok(())
    }
}
