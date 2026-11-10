//! Pure syntax for Git's global options, shared by command and operation classifiers.
//!
//! This recognizes the application's supported envelope, not every Git invocation.
//! Unknown or incomplete options stop parsing: their following values must never be
//! guessed to be commands. Callers choose their own execution, hook and network policy
//! for errors. Application-owned NUL markers are not Git commands; callers must unwrap
//! typed/internal steps before using this parser. No aliases are expanded here.

/// Why an invocation cannot be classified without guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalOptionError {
    MissingValue { index: usize },
    UnknownOption { index: usize },
    NoCommand,
    InternalMarker { index: usize },
}

/// Return `(command_index, command_name)` after consuming supported global options.
///
/// Accepts both `&[String]` and `&[&str]`, without allocating. Required values consume
/// the next word even when it is `fetch`, `push`, or starts with `-`. Attached values
/// retain Git's spelling (`-Cdir`, `-ckey=value`, `--git-dir=dir`); an empty attached
/// value is syntactically present. The subcommand and all later arguments stay opaque.
///
/// Saved-operation replay should reject errors; other classifiers can decline to
/// classify them and let Git diagnose them. This function makes no policy decision.
pub fn parse_command_envelope<S: AsRef<str>>(
    args: &[S],
) -> Result<(usize, &str), GlobalOptionError> {
    let mut at = 0;
    while let Some(word) = args.get(at).map(AsRef::as_ref) {
        if word.contains('\0') {
            return Err(GlobalOptionError::InternalMarker { index: at });
        }
        if !word.starts_with('-') {
            return Ok((at, word));
        }
        match word {
            "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace" | "--config-env"
            | "--super-prefix" => {
                let option = at;
                at += 1;
                let value = args
                    .get(at)
                    .ok_or(GlobalOptionError::MissingValue { index: option })?
                    .as_ref();
                if value.contains('\0') {
                    return Err(GlobalOptionError::InternalMarker { index: at });
                }
            }
            "--bare"
            | "--no-pager"
            | "--paginate"
            | "-p"
            | "-P"
            | "--no-replace-objects"
            | "--no-lazy-fetch"
            | "--no-optional-locks"
            | "--literal-pathspecs"
            | "--glob-pathspecs"
            | "--noglob-pathspecs"
            | "--icase-pathspecs" => {}
            w if (w.starts_with("-c") || w.starts_with("-C")) && w.len() > 2 => {}
            w if [
                "--git-dir=",
                "--work-tree=",
                "--namespace=",
                "--config-env=",
                "--super-prefix=",
            ]
            .iter()
            .any(|prefix| w.starts_with(prefix)) => {}
            _ => return Err(GlobalOptionError::UnknownOption { index: at }),
        }
        at += 1;
    }
    Err(GlobalOptionError::NoCommand)
}

#[cfg(test)]
#[path = "global_options/tests.rs"]
mod tests;
