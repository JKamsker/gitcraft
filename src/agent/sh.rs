//! Writing POSIX `sh` scripts from text the user typed, whatever is in it.

pub(super) use crate::remote::sh_quote as quote;

/// `name='text'`: `text` given to the shell as it is, newlines and quotes included. (Not a
/// heredoc inside `$(…)`, which bash 3.2, macOS's `/bin/sh`, misreads around a lone `'` or `)`.)
pub(super) fn assign_verbatim(name: &str, text: &str) -> String {
    format!("{name}={}\n", quote(text))
}

/// `value` between double quotes, where `$NAME`, `$((…))` and `$(…)` still expand: only `"` and
/// `\` are kept as they are.
pub(super) fn double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Whether `name` can be an environment variable's name in `sh`.
pub(super) fn valid_name(name: &str) -> bool {
    if reserved(name) {
        return false;
    }
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Names the scripts use themselves, or that change how `sh` reads them.
pub(super) fn reserved(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("gitcraft_") || matches!(name, "IFS" | "PS4")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_assigned_as_it_is() {
        assert_eq!(assign_verbatim("x", "it's\n)"), "x='it'\\''s\n)'\n");
    }

    #[test]
    fn names_are_shell_names() {
        assert!(valid_name("PORT"));
        assert!(valid_name("_x9"));
        assert!(!valid_name("9x"));
        assert!(!valid_name(""));
        assert!(!valid_name("A-B"));
        assert!(!valid_name("Ä"));
        assert!(!valid_name("gitcraft_failed"));
        assert!(!valid_name("GITCRAFT_WORKTREE_INDEX"));
        assert!(!valid_name("IFS"));
    }

    #[test]
    fn quotes_and_backslashes_stay_literal_in_double_quotes() {
        assert_eq!(double_quoted(r#"a "b" \c $X"#), r#""a \"b\" \\c $X""#);
    }
}
