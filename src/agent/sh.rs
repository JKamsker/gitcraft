//! Writing POSIX `sh` scripts from text the user typed, whatever is in it.

pub(super) use crate::remote::sh_quote as quote;

/// `name=$(cat <<'X' … X)`: `text` given to the shell as it is, newlines and quotes included,
/// under a delimiter that is none of its lines. (`$(…)` drops the newlines at its end.)
pub(super) fn assign_verbatim(name: &str, text: &str) -> String {
    let mut delimiter = "GITCRAFT_END".to_owned();
    let mut n = 0;
    while text.lines().any(|line| line == delimiter) {
        n += 1;
        delimiter = format!("GITCRAFT_END_{n}");
    }
    format!("{name}=$(cat <<'{delimiter}'\n{text}\n{delimiter}\n)\n")
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
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delimiter_never_ends_the_text_early() {
        let text = "echo a\nGITCRAFT_END\nGITCRAFT_END_1";
        let script = assign_verbatim("x", text);
        assert!(script.contains("<<'GITCRAFT_END_2'"), "{script}");
    }

    #[test]
    fn names_are_shell_names() {
        assert!(valid_name("PORT"));
        assert!(valid_name("_x9"));
        assert!(!valid_name("9x"));
        assert!(!valid_name(""));
        assert!(!valid_name("A-B"));
        assert!(!valid_name("Ä"));
    }

    #[test]
    fn quotes_and_backslashes_stay_literal_in_double_quotes() {
        assert_eq!(double_quoted(r#"a "b" \c $X"#), r#""a \"b\" \\c $X""#);
    }
}
