use super::*;

#[test]
fn separate_and_attached_values_cannot_become_subcommands() {
    for option in [
        "--git-dir",
        "--work-tree",
        "--namespace",
        "--config-env",
        "--super-prefix",
    ] {
        for value in ["/tmp/r.git", "fetch", "push", "-strange", ""] {
            assert_eq!(
                parse_command_envelope(&[option, value, "fetch"]),
                Ok((2, "fetch"))
            );
            let attached = format!("{option}={value}");
            assert_eq!(
                parse_command_envelope(&[attached.as_str(), "push"]),
                Ok((1, "push"))
            );
        }
    }
    for option in ["-C", "-c"] {
        for value in ["fetch", "push", "key=value", ""] {
            assert_eq!(
                parse_command_envelope(&[option, value, "status"]),
                Ok((2, "status"))
            );
            assert_eq!(
                parse_command_envelope(&[format!("{option}{value}x"), "push".into()]),
                Ok((1, "push"))
            );
        }
    }
}

#[test]
fn mixed_options_and_command_arguments_keep_their_indices() {
    assert_eq!(
        parse_command_envelope(&[
            "--bare",
            "-C",
            "push",
            "--git-dir=/tmp/r.git",
            "-c",
            "fetch",
            "fetch",
            "--unknown"
        ]),
        Ok((6, "fetch"))
    );
    assert_eq!(
        parse_command_envelope(&["--git-dir", "/tmp/r.git", "fetch"]),
        Ok((2, "fetch"))
    );
    assert_eq!(
        parse_command_envelope(&["push", "--git-dir", "fetch"]),
        Ok((0, "push"))
    );
    let owned: Vec<String> = ["--work-tree", "push", "status"].map(String::from).into();
    assert_eq!(parse_command_envelope(&owned), Ok((2, "status")));
}

#[test]
fn incomplete_unknown_and_internal_steps_are_not_guessed() {
    for option in [
        "-c",
        "-C",
        "--git-dir",
        "--work-tree",
        "--namespace",
        "--config-env",
        "--super-prefix",
    ] {
        assert_eq!(
            parse_command_envelope(&[option]),
            Err(GlobalOptionError::MissingValue { index: 0 })
        );
        assert_eq!(
            parse_command_envelope(&[option, "fetch"]),
            Err(GlobalOptionError::NoCommand)
        );
    }
    for option in ["--future", "--future=x", "--", "-"] {
        assert_eq!(
            parse_command_envelope(&[option, "push", "fetch"]),
            Err(GlobalOptionError::UnknownOption { index: 0 })
        );
    }
    for mark in ["\0try", "\0expect-route", "\0approved-push"] {
        assert_eq!(
            parse_command_envelope(&[mark, "push"]),
            Err(GlobalOptionError::InternalMarker { index: 0 })
        );
        assert_eq!(
            parse_command_envelope(&["-C", mark, "fetch"]),
            Err(GlobalOptionError::InternalMarker { index: 1 })
        );
    }
    assert_eq!(
        parse_command_envelope::<&str>(&[]),
        Err(GlobalOptionError::NoCommand)
    );
    assert_eq!(
        parse_command_envelope(&["--bare"]),
        Err(GlobalOptionError::NoCommand)
    );
}

#[test]
fn every_supported_valueless_option_preserves_the_command() {
    for option in [
        "--bare",
        "--no-pager",
        "--paginate",
        "-p",
        "-P",
        "--no-replace-objects",
        "--no-lazy-fetch",
        "--no-optional-locks",
        "--literal-pathspecs",
        "--glob-pathspecs",
        "--noglob-pathspecs",
        "--icase-pathspecs",
    ] {
        assert_eq!(parse_command_envelope(&[option, "fetch"]), Ok((1, "fetch")));
    }
}
