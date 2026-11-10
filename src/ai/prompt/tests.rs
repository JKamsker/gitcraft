use super::*;

fn file(path: &str, added: usize) -> String {
    let mut s = format!(
        "diff --git a/{path} b/{path}\nindex 1..2 100644\n--- a/{path}\n+++ b/{path}\n@@ -1 +1,{added} @@\n"
    );
    for i in 0..added {
        s.push_str(&format!("+line {i} of {path}\n"));
    }
    s
}

#[test]
fn a_diff_is_split_by_file_with_counts() {
    let diff = format!(
        "{}{}diff --git a/old.txt b/old.txt\ndeleted file mode 100644\n--- a/old.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-+++ b\n",
        file("src/a.rs", 2),
        file("docs/b c.md", 1)
    );
    let files = split_files(&diff);
    let seen: Vec<(&str, usize, usize)> = files
        .iter()
        .map(|f| (f.path.as_str(), f.added, f.removed))
        .collect();
    assert_eq!(
        seen,
        [("src/a.rs", 2, 0), ("docs/b c.md", 1, 0), ("old.txt", 0, 2)]
    );
    assert_eq!(files.iter().map(|f| f.text).collect::<String>(), diff);
}

#[test]
fn odd_names_are_read_from_the_diff() {
    // git quotes names with a tab, a quote or (with core.quotePath) any non-ASCII.
    let quoted = "diff --git \"a/t\\tb\" \"b/t\\tb\"\n--- \"a/t\\tb\"\n+++ \"b/t\\tb\"\n@@ -1 +1 @@\n-x\n+y\n";
    assert_eq!(split_files(quoted)[0].path, "t\tb");
    let unicode = file("ünï cødé/файл.txt", 1);
    assert_eq!(split_files(&unicode)[0].path, "ünï cødé/файл.txt");
    // A binary file has no --- / +++ lines.
    let binary = "diff --git a/img/a b.png b/img/a b.png\nnew file mode 100644\nBinary files /dev/null and b/img/a b.png differ\n";
    assert_eq!(split_files(binary)[0].path, "img/a b.png");
    assert!(split_files("").is_empty());
}

/// Found in review: names with ` b/` in them, renames, and quoted names with escapes.
#[test]
fn names_are_read_whole_and_unescaped() {
    let mode = "diff --git a/x b/y.png b/x b/y.png\nold mode 100644\nnew mode 100755\n";
    assert_eq!(split_files(mode)[0].path, "x b/y.png");
    let rename = "diff --git a/old b/x b/new b/x\nsimilarity index 100%\nrename from old b/x\nrename to new b/x\n";
    assert_eq!(split_files(rename)[0].path, "new b/x");
    let quoted = "diff --git \"a/q\\\"x\" \"b/q\\\"x\"\n--- \"a/q\\\"x\"\n+++ \"b/q\\\"x\"\n@@ -1 +1 @@\n-a\n+b\n";
    assert_eq!(split_files(quoted)[0].path, "q\"x");
    assert_eq!(unescape("\\303\\274ber\\ttab"), "über\ttab");
}

#[test]
fn a_small_diff_goes_whole() {
    let diff = format!("{}{}", file("a", 3), file("b", 3));
    let b = bound(&diff, 64 * 1024);
    assert_eq!(b.text, diff);
    assert!(b.left_out.is_empty());
    assert_eq!(b.files, 2);
}

#[test]
fn a_huge_file_is_cut_and_the_small_ones_stay_whole() {
    let small = file("small.rs", 5);
    let huge = file("huge.rs", 50_000);
    let diff = format!("{huge}{small}");
    let max = 8 * 1024;
    let b = bound(&diff, max);
    assert!(b.text.len() <= max + 100, "{}", b.text.len());
    assert!(b.text.contains(&small));
    assert!(b.text.contains("more lines of this file left out"));
    assert_eq!(
        b.left_out,
        [LeftOut {
            path: "huge.rs".to_owned(),
            added: 50_000,
            removed: 0,
            why: Why::Cut
        }]
    );
    // Cut after a whole line.
    let cut_at = b.text.find("[…").unwrap_or_default();
    assert!(b.text[..cut_at].ends_with('\n'));
}

#[test]
fn many_files_past_the_limit_are_listed() {
    let diff: String = (0..400).map(|i| file(&format!("f{i}.txt"), 20)).collect();
    let b = bound(&diff, 4096);
    assert!(b.text.len() <= 4096);
    assert!(b.left_out.iter().any(|l| l.why == Why::TooLarge));
    let listed = left_out_text(&b.left_out);
    assert!(listed.contains(": too large\n"), "{listed}");
    // As many whole files as fit go, in their order.
    assert!(b.text.contains("+++ b/f0.txt"), "{}", b.text);
    assert!(listed.contains("more files"), "{listed}");
    assert!(listed.lines().count() <= MAX_LISTED + 3);
}

#[test]
fn lock_files_are_only_named() {
    let diff = format!(
        "{}{}",
        file("Cargo.lock", 3),
        file("web/package-lock.json", 3)
    );
    let b = bound(&diff, 64 * 1024);
    assert!(b.text.is_empty());
    assert!(b.left_out.iter().all(|l| l.why == Why::LockFile));
    assert!(left_out_text(&b.left_out).contains("- Cargo.lock (+3 −0): a lock file"));
    assert!(!is_lock_file("src/Cargo.locker"));
}

#[test]
fn multibyte_text_is_cut_on_a_character() {
    let mut diff = file("emoji.txt", 0);
    for _ in 0..2000 {
        diff.push_str("+😀😀😀 ünïcødé\n");
    }
    for max in [4000, 4001, 4002, 4003] {
        let b = bound(&diff, max);
        assert!(b.text.len() <= max + 60);
        assert!(b.text.contains("😀"));
    }
}

#[test]
fn nothing_is_listed_when_nothing_is_left_out() {
    assert_eq!(left_out_text(&[]), "");
}

#[test]
fn conventional_summaries_are_recognised() {
    for yes in [
        "feat: add x",
        "fix(ui): y",
        "refactor!: z",
        "feat(api)!: drop v1",
    ] {
        assert!(is_conventional(yes), "{yes}");
    }
    for no in [
        "Add x",
        "Merge branch 'a'",
        "feat:x",
        "Fix: y",
        "feat(): y",
        "feat: ",
        "a really long kind: x",
        "",
    ] {
        assert!(!is_conventional(no), "{no}");
    }
}

#[test]
fn conventions_follow_the_history() {
    let owned = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let conventional = conventions(&owned(&["feat: a", "fix(x): b", "docs: c", "Add d"]));
    assert!(
        conventional.contains("Conventional Commits"),
        "{conventional}"
    );
    assert!(conventional.contains("- fix(x): b"));
    let plain = conventions(&owned(&["Add a", "Fix b", "feat: c"]));
    assert!(!plain.contains("Conventional"), "{plain}");
    assert!(plain.contains("- Add a"));
    // Too few to tell.
    assert!(!conventions(&owned(&["feat: a"])).contains("Conventional"));
    assert_eq!(conventions(&[]), "");
    let many: Vec<String> = (0..30).map(|i| format!("feat: {i}")).collect();
    assert_eq!(conventions(&many).lines().count(), EXAMPLES + 1);
}

#[test]
fn a_template_is_filled_in_one_pass() {
    let filled = fill(
        "A {x}\n{empty}\nB {diff} {unknown}",
        &[("x", "{diff}"), ("empty", ""), ("diff", "D")],
    );
    assert_eq!(filled, "A {diff}\nB D {unknown}");
    // No {diff}: it comes at the end all the same.
    assert_eq!(fill("Describe it.", &[("diff", "D")]), "Describe it.\n\nD");
    // An empty value not alone on its line just goes.
    assert_eq!(fill("a{e}b", &[("e", "")]), "ab");
    assert_eq!(fill("{", &[]), "{");
    assert_eq!(fill("ü{x}ü", &[("x", "€")]), "ü€ü");
}

#[test]
fn the_commit_prompt_holds_the_diff_conventions_and_what_was_left_out() {
    let settings = AiSettings {
        max_diff_kb: 4,
        ..AiSettings::default()
    };
    let diff = format!("{}{}", file("a.rs", 2), file("big.rs", 10_000));
    let subjects: Vec<String> = ["feat: a", "fix: b", "chore: c"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let p = commit_prompt(&settings, &diff, &subjects);
    assert!(p.text.contains("Conventional Commits"));
    assert!(p.text.contains("+line 0 of a.rs"));
    assert!(p.text.contains("- big.rs (+10000 −0)"));
    assert!(!p.text.contains("{diff}") && !p.text.contains("{left_out}"));
    assert_eq!((p.files, p.left_out), (2, 1));
    assert!(p.text.len() < 6 * 1024);
}

#[test]
fn the_review_prompt_names_the_branches() {
    let settings = AiSettings::default();
    let diff = file("x.rs", 1);
    let p = review_prompt(
        &settings,
        &ReviewInput {
            branch: "feat/ünï",
            base: "main",
            commits: "",
            diff: &diff,
        },
    );
    assert!(p.text.contains("branch feat/ünï adds to main"));
    assert!(p.text.contains("Commits:\n(none)"));
    assert!(p.text.contains("+line 0 of x.rs"));
}

#[test]
fn answers_are_cleaned() {
    assert_eq!(
        clean_answer("\u{1b}[1;32mfeat: x\u{1b}[0m\r\n\r\nbody\r\n"),
        "feat: x\n\nbody"
    );
    assert_eq!(clean_answer("```text\nfix: y\n\nwhy\n```"), "fix: y\n\nwhy");
    assert_eq!(clean_answer("\u{1b}]0;title\u{7}ok"), "ok");
    // A review that starts and ends with blocks of its own keeps them.
    let review = "```rust\nfoo\n```\nWhy\n```rust\nbar\n```";
    assert_eq!(clean_answer(review), review);
    assert_eq!(clean_answer("  \n "), "");
}

#[test]
fn a_message_is_split_into_summary_and_description() {
    assert_eq!(
        split_message("feat: add x\n\nBecause y.\nAnd z.\n"),
        ("feat: add x".to_owned(), "Because y.\nAnd z.".to_owned())
    );
    assert_eq!(
        split_message("Commit message:\n\n`fix: ü`"),
        ("fix: ü".to_owned(), String::new())
    );
    assert_eq!(split_message(""), (String::new(), String::new()));
    // A backtick at one end belongs to the summary.
    assert_eq!(split_message("fix: handle `None`").0, "fix: handle `None`");
    assert_eq!(split_message("\"feat: quoted\"").0, "feat: quoted");
}
