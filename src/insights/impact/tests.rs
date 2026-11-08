use super::scan::{self, ItemKind, mask, scan};
use super::*;

fn names(text: &str) -> Vec<(ItemKind, String, u32, u32)> {
    scan(text)
        .into_iter()
        .map(|i| (i.kind, i.path, i.start, i.end))
        .collect()
}

#[test]
fn comments_strings_and_characters_are_blanked_with_their_lines_kept() {
    let text = "let a = \"fn x() { \\\" }\"; // fn y() {\n\
                /* outer /* fn z() {} */ still */ let b = r#\"a \"quoted\" fn w\"#;\n\
                let c = '\"'; let d = '\\''; let e = b'{'; fn f<'a>(x: &'a str) {}\n\
                let g = br##\"}\"##; let h = c\"{\";";
    let masked = mask(text);
    assert_eq!(masked.lines().count(), text.lines().count());
    for word in [
        "fn x", "fn y", "fn z", "fn w", "quoted", "'\"'", "b'{'", "\"}\"",
    ] {
        assert!(!masked.contains(word), "{word} left in {masked}");
    }
    assert!(masked.contains("let a ="));
    assert!(!masked.contains("still"));
    // A lifetime is not a character.
    assert!(masked.contains("fn f<'a>(x: &'a str) {}"));
}

/// A file cut off in an attribute (a truncated file, a parser's fixture) is scanned, not a
/// panic.
#[test]
fn a_file_ending_in_an_open_attribute_is_scanned() {
    for text in ["#[", "fn a() {}\n#[", "#![", "fn a() {}\n#[cfg(test"] {
        let items = scan::scan(text);
        assert!(items.len() <= 1, "{text:?}: {items:?}");
    }
}

#[test]
fn the_items_of_a_file_are_found_with_where_they_start_and_end() -> Result<(), String> {
    let text = r#"//! A crate.
use std::fmt;

/// A thing.
/// Two lines.
#[derive(Debug)]
pub struct Thing {
    pub name: String,
}

pub(crate) struct Pair(u8, u8);

impl<T: Fn() -> u8> fmt::Display for Thing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = "fn fake() {";
        write!(f, "{s}")
    }
}

impl Thing {
    pub const fn new() -> Self {
        Thing { name: String::new() }
    }
    fn make(&self) -> impl Iterator<Item = u8> {
        [1u8; 3].into_iter()
    }
}

pub trait Speak {
    fn speak(&self);
    fn loud(&self) -> String { String::new() }
}

const LIMIT: Thing = Thing { name: String::new() };
pub static mut COUNT: u8 = 0;
type Alias<const N: usize> = [u8; N];

macro_rules! twice {
    ($e:expr) => { $e; $e };
}

enum Mode { A = { 1 }, B }

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let r#type = 1;
    }
}
"#;
    let items = scan(text);
    let got = names(text);
    let find = |path: &str| {
        items
            .iter()
            .find(|i| i.path == path)
            .ok_or_else(|| format!("{path} not in {got:?}"))
    };
    let thing = find("Thing")?;
    assert_eq!(
        (thing.kind, thing.start, thing.line, thing.end),
        (ItemKind::Struct, 4, 7, 9)
    );
    assert!(thing.public);
    let pair = find("Pair")?;
    assert_eq!((pair.start, pair.end), (11, 11));
    assert!(!pair.public);
    let display = find("impl Display for Thing")?;
    assert_eq!(display.name, "Thing");
    assert_eq!((display.start, display.end), (13, 18));
    let fmt = find("Thing::fmt")?;
    assert_eq!((fmt.start, fmt.end), (14, 17));
    assert!(items.iter().all(|i| i.name != "fake"));
    let new = find("Thing::new")?;
    assert!(new.public);
    assert_eq!((new.start, new.end), (21, 23));
    assert_eq!(find("Thing::make")?.end, 26);
    assert_eq!(find("Speak::speak")?.end, 30);
    assert_eq!(find("Speak::loud")?.end, 31);
    assert_eq!(find("LIMIT")?.end, 34);
    assert!(find("COUNT")?.public);
    assert_eq!(find("COUNT")?.kind, ItemKind::Static);
    assert_eq!(find("Alias")?.kind, ItemKind::Type);
    assert!(items.iter().all(|i| i.name != "N"));
    let twice = find("twice")?;
    assert_eq!((twice.kind, twice.end), (ItemKind::Macro, 40));
    assert_eq!(find("Mode")?.end, 42);
    let tests = find("tests")?;
    assert!(tests.test);
    assert_eq!(tests.start, 44);
    let works = find("tests::it_works")?;
    assert!(works.test);
    assert!(!find("Thing::new")?.test);
    assert!(scan::in_test(&items, 49));
    assert!(!scan::in_test(&items, 22));
    assert_eq!(
        scan::innermost(&items, 22).map(|i| items[i].path.as_str()),
        Some("Thing::new")
    );
    assert_eq!(
        scan::innermost(&items, 20).map(|i| items[i].path.as_str()),
        Some("impl Thing")
    );
    assert_eq!(scan::innermost(&items, 2), None);
    Ok(())
}

#[test]
fn test_attributes_are_told_from_others() {
    let items = scan(
        "#[tokio::test]\nasync fn a() {}\n#[cfg(not(test))]\nfn b() {}\n\
         #[cfg(all(test, unix))]\nfn c() {}\n#[allow(dead_code)]\nfn d() {}\n",
    );
    let test: Vec<(&str, bool)> = items.iter().map(|i| (i.name.as_str(), i.test)).collect();
    assert_eq!(test, [("a", true), ("b", false), ("c", true), ("d", false)]);
}

fn hunk(old_start: u32, old_count: u32, new_start: u32, new_count: u32) -> Hunk {
    Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
    }
}

#[test]
fn changed_lines_are_mapped_to_the_items_around_them_on_both_sides() {
    let base = scan("fn keep() {\n    1\n}\n\nfn gone() {\n}\n\nfn edit() {\n    2\n}\n");
    let head = scan("use x;\nfn keep() {\n    1\n}\n\nfn edit() {\n    3\n}\nfn new_one() {}\n");
    // `use x;` added; `gone` removed; a line of `edit` changed; `new_one` added.
    let hunks = [
        hunk(0, 0, 1, 1),
        hunk(5, 3, 5, 0),
        hunk(9, 1, 7, 1),
        hunk(10, 0, 9, 1),
    ];
    let (touched, outside) = touched(&hunks, &head, &base);
    let by_name = |t: &Touched| {
        t.head
            .map(|i| head[i].name.clone())
            .or(t.base.map(|i| base[i].name.clone()))
            .unwrap_or_default()
    };
    let mut got: Vec<(String, Change, u32)> = touched
        .iter()
        .map(|t| (by_name(t), t.change(), t.lines))
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("edit".to_owned(), Change::Modified, 2),
            ("gone".to_owned(), Change::Removed, 2),
            ("new_one".to_owned(), Change::Added, 1),
        ]
    );
    // `use x;` and the blank line after `gone`.
    assert_eq!(outside, 2);
}

#[test]
fn words_paths_and_grep_lines_are_read() {
    assert!(has_word("x.foo(1)", "foo"));
    assert!(!has_word("x.foobar(1)", "foo"));
    assert!(!has_word("my_foo()", "foo"));
    assert!(has_word("é foo", "foo"));
    assert!(test_path("tests/it.rs"));
    assert!(test_path("crates/a/tests/it.rs"));
    assert!(test_path("src/repo/insights/tests.rs"));
    assert!(test_path("src/git/clone_tests.rs"));
    assert!(!test_path("src/testing.rs"));
    let hits = parse_grep(
        "abc:src/a:b.rs\x0012\x00    foo();\nabc:x.rs\x003\0fn foo() {}\n",
        "abc",
    );
    assert_eq!(
        hits,
        [
            Hit {
                path: "src/a:b.rs".to_owned(),
                line: 12,
                text: "    foo();".to_owned()
            },
            Hit {
                path: "x.rs".to_owned(),
                line: 3,
                text: "fn foo() {}".to_owned()
            }
        ]
    );
}

const LIB: &str = "pub fn compute(x: u32) -> u32 {\n    x + 1\n}\n\n\
                   fn helper() -> u32 {\n    compute(2)\n}\n\n\
                   #[cfg(test)]\nmod tests {\n    #[test]\n    fn it_computes() {\n        \
                   assert_eq!(super::compute(1), 2);\n    }\n}\n";

/// A repository where `compute` is used by other code, a test, a comment and a string, and a
/// second commit changes it and a TypeScript file.
fn fixture(name: &str) -> Result<(crate::git::fixture::Repo, String, String), String> {
    let repo = crate::git::fixture::Repo::new(name)?;
    let one = repo.commit(
        "one",
        &[
            ("src/lib.rs", Some(LIB)),
            (
                "src/user.rs",
                Some("// compute is called here\npub fn run() -> u32 {\n    crate::compute(5)\n}\nconst S: &str = \"compute\";\n"),
            ),
            ("tests/it.rs", Some("#[test]\nfn outer() {\n    app::compute(0);\n}\n")),
            ("web/app.ts", Some("export const a = 1;\n")),
        ],
    )?;
    let two = repo.commit(
        "two",
        &[
            ("src/lib.rs", Some(&LIB.replace("x + 1", "x + 2"))),
            ("web/app.ts", Some("export const a = 2;\n")),
        ],
    )?;
    Ok((repo, one, two))
}

#[test]
fn a_real_change_has_its_symbols_references_and_unknown_files() -> Result<(), String> {
    let (repo, one, two) = fixture("impact")?;
    let range = resolve(repo.path(), &Ask::Commit(two.clone()))?;
    assert_eq!(
        (range.base.as_str(), range.head.as_str()),
        (one.as_str(), two.as_str())
    );
    let impact = analyze(repo.path(), &range)?;
    assert_eq!(impact.symbols.len(), 1, "{:?}", impact.symbols);
    let compute = &impact.symbols[0];
    assert_eq!(compute.name, "compute");
    assert_eq!(compute.change, Change::Modified);
    assert!(compute.public);
    assert_eq!(
        compute.head.as_ref().map(|s| (s.rev.as_str(), s.line)),
        Some((two.as_str(), 1))
    );
    let mut sites: Vec<(&str, &str, bool, bool)> = impact
        .sites
        .iter()
        .map(|s| (s.path.as_str(), s.label.as_str(), s.test, s.in_diff))
        .collect();
    sites.sort();
    assert_eq!(
        sites,
        [
            ("src/lib.rs", "helper", false, true),
            ("src/lib.rs", "tests::it_computes", true, true),
            ("src/user.rs", "run", false, false),
            ("tests/it.rs", "outer", true, false),
        ]
    );
    // The comment and the string naming it are not references.
    assert_eq!(impact.in_comments, 2);
    assert!(impact.refs.iter().all(|r| r.rev == two));
    assert_eq!(impact.other.len(), 1);
    assert_eq!(impact.other[0].why, "no analyzer for .ts files");
    assert!(impact.partial.is_empty());
    let p = impact.provenance();
    assert_eq!(p.tips[0], ("head".to_owned(), two));
    Ok(())
}

#[test]
fn references_are_read_at_the_head_not_from_later_commits_or_the_working_tree() -> Result<(), String>
{
    let (repo, _, two) = fixture("impact-revision")?;
    repo.commit(
        "three",
        &[(
            "src/late.rs",
            Some("fn late() -> u32 { crate::compute(9) }\n"),
        )],
    )?;
    std::fs::write(
        repo.path().join("src/user.rs"),
        "pub fn run() -> u32 {\n    crate::compute(5)\n}\nfn dirty() { crate::compute(7); }\n",
    )
    .map_err(|e| e.to_string())?;
    let at_two = analyze(repo.path(), &resolve(repo.path(), &Ask::Commit(two))?)?;
    assert!(at_two.sites.iter().all(|s| s.path != "src/late.rs"));
    assert!(at_two.sites.iter().all(|s| s.label != "dirty"));
    // The branch as a whole does have the later caller, at its own head.
    let branch = resolve(
        repo.path(),
        &Ask::Branch {
            base: Some("main~2".to_owned()),
        },
    )?;
    let whole = analyze(repo.path(), &branch)?;
    assert!(whole.sites.iter().any(|s| s.path == "src/late.rs"));
    assert!(whole.sites.iter().all(|s| s.label != "dirty"));
    // Once the branch moves on, what was read names the commit it was read at.
    let (name, sha) = branch.tip.clone().ok_or("no tip")?;
    assert_eq!(name, "main");
    repo.commit("four", &[("src/more.rs", Some("fn more() {}\n"))])?;
    let now = resolve(
        repo.path(),
        &Ask::Branch {
            base: Some("main~3".to_owned()),
        },
    )?;
    assert_ne!(now.head, sha);
    let mut p = whole.provenance().tip(name, sha);
    let moved = now.head.clone();
    p.check_tips(|n| {
        (n == "main")
            .then(|| moved.clone())
            .or_else(|| Some(String::new()))
    });
    assert!(p.stale.is_some());
    Ok(())
}

#[test]
fn removed_symbols_are_looked_for_at_both_revisions() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("impact-removed")?;
    repo.commit(
        "one",
        &[
            ("a.rs", Some("pub fn old_name() {}\n")),
            ("b.rs", Some("fn user() { old_name(); }\n")),
        ],
    )?;
    let two = repo.commit("two", &[("a.rs", Some("pub fn new_name() {}\n"))])?;
    let impact = analyze(
        repo.path(),
        &resolve(repo.path(), &Ask::Commit(two.clone()))?,
    )?;
    let mut changes: Vec<(&str, Change)> = impact
        .symbols
        .iter()
        .map(|s| (s.name.as_str(), s.change))
        .collect();
    changes.sort();
    assert_eq!(
        changes,
        [("new_name", Change::Added), ("old_name", Change::Removed)]
    );
    // The caller left behind is found at the head, where it no longer compiles.
    assert!(impact.refs.iter().any(|r| r.path == "b.rs" && r.rev == two));
    Ok(())
}

#[test]
fn a_change_in_no_supported_language_is_unknown_and_a_missing_head_is_said() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("impact-unknown")?;
    repo.commit("one", &[("a.py", Some("x = 1\n"))])?;
    let two = repo.commit(
        "two",
        &[("a.py", Some("x = 2\n")), ("Makefile", Some("all:\n"))],
    )?;
    let impact = analyze(repo.path(), &resolve(repo.path(), &Ask::Commit(two))?)?;
    assert!(impact.symbols.is_empty());
    let why: Vec<&str> = impact.other.iter().map(|o| o.why.as_str()).collect();
    assert_eq!(
        why,
        ["no analyzer for this file", "no analyzer for .py files"]
    );
    let missing = resolve(
        repo.path(),
        &Ask::Pr {
            number: 7,
            head: "0123456789012345678901234567890123456789".to_owned(),
            base: "main".to_owned(),
        },
    );
    assert!(missing.is_err_and(|e| e.contains("fetch")));
    // A root commit is compared with nothing.
    let root = resolve(repo.path(), &Ask::Commit("main~1".to_owned()))?;
    let first = analyze(repo.path(), &root)?;
    assert_eq!(first.other.len(), 1);
    Ok(())
}

/// A local base branch behind its remote one: the change is counted from where it left the
/// newer one, not with the base's own commits in it.
#[test]
fn a_stale_local_base_branch_doesnt_add_its_commits_to_the_change() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("impact-stale-base")?;
    let a = repo.commit("a", &[("a.rs", Some("fn a() {}\n"))])?;
    let b = repo.commit("b", &[("b.rs", Some("fn b() {}\n"))])?;
    // origin/main has b; main is still at a.
    repo.git(&["update-ref", "refs/remotes/origin/main", &b])?;
    repo.git(&["reset", "-q", "--hard", &a])?;
    repo.git(&["checkout", "-q", "-b", "topic", &b])?;
    let c = repo.commit("c", &[("c.rs", Some("fn c() {}\n"))])?;
    let pr = resolve(
        repo.path(),
        &Ask::Pr {
            number: 1,
            head: c.clone(),
            base: "main".to_owned(),
        },
    )?;
    assert_eq!(pr.base, b);
    let branch = resolve(
        repo.path(),
        &Ask::Branch {
            base: Some("main".to_owned()),
        },
    )?;
    assert_eq!(branch.base, b);
    Ok(())
}
