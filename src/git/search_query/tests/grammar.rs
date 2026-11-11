//! Grammar.

use super::*;

pub(super) fn parsed(query: &str) -> SearchQuery {
    match parse(query) {
        Ok(Some(q)) => q,
        other => unreachable!("{query}: {other:?}"),
    }
}

pub(super) fn error(query: &str) -> QueryError {
    match parse(query) {
        Err(e) => e,
        other => unreachable!("{query} parsed: {other:?}"),
    }
}

// ---- Parsing ---------------------------------------------------------------------------

#[test]
pub(super) fn text_without_operators_is_searched_as_before() {
    for query in [
        "fix login",
        "\"unclosed quote",
        "fix: typo in readme",
        "http://example.com/a:b",
        "-merges",
        "",
        "  ",
        "author",
    ] {
        assert_eq!(parse(query), Ok(None), "{query}");
        assert_eq!(
            prepare(query, false),
            Ok(Plan::Plain {
                query: query.to_owned(),
                code: false
            })
        );
    }
}

#[test]
pub(super) fn every_operator_is_read() {
    let q = parsed(
        "author:alice committer:bob path:src/main.rs after:2024-01-01 before:2025-01-01 \
         message:fix contents:login merges:",
    );
    assert_eq!(q.authors, ["alice"]);
    assert_eq!(q.committers, ["bob"]);
    assert_eq!(q.paths, ["src/main.rs"]);
    assert_eq!(q.after.as_deref(), Some("2024-01-01 00:00:00"));
    assert_eq!(q.before.as_deref(), Some("2025-01-01 00:00:00"));
    assert_eq!(q.messages, ["fix"]);
    assert_eq!(q.contents.as_deref(), Some("login"));
    assert_eq!(q.merges, Some(true));
    assert!(q.text.is_empty());
    let q = parsed("regex:fn\\s+main -merges:");
    assert_eq!(q.regex.as_deref(), Some("fn\\s+main"));
    assert_eq!(q.merges, Some(false));
}

#[test]
pub(super) fn operator_names_ignore_letter_case() {
    let q = parsed("Author:alice PATH:src -Merges:");
    assert_eq!(q.authors, ["alice"]);
    assert_eq!(q.paths, ["src"]);
    assert_eq!(q.merges, Some(false));
}

#[test]
pub(super) fn free_text_goes_with_the_operators() {
    let q = parsed("fix  author:alice the   \"login  page\" bug");
    assert_eq!(q.text, "fix the login  page bug");
    assert_eq!(q.authors, ["alice"]);
}

#[test]
pub(super) fn quoted_values_keep_their_spaces_and_escapes() {
    let q =
        parsed(r#"author:"Jane Doe" path:"docs/Read me.md" message:"say \"hi\"" path:"C:\dir\\""#);
    assert_eq!(q.authors, ["Jane Doe"]);
    assert_eq!(q.paths, ["docs/Read me.md", "C:\\dir\\"]);
    assert_eq!(q.messages, ["say \"hi\""]);
    // A quote inside an unquoted value is only a character.
    assert_eq!(parsed("message:it's\"ok").messages, ["it's\"ok"]);
}

#[test]
pub(super) fn unicode_is_kept_as_typed() {
    let q = parsed("author:Zoë path:docs/café.md message:\"résumé 日本語\" 🎉");
    assert_eq!(q.authors, ["Zoë"]);
    assert_eq!(q.paths, ["docs/café.md"]);
    assert_eq!(q.messages, ["résumé 日本語"]);
    assert_eq!(q.text, "🎉");
    // Whitespace other than a space separates too.
    assert_eq!(parsed("author:a\u{3000}b").text, "b");
}

#[test]
pub(super) fn repeated_operators_add_up() {
    let q = parsed("author:a author:b path:x path:y message:m message:n");
    assert_eq!(q.authors, ["a", "b"]);
    assert_eq!(q.paths, ["x", "y"]);
    assert_eq!(q.messages, ["m", "n"]);
    assert_eq!(parsed("merges: merges:yes").merges, Some(true));
    assert_eq!(parsed("merges:no").merges, Some(false));
    assert_eq!(parsed("-merges:no").merges, Some(true));
}

#[test]
pub(super) fn relative_dates_go_to_git() {
    let q = parsed("after:2w before:yesterday");
    assert_eq!(q.after.as_deref(), Some("2.weeks.ago"));
    assert_eq!(q.before.as_deref(), Some("yesterday.midnight"));
}

#[test]
pub(super) fn mistakes_are_explained_where_they_are() {
    let cases: &[(&str, &str, usize)] = &[
        ("author:", "author: needs a name or an email", 0),
        ("fix path:", "path: needs a file or a folder", 4),
        (
            "é after:",
            "after: needs a date such as 2024-05-01 or 2w right after the colon",
            2,
        ),
        ("author:\"Jane", "quote is not closed", 7),
        ("author:\"Jane\"Doe", "space after the closing quote", 7),
        ("author:a x \"two words", "quote is not closed", 11),
        ("-author:bob", "-author: isn't supported", 0),
        ("merges:maybe", "merges: takes no value", 0),
        ("merges: -merges:", "keep one of them", 8),
        ("after:soon", "“soon” is not a date", 6),
        ("before:2024-02-30", "is not a date", 7),
        ("after:1w after:2w", "Only one after:", 9),
        ("before:1w before:2w", "Only one before:", 10),
        ("contents:a regex:b", "Only one contents: or regex:", 11),
        ("contents:a contents:b", "Only one contents: or regex:", 11),
        ("after:2024-05-01 before:2024-05-01", "must be earlier", 0),
        ("author:a\0b", "NUL", 7),
    ];
    assert!(!error("before:").message.contains("quote"));
    for (query, message, at) in cases {
        let e = error(query);
        assert!(e.message.contains(message), "{query}: {}", e.message);
        assert_eq!(e.at, *at, "{query}");
    }
}

#[test]
pub(super) fn huge_queries_with_operators_are_refused_plain_ones_are_not() {
    let long = "x".repeat(MAX_QUERY_CHARS + 1);
    assert!(parse(&long).is_ok_and(|q| q.is_none()));
    let e = error(&format!("author:{long}"));
    assert!(e.message.contains("too long"), "{}", e.message);
    let just_fits = format!("author:{}", "é".repeat(MAX_QUERY_CHARS - 7));
    assert_eq!(
        parsed(&just_fits).authors[0].chars().count(),
        MAX_QUERY_CHARS - 7
    );
}

// ---- Pieces at the cursor and quoting ---------------------------------------------------

#[test]
pub(super) fn the_piece_at_the_cursor() {
    let q = "fix author:Zo path:\"a b";
    let at = |cursor| token_at(q, cursor).map(|t| (t.op, t.value));
    assert_eq!(at(2), Some((None, "fix".to_owned())));
    assert_eq!(at(13), Some((Some(Operator::Author), "Zo".to_owned())));
    assert_eq!(at(9), Some((Some(Operator::Author), "Zo".to_owned())));
    assert_eq!(at(24), Some((Some(Operator::Path), "a b".to_owned())));
    assert_eq!(at(99), Some((Some(Operator::Path), "a b".to_owned())));
    assert_eq!(token_at("a  b", 2), None);
    assert_eq!(token_at("", 0), None);
    // Counted in characters: the é before it is two bytes.
    let t = token_at("é author:x", 9);
    assert_eq!(t.map(|t| t.span), Some(3..11));
}

#[test]
pub(super) fn values_are_quoted_when_they_need_it_and_read_back_the_same() {
    for value in [
        "alice",
        "Jane Doe",
        "say \"hi\"",
        "C:\\dir\\",
        "\"lead",
        "tab\there",
        "日本 語",
    ] {
        let written = format!("author:{} x", quote_value(value));
        let q = parsed(&written);
        assert_eq!(q.authors, [value]);
    }
    assert_eq!(quote_value("alice"), "alice");
    assert_eq!(quote_value("Jane Doe"), "\"Jane Doe\"");
    assert_eq!(quote_value(""), "\"\"");
}

// ---- Mapping to git options -------------------------------------------------------------

pub(super) fn passes(query: &str, code: bool) -> (Vec<Vec<String>>, Vec<String>) {
    match prepare(query, code) {
        Ok(Plan::Operators { passes, paths, .. }) => (passes, paths),
        other => unreachable!("{query}: {other:?}"),
    }
}

pub(super) const BASE: [&str; 4] = ["-i", "--fixed-strings", "--all-match", "--use-mailmap"];

pub(super) fn with_base(rest: &[&str]) -> Vec<String> {
    BASE.iter().chain(rest).map(|s| (*s).to_owned()).collect()
}

#[test]
pub(super) fn each_operator_is_one_git_option() {
    let (p, paths) = passes(
        "author:\"Jane Doe\" committer:bob message:fix after:2w before:2024-01-01 \
         contents:\"a b\" -merges: path:src path:\"Read me.md\"",
        false,
    );
    assert_eq!(
        p,
        [with_base(&[
            "--author=Jane Doe",
            "--committer=bob",
            "--grep=fix",
            "--since=2.weeks.ago",
            "--until=2024-01-01 00:00:00",
            "-Sa b",
            "--no-merges",
        ])]
    );
    assert_eq!(paths, ["src", "Read me.md"]);
    // Changes alone match the case typed (git's -i would apply to them too).
    let (p, _) = passes("regex:^fn merges:", false);
    assert_eq!(p, [with_base(&["-G^fn", "--merges"])[1..].to_vec()]);
    let (p, _) = passes("Foo after:2w", true);
    assert_eq!(
        p,
        [with_base(&["--since=2.weeks.ago", "-SFoo"])[1..].to_vec()]
    );
}

#[test]
pub(super) fn free_text_looks_in_messages_and_authors_or_in_code() {
    let (p, _) = passes("login merges:", false);
    assert_eq!(
        p,
        [
            with_base(&["--merges", "--grep=login"]),
            with_base(&["--merges", "--author=login"]),
        ]
    );
    // author: names the author: the text is looked for in the message only.
    let (p, _) = passes("login author:bob", false);
    assert_eq!(p, [with_base(&["--author=bob", "--grep=login"])]);
    let (p, _) = passes("login author:bob", true);
    assert_eq!(p, [with_base(&["--author=bob", "-Slogin"])]);
    let e = prepare("login contents:x", true).err();
    assert!(e.is_some_and(|e| e.message.contains("switch to Messages")));
    let (p, _) = passes("login path:src", true);
    assert_eq!(p, [with_base(&["-Slogin"])[1..].to_vec()]);
    // Without free text, Code changes nothing.
    assert_eq!(passes("contents:x", true), passes("contents:x", false));
}

#[test]
pub(super) fn values_that_look_like_options_stay_values() {
    let (p, paths) = passes("author:--all message:-v path:--output=x path:-p", false);
    assert_eq!(p, [with_base(&["--author=--all", "--grep=-v"])]);
    assert_eq!(paths, ["--output=x", "-p"]);
}
