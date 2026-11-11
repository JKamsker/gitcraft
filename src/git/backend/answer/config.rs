//! `git config [--local] [-z|--null] [--bool|--type=bool|--int|--type=int]
//! [--get|--get-all|--get-regexp] <key or pattern>`: the config files git reads (the
//! installation's, the system's, the user's, the repository's and its worktree's, with
//! what they include), in git's order. `--local` is the repository's file alone, without
//! what it includes, as git reads a named file.

use super::*;

enum Action {
    Get,
    GetAll,
    GetRegexp,
}
enum Type {
    Raw,
    Bool,
    Int,
}

/// Handle supported read-only `git config` queries without launching Git.
pub(super) fn answer(path: &Path, args: &[&str]) -> Option<Answer> {
    let mut local = false;
    let mut nul = false;
    let mut kind = Type::Raw;
    let mut action = None;
    let mut positional = Vec::new();
    for arg in args {
        match *arg {
            "--local" if !local => local = true,
            "-z" | "--null" if !nul => nul = true,
            "--bool" | "--type=bool" if matches!(kind, Type::Raw) => kind = Type::Bool,
            "--int" | "--type=int" if matches!(kind, Type::Raw) => kind = Type::Int,
            "--get" if action.is_none() => action = Some(Action::Get),
            "--get-all" if action.is_none() => action = Some(Action::GetAll),
            "--get-regexp" if action.is_none() => action = Some(Action::GetRegexp),
            arg if arg.starts_with('-') => return None,
            arg => positional.push(arg),
        }
    }
    // `git config <key>` gets; with a value it would set.
    let [query] = positional.as_slice() else {
        return None;
    };
    let action = action.unwrap_or(Action::Get);
    let matches = matcher(&action, query)?;
    let repo = open(path)?;
    let entries = entries(&repo, local)?;
    let found: Vec<&(String, Option<BString>)> =
        entries.iter().filter(|(key, _)| matches(key)).collect();
    if found.is_empty() {
        return Some(Answer::fail(1, ""));
    }
    let shown = match action {
        Action::Get => &found[found.len() - 1..],
        Action::GetAll | Action::GetRegexp => &found[..],
    };
    let keys = matches!(action, Action::GetRegexp);
    let mut out = Vec::new();
    for (key, value) in shown {
        let value = match (&kind, value) {
            (Type::Raw, Some(value)) => Some(value.to_vec()),
            (Type::Raw, None) => None,
            (Type::Bool, value) => Some(boolean(value.as_ref())?.as_bytes().to_vec()),
            (Type::Int, Some(value)) => Some(integer(value)?.to_string().into_bytes()),
            (Type::Int, None) => return None,
        };
        if keys {
            out.extend_from_slice(key.as_bytes());
            if let Some(value) = &value {
                out.push(if nul { b'\n' } else { b' ' });
                out.extend_from_slice(value);
            }
        } else if let Some(value) = &value {
            out.extend_from_slice(value);
        }
        out.push(if nul { 0 } else { b'\n' });
    }
    Some(Answer::out(out))
}

/// Validate native query matching independently of config loading and rendering.
fn matcher(action: &Action, query: &str) -> Option<Box<dyn Fn(&str) -> bool>> {
    Some(match action {
        Action::Get | Action::GetAll => {
            let (section, subsection, name) = key_parts(query)?;
            let (section, subsection, name) = (
                section.to_owned(),
                subsection.map(str::to_owned),
                name.to_owned(),
            );
            Box::new(move |key: &str| {
                key_parts(key).is_some_and(|(s, sub, n)| {
                    s.eq_ignore_ascii_case(&section)
                        && n.eq_ignore_ascii_case(&name)
                        && sub == subsection.as_deref()
                })
            })
        }
        Action::GetRegexp => {
            // `!pattern` inverts. Git's (POSIX) regexes read `\d`, `\w`… as the letter
            // itself: those are left to it.
            let escaped_word = query
                .as_bytes()
                .windows(2)
                .any(|w| w[0] == b'\\' && w[1].is_ascii_alphanumeric());
            if query.starts_with('!') || escaped_word {
                return None;
            }
            let pattern = regex::Regex::new(&lowered_ends(query)).ok()?;
            Box::new(move |key: &str| pattern.is_match(key))
        }
    })
}

/// A key's section, subsection and name: `remote.origin.url`, `user.name`.
fn key_parts(key: &str) -> Option<(&str, Option<&str>, &str)> {
    let (section, rest) = key.split_once('.')?;
    let (subsection, name) = match rest.rsplit_once('.') {
        Some((sub, name)) => (Some(sub), name),
        None => (None, rest),
    };
    let word = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    (word(section) && word(name) && name.as_bytes()[0].is_ascii_alphabetic())
        .then_some((section, subsection, name))
}

/// The pattern as git matches it: what is before its first dot and after its last one in
/// lower case, as keys' sections and names are.
fn lowered_ends(pattern: &str) -> String {
    let first = pattern.find('.').unwrap_or(pattern.len());
    let last = pattern.rfind('.').map_or(0, |i| i + 1);
    pattern
        .char_indices()
        .map(|(i, c)| {
            if i < first || i >= last {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

/// Every setting git reads, in its order, as `section.Subsection.name` and its value
/// (`None`: written without `=`). `None` when one can't be read as git reads it.
fn entries(repo: &gix::Repository, local: bool) -> Option<Vec<(String, Option<BString>)>> {
    use gix::config::Source;
    let snapshot = repo.config_snapshot();
    let file = snapshot.plumbing();
    let mut out = Vec::new();
    // The same file found as the installation's and the system's is read once.
    let mut first_level: Vec<(Source, std::path::PathBuf)> = Vec::new();
    for section in file.sections() {
        let meta = section.meta();
        let wanted = if local {
            meta.source == Source::Local && meta.level == 0
        } else {
            matches!(
                meta.source,
                Source::GitInstallation
                    | Source::System
                    | Source::Git
                    | Source::User
                    | Source::Local
                    | Source::Worktree
            )
        };
        if !wanted {
            continue;
        }
        if meta.level == 0
            && let Some(path) = &meta.path
        {
            let seen = first_level
                .iter()
                .any(|(source, p)| *source != meta.source && p == path);
            if seen {
                continue;
            }
            if !first_level
                .iter()
                .any(|(s, p)| *s == meta.source && p == path)
            {
                first_level.push((meta.source, path.clone()));
            }
        }
        let header = section.header();
        if header.is_legacy() {
            return None;
        }
        let name = header.name().to_str().ok()?.to_ascii_lowercase();
        let prefix = match header.subsection_name() {
            Some(sub) => format!("{name}.{}", sub.to_str().ok()?),
            None => name,
        };
        let body = section.body();
        let names: Vec<String> = body.value_names().collect();
        let mut taken: Vec<(String, usize)> = Vec::new();
        for value_name in &names {
            let lower = value_name.to_ascii_lowercase();
            let occurrences = names
                .iter()
                .filter(|n| n.eq_ignore_ascii_case(value_name))
                .count();
            let value = if occurrences == 1 {
                // `None` inside: written without `=`, which `values` reads as empty.
                body.value_implicit(value_name)?
            } else {
                let at = match taken.iter_mut().find(|(n, _)| *n == lower) {
                    Some((_, at)) => {
                        *at += 1;
                        *at - 1
                    }
                    None => {
                        taken.push((lower.clone(), 1));
                        0
                    }
                };
                let values = body.values(value_name);
                // One of them without `=`: which one isn't known here.
                if values.len() != occurrences || body.value_implicit(value_name)?.is_none() {
                    return None;
                }
                Some(values.get(at)?.clone())
            };
            out.push((format!("{prefix}.{lower}"), value));
        }
    }
    Some(out)
}

/// `--bool`, as git prints it; `None` for what git refuses.
fn boolean(value: Option<&BString>) -> Option<&'static str> {
    let Some(value) = value else {
        return Some("true");
    };
    let text = value.to_str().ok()?;
    match text.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" => Some("true"),
        "false" | "no" | "off" | "" => Some("false"),
        _ => integer(value).map(|n| if n != 0 { "true" } else { "false" }),
    }
}

/// `--type=int`: a whole number, with `k`, `m` or `g` after it; `None` for other forms.
fn integer(value: &BString) -> Option<i64> {
    let text = value.to_str().ok()?;
    let (digits, unit) = match text.char_indices().last()? {
        (i, 'k' | 'K') => (&text[..i], 1 << 10),
        (i, 'm' | 'M') => (&text[..i], 1 << 20),
        (i, 'g' | 'G') => (&text[..i], 1 << 30),
        _ => (text, 1),
    };
    let unsigned = digits.strip_prefix('-').unwrap_or(digits);
    // Git also reads `0x10` and `010` (octal): left to it.
    if unsigned.is_empty()
        || !unsigned.bytes().all(|b| b.is_ascii_digit())
        || (unsigned.len() > 1 && unsigned.starts_with('0'))
    {
        return None;
    }
    digits.parse::<i64>().ok()?.checked_mul(unit)
}
