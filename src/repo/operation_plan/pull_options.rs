/// Classify the options consumed by Git's builtin/pull.c and parse-options.c.
/// Optional arguments consume only attached text; required arguments also consume
/// the next token, even when it looks like an option. Git permutes operands until `--` or `--end-of-options`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::repo) struct PullMode {
    pub(super) rebase: Option<bool>,
    pub(super) ff_only: bool,
    pub(super) ff_explicit: bool,
    pub(super) recurse: Option<bool>,
}

pub(in crate::repo) fn pull_mode(args: &[String]) -> Result<PullMode, String> {
    // (long name, argument: 0 = none, 1 = required, 2 = optional).
    const OPTIONS: &[(&str, u8)] = &[
        ("verbose", 0),
        ("quiet", 0),
        ("progress", 0),
        ("recurse-submodules", 2),
        ("rebase", 2),
        ("stat", 0),
        ("summary", 0),
        ("compact-summary", 0),
        ("log", 2),
        ("signoff", 2),
        ("squash", 0),
        ("commit", 0),
        ("edit", 0),
        ("cleanup", 1),
        ("ff", 0),
        ("ff-only", 0),
        ("verify", 0),
        ("verify-signatures", 0),
        ("autostash", 0),
        ("strategy", 1),
        ("strategy-option", 1),
        ("gpg-sign", 2),
        ("allow-unrelated-histories", 0),
        ("all", 0),
        ("append", 0),
        ("upload-pack", 1),
        ("force", 0),
        ("tags", 0),
        ("prune", 0),
        ("jobs", 2),
        ("dry-run", 0),
        ("keep", 0),
        ("depth", 1),
        ("shallow-since", 1),
        ("shallow-exclude", 1),
        ("deepen", 1),
        ("unshallow", 0),
        ("update-shallow", 0),
        ("refmap", 1),
        ("server-option", 1),
        ("ipv4", 0),
        ("ipv6", 0),
        ("negotiation-tip", 1),
        ("show-forced-updates", 0),
        ("set-upstream", 0),
    ];
    fn boolean_value(value: &str) -> Result<bool, String> {
        match value.to_ascii_lowercase().as_str() {
            "" | "false" | "no" | "off" | "0" => Ok(false),
            "true" | "yes" | "on" | "1" => Ok(true),
            _ => {
                // git_parse_maybe_bool also accepts base-0 integers with k/m/g
                // suffixes, bounded symmetrically by INT_MAX (parse.c).
                let lower = value.to_ascii_lowercase();
                let number = lower.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
                let number = number.strip_prefix(['+', '-']).unwrap_or(number);
                let (number, factor) = match number.as_bytes().last() {
                    Some(b'k') => (&number[..number.len() - 1], 1024_i64),
                    Some(b'm') => (&number[..number.len() - 1], 1024_i64.pow(2)),
                    Some(b'g') => (&number[..number.len() - 1], 1024_i64.pow(3)),
                    _ => (number, 1),
                };
                let (digits, radix) = if let Some(hex) = number.strip_prefix("0x") {
                    (hex, 16)
                } else if number.starts_with('0') {
                    (number, 8)
                } else {
                    (number, 10)
                };
                i64::from_str_radix(digits, radix)
                    .ok()
                    .filter(|_| !digits.starts_with(['+', '-']))
                    .filter(|n| *n >= 0 && *n <= i64::from(i32::MAX) / factor)
                    .map(|n| n != 0)
                    .ok_or_else(|| format!("Invalid pull boolean value: {value}"))
            }
        }
    }
    fn rebase_value(value: &str) -> Result<bool, String> {
        if matches!(value, "merges" | "m" | "interactive" | "i") {
            Ok(true)
        } else {
            boolean_value(value)
        }
    }
    let mut mode = PullMode::default();
    let mut tokens = args.iter().skip(1);
    while let Some(token) = tokens.next() {
        if matches!(token.as_str(), "--" | "--end-of-options") {
            break;
        }
        if let Some(long) = token.strip_prefix("--") {
            let (name, value) = long
                .split_once('=')
                .map_or((long, None), |(n, v)| (n, Some(v)));
            let (name, negated) = name
                .strip_prefix("no-")
                .map_or((name, false), |n| (n, true));
            let exact = OPTIONS.iter().find(|(n, _)| *n == name);
            let mut matches = OPTIONS.iter().filter(|(n, _)| n.starts_with(name));
            let option = if let Some(exact) = exact {
                exact
            } else {
                let first = matches
                    .next()
                    .ok_or_else(|| format!("Unknown pull option: {token}"))?;
                if matches.next().is_some() {
                    return Err(format!("Ambiguous pull option: {token}"));
                }
                first
            };
            let (name, arity) = *option;
            if (negated || arity == 0) && value.is_some() {
                return Err(format!("Pull option takes no value: {token}"));
            }
            if negated && matches!(name, "ff-only" | "unshallow" | "refmap") {
                return Err(format!("Pull option cannot be negated: {token}"));
            }
            if !negated && arity == 1 && value.is_none() && tokens.next().is_none() {
                return Err(format!("Pull option requires a value: {token}"));
            }
            match name {
                "recurse-submodules" => {
                    mode.recurse = Some(if negated {
                        false
                    } else {
                        let value = value.unwrap_or("yes");
                        value == "on-demand" || boolean_value(value)?
                    });
                }
                "rebase" => {
                    mode.rebase = Some(if negated {
                        false
                    } else {
                        value.map_or(Ok(true), rebase_value)?
                    })
                }
                "ff-only" => {
                    mode.ff_only = true;
                    mode.ff_explicit = true;
                }
                "ff" => {
                    mode.ff_only = false;
                    mode.ff_explicit = true;
                }
                _ => {}
            }
        } else if let Some(short) = token.strip_prefix('-') {
            let letters = short.char_indices();
            for (at, letter) in letters {
                let rest = &short[at + letter.len_utf8()..];
                match letter {
                    'r' => {
                        mode.rebase = Some(if rest.is_empty() {
                            true
                        } else {
                            rebase_value(rest)?
                        });
                        break;
                    }
                    's' | 'X' | 'o' => {
                        if rest.is_empty() && tokens.next().is_none() {
                            return Err(format!("Pull option requires a value: -{letter}"));
                        }
                        break;
                    }
                    'S' | 'j' => break, // optional value is the entire attached rest
                    'v' | 'q' | 'n' | 'a' | 'f' | 't' | 'p' | 'k' | '4' | '6' => {}
                    _ => return Err(format!("Unknown pull option: -{letter}")),
                }
            }
        }
    }
    Ok(mode)
}
