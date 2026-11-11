//! The complexity the churn timeline shows: a heuristic, not a parser. It counts the branch
//! points of Rust and C-like files (`if`, `match`, `for`, `while`, `case`, `catch`, `&&`, `||`,
//! `?`) outside comments, strings and character literals, which tracks how much a file
//! decides without claiming to be its cyclomatic complexity. Other languages aren't measured.

/// What the view calls the measure, so that it is never taken for a fact.
pub const LABEL: &str = "analyzer-defined (heuristic)";

/// The languages the heuristic reads, by how their strings and comments are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    /// Nested block comments, raw strings, lifetimes, `match`.
    Rust,
    /// C, C++, C#, Java, Kotlin, Swift, Scala, Dart, Objective-C.
    CLike,
    /// Back-quoted raw strings.
    Go,
    /// JavaScript and TypeScript: back-quoted template strings.
    Script,
}

/// The language of `path`, by its extension; `None`: not measured.
pub fn language(path: &str) -> Option<Lang> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => Lang::Rust,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "cs" | "java" | "kt" | "kts"
        | "swift" | "scala" | "dart" | "m" | "mm" => Lang::CLike,
        "go" => Lang::Go,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Lang::Script,
        _ => return None,
    })
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn keyword(word: &[u8], lang: Lang) -> bool {
    match lang {
        Lang::Rust => matches!(word, b"if" | b"match" | b"for" | b"while"),
        Lang::CLike | Lang::Go | Lang::Script => {
            matches!(word, b"if" | b"for" | b"while" | b"case" | b"catch")
        }
    }
}

mod literals;
use literals::{skip_block, skip_character, skip_quoted, skip_raw};

/// The branch points of `text` in `lang`, as the module says.
pub fn branch_points(text: &str, lang: Lang) -> u64 {
    let b = text.as_bytes();
    let mut n = 0;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        match c {
            b'/' if next == Some(b'/') => {
                i = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(b.len(), |p| i + p + 1);
            }
            b'/' if next == Some(b'*') => {
                i = skip_block(b, i, lang);
            }
            b'"' => i = skip_quoted(b, i + 1, b'"', true, false),
            b'\'' if lang == Lang::Rust => {
                i = skip_character(b, i);
            }
            b'\'' => i = skip_quoted(b, i + 1, b'\'', true, true),
            b'`' if lang == Lang::Go => i = skip_quoted(b, i + 1, b'`', false, false),
            b'`' if lang == Lang::Script => i = skip_quoted(b, i + 1, b'`', true, false),
            b'&' if next == Some(b'&') => {
                n += 1;
                i += 2;
            }
            b'|' if next == Some(b'|') => {
                n += 1;
                i += 2;
            }
            // `?.` reads a field that may be missing; `??` is one choice, not two.
            b'?' if next == Some(b'.') => i += 2,
            b'?' if next == Some(b'?') => {
                n += 1;
                i += 2;
            }
            b'?' => {
                n += 1;
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && is_ident(b[i]) {
                    i += 1;
                }
                let word = &b[start..i];
                if lang == Lang::Rust
                    && matches!(word, b"r" | b"br" | b"cr")
                    && let Some(end) = skip_raw(b, i)
                {
                    i = end;
                } else if keyword(word, lang) {
                    n += 1;
                }
            }
            // A number, with whatever letters it has (`0x1f`, `1e9`).
            c if c.is_ascii_digit() => {
                while i < b.len() && is_ident(b[i]) {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    n
}

/// The lines of `text`: its line ends, and one more when the last line has none.
pub fn lines(text: &[u8]) -> u64 {
    let ends = text.iter().filter(|&&c| c == b'\n').count() as u64;
    ends + u64::from(text.last().is_some_and(|&c| c != b'\n'))
}

/// Whether `data` looks binary, as git decides: a NUL in its first 8000 bytes.
pub fn is_binary(data: &[u8]) -> bool {
    data.iter().take(8000).any(|&c| c == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branches_in_comments_and_strings_do_not_count() {
        let rust = r##"
            // if this were code, for sure
            /* while /* nested if */ still a comment && */
            fn f<'a>(x: &'a str) -> Option<u8> {
                let s = "if \" for && ||";
                let r = r#"match "quoted" while"#;
                let c = '"';
                let q = '\'';
                if x.is_empty() && s.len() > 2 || r.is_empty() {
                    return None;
                }
                match x.parse::<u8>().ok()? {
                    0 => None,
                    n => Some(n),
                }
            }
        "##;
        // if, &&, ||, match, ?
        assert_eq!(branch_points(rust, Lang::Rust), 5);
        let c = "/* if */ int f(int x) { // while\n  char q = '\\''; const char *s = \"case\";\n  \
                 for (;;) { if (x > 0 ? 1 : 0) break; }\n  switch (x) { case 1: return 1; }\n}\n";
        // for, if, ?, case
        assert_eq!(branch_points(c, Lang::CLike), 4);
        let ts = "const s = `if ${x}`; const y = a?.b ?? c; if (y) {} // for\n";
        // ??, if
        assert_eq!(branch_points(ts, Lang::Script), 2);
        let go = "s := `for if` // while\nif a || b { for {} }\n";
        assert_eq!(branch_points(go, Lang::Go), 3);
    }

    #[test]
    fn words_that_contain_keywords_are_not_keywords() {
        assert_eq!(
            branch_points("let iffy = forward + whiles; 0xif1;", Lang::Rust),
            0
        );
        assert_eq!(branch_points("x.match(y)", Lang::Script), 0);
    }

    #[test]
    fn unterminated_strings_and_comments_end_with_the_text() {
        assert_eq!(branch_points("if \"never closed if", Lang::Rust), 1);
        assert_eq!(branch_points("if /* never closed if", Lang::CLike), 1);
        assert_eq!(branch_points("if r#\"raw if", Lang::Rust), 1);
    }

    #[test]
    fn languages_lines_and_binaries() {
        assert_eq!(language("src/a.rs"), Some(Lang::Rust));
        assert_eq!(language("x/Main.JAVA"), Some(Lang::CLike));
        assert_eq!(language("web/app.tsx"), Some(Lang::Script));
        assert_eq!(language("README.md"), None);
        assert_eq!(language("Makefile"), None);
        assert_eq!(language("dir.rs/file"), None);
        assert_eq!(lines(b""), 0);
        assert_eq!(lines(b"a\nb\n"), 2);
        assert_eq!(lines(b"a\nb"), 2);
        assert!(is_binary(b"ab\0c"));
        assert!(!is_binary("ü\n".as_bytes()));
    }
}
