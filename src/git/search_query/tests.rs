//! The parser, its mapping to `git log` options, and searches on real repositories through
//! [`prepare`] and [`run`], as the graph's search runs them.

use std::path::{Path, PathBuf};

use super::parse::{QueryError, SearchQuery, parse};
use super::*;
use crate::git::run as git;

fn parsed(query: &str) -> SearchQuery {
    match parse(query) {
        Ok(Some(q)) => q,
        other => unreachable!("{query}: {other:?}"),
    }
}

fn error(query: &str) -> QueryError {
    match parse(query) {
        Err(e) => e,
        other => unreachable!("{query} parsed: {other:?}"),
    }
}

// ---- Parsing ---------------------------------------------------------------------------

#[test]
fn text_without_operators_is_searched_as_before() {
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
fn every_operator_is_read() {
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
fn operator_names_ignore_letter_case() {
    let q = parsed("Author:alice PATH:src -Merges:");
    assert_eq!(q.authors, ["alice"]);
    assert_eq!(q.paths, ["src"]);
    assert_eq!(q.merges, Some(false));
}

#[test]
fn free_text_goes_with_the_operators() {
    let q = parsed("fix  author:alice the   \"login  page\" bug");
    assert_eq!(q.text, "fix the login  page bug");
    assert_eq!(q.authors, ["alice"]);
}

#[test]
fn quoted_values_keep_their_spaces_and_escapes() {
    let q =
        parsed(r#"author:"Jane Doe" path:"docs/Read me.md" message:"say \"hi\"" path:"C:\dir\\""#);
    assert_eq!(q.authors, ["Jane Doe"]);
    assert_eq!(q.paths, ["docs/Read me.md", "C:\\dir\\"]);
    assert_eq!(q.messages, ["say \"hi\""]);
    // A quote inside an unquoted value is only a character.
    assert_eq!(parsed("message:it's\"ok").messages, ["it's\"ok"]);
}

#[test]
fn unicode_is_kept_as_typed() {
    let q = parsed("author:Zoë path:docs/café.md message:\"résumé 日本語\" 🎉");
    assert_eq!(q.authors, ["Zoë"]);
    assert_eq!(q.paths, ["docs/café.md"]);
    assert_eq!(q.messages, ["résumé 日本語"]);
    assert_eq!(q.text, "🎉");
    // Whitespace other than a space separates too.
    assert_eq!(parsed("author:a\u{3000}b").text, "b");
}

#[test]
fn repeated_operators_add_up() {
    let q = parsed("author:a author:b path:x path:y message:m message:n");
    assert_eq!(q.authors, ["a", "b"]);
    assert_eq!(q.paths, ["x", "y"]);
    assert_eq!(q.messages, ["m", "n"]);
    assert_eq!(parsed("merges: merges:yes").merges, Some(true));
    assert_eq!(parsed("merges:no").merges, Some(false));
    assert_eq!(parsed("-merges:no").merges, Some(true));
}

#[test]
fn relative_dates_go_to_git() {
    let q = parsed("after:2w before:yesterday");
    assert_eq!(q.after.as_deref(), Some("2.weeks.ago"));
    assert_eq!(q.before.as_deref(), Some("yesterday.midnight"));
}

#[test]
fn mistakes_are_explained_where_they_are() {
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
fn huge_queries_with_operators_are_refused_plain_ones_are_not() {
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
fn the_piece_at_the_cursor() {
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
fn values_are_quoted_when_they_need_it_and_read_back_the_same() {
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

fn passes(query: &str, code: bool) -> (Vec<Vec<String>>, Vec<String>) {
    match prepare(query, code) {
        Ok(Plan::Operators { passes, paths, .. }) => (passes, paths),
        other => unreachable!("{query}: {other:?}"),
    }
}

const BASE: [&str; 4] = ["-i", "--fixed-strings", "--all-match", "--use-mailmap"];

fn with_base(rest: &[&str]) -> Vec<String> {
    BASE.iter().chain(rest).map(|s| (*s).to_owned()).collect()
}

#[test]
fn each_operator_is_one_git_option() {
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
fn free_text_looks_in_messages_and_authors_or_in_code() {
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
fn values_that_look_like_options_stay_values() {
    let (p, paths) = passes("author:--all message:-v path:--output=x path:-p", false);
    assert_eq!(p, [with_base(&["--author=--all", "--grep=-v"])]);
    assert_eq!(paths, ["--output=x", "-p"]);
}

// ---- On real repositories ---------------------------------------------------------------

/// A repository removed when dropped.
struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Repo {
    fn new(name: &str) -> Result<Repo, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-sq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Alice Smith"],
            &["config", "user.email", "alice@example.com"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        git(Some(&self.0), args)
    }

    fn write(&self, file: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(file);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// Commits everything as `author` (`Name <email>`) on `date`, with `message`; its hash.
    fn commit(&self, message: &str, author: &str, date: &str) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
            "--author",
            author,
            "--date",
            date,
        ])?;
        self.head()
    }

    fn head(&self) -> Result<String, String> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    fn search(&self, query: &str, code: bool) -> Result<Vec<String>, String> {
        let plan = prepare(query, code).map_err(|e| e.message)?;
        run(self.path(), None, &plan)
    }
}

const ALICE: &str = "Alice Smith <alice@example.com>";
const BOB: &str = "Bob <bob@old.example>";
const ZOE: &str = "Zoë Ünal <zoe@example.com>";

/// main: init (Alice) → login fix (Bob) → feature merged (Alice, no fast-forward) →
/// résumé (Zoë, with a `.mailmap` naming Bob “Robert Bé”).
struct History {
    repo: Repo,
    init: String,
    login: String,
    feature: String,
    merge: String,
    resume: String,
}

fn history(name: &str) -> Result<History, String> {
    let repo = Repo::new(name)?;
    repo.write("src/main.rs", "fn main() {}\n")?;
    repo.write("docs/Read me.md", "# Read me\n")?;
    let init = repo.commit("Initial commit", ALICE, "2020-01-01T12:00:00Z")?;
    repo.write("src/main.rs", "fn main() {\n    login();\n}\n")?;
    let login = repo.commit(
        "Fix login bug\n\nThe session expired.",
        BOB,
        "2022-03-01T12:00:00Z",
    )?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    repo.write("src/lib.rs", "pub fn feature_flag() -> bool { true }\n")?;
    let feature = repo.commit("Add feature flag", ALICE, "2023-01-10T12:00:00Z")?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.git(&["merge", "-q", "--no-ff", "-m", "Merge feature", "feature"])?;
    let merge = repo.head()?;
    repo.write("docs/café.md", "naïve text\n")?;
    repo.write(
        ".mailmap",
        "Robert Bé <bob@new.example> <bob@old.example>\n",
    )?;
    let resume = repo.commit("Ünïcödé résumé", ZOE, "2023-06-15T12:00:00Z")?;
    Ok(History {
        repo,
        init,
        login,
        feature,
        merge,
        resume,
    })
}

#[test]
fn author_and_committer_on_a_real_repository() -> Result<(), String> {
    let h = history("people")?;
    assert_eq!(
        h.repo.search("author:alice", false)?,
        [h.merge.as_str(), h.feature.as_str(), h.init.as_str()]
    );
    assert_eq!(h.repo.search("author:ALICE@EXAMPLE.COM", false)?.len(), 3);
    assert_eq!(
        h.repo.search("author:\"Zoë Ünal\"", false)?,
        [h.resume.as_str()]
    );
    // The name `.mailmap` gives, and the one in the commit, both find Bob's commit.
    assert_eq!(
        h.repo.search("author:\"Robert Bé\"", false)?,
        [h.login.as_str()]
    );
    assert_eq!(h.repo.search("author:bob", false)?, [h.login.as_str()]);
    // Several authors: any of them.
    assert_eq!(
        h.repo.search("author:zoë author:robert", false)?,
        [h.resume.as_str(), h.login.as_str()]
    );
    // Everything was committed by the repository's user.
    assert_eq!(h.repo.search("committer:alice", false)?.len(), 5);
    assert!(h.repo.search("committer:zoë", false)?.is_empty());
    assert!(h.repo.search("author:nobody", false)?.is_empty());
    Ok(())
}

#[test]
fn path_message_and_free_text_on_a_real_repository() -> Result<(), String> {
    let h = history("paths")?;
    assert_eq!(
        h.repo.search("path:src/main.rs", false)?,
        [h.login.as_str(), h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("path:\"docs/Read me.md\"", false)?,
        [h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("path:docs/café.md", false)?,
        [h.resume.as_str()]
    );
    assert_eq!(h.repo.search("path:src", false)?.len(), 3);
    assert_eq!(
        h.repo.search("path:src/lib.rs path:docs/café.md", false)?,
        [h.resume.as_str(), h.feature.as_str()]
    );
    assert_eq!(
        h.repo.search("path:*.md", false)?,
        [h.resume.as_str(), h.init.as_str()]
    );
    assert!(h.repo.search("path:missing.txt", false)?.is_empty());
    assert_eq!(h.repo.search("message:session", false)?, [h.login.as_str()]);
    // Letter case is ignored for A to Z (git's -i). Whether git also folds other letters
    // depends on its platform and locale, so that is not asserted.
    assert_eq!(h.repo.search("message:RéSuMé", false)?, [h.resume.as_str()]);
    // Several messages must all match.
    assert_eq!(
        h.repo.search("message:fix message:bug", false)?,
        [h.login.as_str()]
    );
    assert!(
        h.repo
            .search("message:fix message:feature", false)?
            .is_empty()
    );
    // Free text: in the message or the author, narrowed by the operators.
    assert_eq!(
        h.repo.search("feature path:src", false)?,
        [h.feature.as_str()]
    );
    assert_eq!(h.repo.search("bob path:src", false)?, [h.login.as_str()]);
    assert_eq!(
        h.repo.search("\"login bug\" author:bob", false)?,
        [h.login.as_str()]
    );
    assert!(h.repo.search("bob author:alice", false)?.is_empty());
    Ok(())
}

#[test]
fn contents_regex_and_merges_on_a_real_repository() -> Result<(), String> {
    let h = history("changes")?;
    assert_eq!(
        h.repo.search("contents:login()", false)?,
        [h.login.as_str()]
    );
    assert_eq!(h.repo.search("contents:naïve", false)?, [h.resume.as_str()]);
    // With a name or message to match, git ignores the case of A to Z in changes too.
    assert_eq!(
        h.repo.search("contents:NAïVE author:zoë", false)?,
        [h.resume.as_str()]
    );
    assert_eq!(
        h.repo.search("regex:feature_[a-z]+", false)?,
        [h.feature.as_str()]
    );
    assert_eq!(
        h.repo.search("regex:fn\\s+main", false)?,
        [h.login.as_str(), h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("contents:login() author:alice", false)?,
        Vec::<String>::new()
    );
    // Code: free text is what the commits added or removed.
    assert_eq!(
        h.repo.search("feature_flag path:src", true)?,
        [h.feature.as_str()]
    );
    assert_eq!(h.repo.search("merges:", false)?, [h.merge.as_str()]);
    assert_eq!(
        h.repo.search("-merges: author:alice", false)?,
        [h.feature.as_str(), h.init.as_str()]
    );
    assert_eq!(
        h.repo.search("merge -merges:", false)?,
        Vec::<String>::new()
    );
    let e = h.repo.search("regex:[unclosed", false).err();
    assert!(e.is_some_and(|e| e.starts_with("git could not search")));
    Ok(())
}

#[test]
fn dates_on_a_real_repository() -> Result<(), String> {
    let repo = Repo::new("dates")?;
    for (summary, date) in [
        ("old", "2020-01-01T12:00:00Z"),
        ("mid", "2023-06-01T12:00:00Z"),
        ("recent", "now"),
    ] {
        repo.write("f.txt", summary)?;
        repo.commit(summary, ALICE, date)?;
    }
    // git filters by the commit date: give each commit its author's (new hashes, then).
    repo.git(&["rebase", "-q", "--committer-date-is-author-date", "--root"])?;
    let hashes = repo.git(&["log", "--format=%H %s"])?;
    let hash = |summary: &str| {
        hashes
            .lines()
            .find_map(|l| l.strip_suffix(&format!(" {summary}")))
            .unwrap_or_default()
            .to_owned()
    };
    let (old, mid, recent) = (hash("old"), hash("mid"), hash("recent"));
    assert!(!old.is_empty() && !mid.is_empty() && !recent.is_empty());
    assert_eq!(
        repo.search("after:2023", false)?,
        [recent.as_str(), mid.as_str()]
    );
    assert_eq!(repo.search("before:2023-06-01", false)?, [old.as_str()]);
    assert_eq!(
        repo.search("after:2023-06 before:2024", false)?,
        [mid.as_str()]
    );
    assert_eq!(repo.search("after:2w", false)?, [recent.as_str()]);
    assert_eq!(repo.search("after:today", false)?, [recent.as_str()]);
    assert_eq!(
        repo.search("before:1y", false)?,
        [mid.as_str(), old.as_str()]
    );
    Ok(())
}

#[test]
fn the_revisions_the_graph_shows_limit_the_search() -> Result<(), String> {
    let h = history("revisions")?;
    let feature_only = ["feature".to_owned()];
    let plan = prepare("author:alice", false).map_err(|e| e.message)?;
    assert_eq!(
        run(h.repo.path(), Some(&feature_only), &plan)?,
        [h.feature.as_str(), h.init.as_str()]
    );
    assert!(run(h.repo.path(), Some(&[]), &plan)?.is_empty());
    let plan = prepare("path:src", false).map_err(|e| e.message)?;
    assert_eq!(run(h.repo.path(), Some(&feature_only), &plan)?.len(), 3);
    Ok(())
}

#[test]
fn people_come_from_history_and_mailmap() -> Result<(), String> {
    let h = history("mailmap")?;
    let authors = people(h.repo.path(), false)?;
    let names: Vec<(&str, &str, usize)> = authors
        .iter()
        .map(|p| (p.name.as_str(), p.email.as_str(), p.commits))
        .collect();
    assert_eq!(
        names,
        [
            ("Alice Smith", "alice@example.com", 3),
            ("Robert Bé", "bob@new.example", 1),
            ("Zoë Ünal", "zoe@example.com", 1),
        ]
    );
    let committers = people(h.repo.path(), true)?;
    assert_eq!(committers.len(), 1);
    assert_eq!(committers[0].commits, 5);
    let empty = Repo::new("mailmap-empty")?;
    assert!(people(empty.path(), false)?.is_empty());
    Ok(())
}

#[test]
fn a_plain_query_is_the_search_it_always_was() -> Result<(), String> {
    let h = history("plain")?;
    assert_eq!(h.repo.search("login", false)?, [h.login.as_str()]);
    assert_eq!(h.repo.search("Zoë", false)?, [h.resume.as_str()]);
    assert_eq!(h.repo.search(&h.init[..8], false)?, [h.init.as_str()]);
    assert_eq!(h.repo.search("login();", true)?, [h.login.as_str()]);
    // Quotes and colons in plain text are only characters.
    assert!(h.repo.search("\"login bug\"", false)?.is_empty());
    assert_eq!(h.repo.search("Merge feature", false)?, [h.merge.as_str()]);
    Ok(())
}

#[test]
fn case_and_hashes_with_operators_match_the_plain_search() -> Result<(), String> {
    let h = history("case-hash")?;
    // Code is case-sensitive with or without operators that only narrow.
    assert!(h.repo.search("LOGIN()", true)?.is_empty());
    assert!(h.repo.search("LOGIN() path:src", true)?.is_empty());
    assert!(h.repo.search("contents:LOGIN()", false)?.is_empty());
    assert_eq!(h.repo.search("login() path:src", true)?, [h.login.as_str()]);
    // A hash in the free text finds its commit first, when the operators match it.
    let short = &h.login[..7];
    let found = h.repo.search(&format!("{short} path:src"), false)?;
    assert_eq!(found.first().map(String::as_str), Some(h.login.as_str()));
    assert!(
        h.repo
            .search(&format!("{short} path:docs"), false)?
            .is_empty()
    );
    assert!(
        h.repo
            .search(&format!("{short} author:alice"), false)?
            .is_empty()
    );
    Ok(())
}
