//! Reading range-diff's output, and pairing real series of commits.

use super::*;

/// A pair's line as range-diff prints it, full hashes made of `c`.
fn line(
    old: Option<(usize, char)>,
    mark: char,
    new: Option<(usize, char)>,
    summary: &str,
) -> String {
    let side = |s: Option<(usize, char)>| match s {
        Some((n, c)) => format!("{n}:  {}", c.to_string().repeat(40)),
        None => format!("-:  {}", "-".repeat(40)),
    };
    format!("{} {mark} {} {summary}", side(old), side(new))
}

fn commits(c: char, n: usize) -> Vec<SeriesCommit> {
    (1..=n)
        .map(|i| SeriesCommit {
            hash: format!("{c}{i:039}"),
            summary: format!("commit {i}"),
            index: i,
            empty: false,
        })
        .collect()
}

#[test]
fn identical_series_pair_up_unchanged() {
    let out = [
        line(Some((1, 'a')), '=', Some((1, 'b')), "one"),
        line(Some((2, 'c')), '=', Some((2, 'd')), "two"),
    ]
    .join("\n");
    let pairs = parse(&out);
    assert_eq!(pairs.len(), 2);
    assert!(
        pairs
            .iter()
            .all(|p| p.kind == PairKind::Same && p.body.is_empty())
    );
    assert_eq!(pairs[1].old, Some((2, "c".repeat(40))));
    assert_eq!(pairs[1].new, Some((2, "d".repeat(40))));
    assert_eq!(pairs[1].summary, "two");
}

#[test]
fn a_changed_pair_keeps_the_diff_of_its_patches() {
    let out = "1:  0e91da0 ! 1:  d8adea0 change ten
    @@ Metadata
     Author: t <t@t>

      ## Commit message ##
    -    change ten
    +    change ten (again)

      ## big.txt ##
     @@
    @@ big.txt
      9
     -10
    -+ten
    ++TEN
      12
2:  53a1584 = 2:  34c7319 change thirty
";
    let pairs = parse(out);
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].kind, PairKind::Changed);
    assert_eq!(pairs[0].summary, "change ten");
    assert_eq!(
        pairs[0].body.first().map(String::as_str),
        Some("@@ Metadata")
    );
    assert!(pairs[0].body.contains(&"-    change ten".to_owned()));
    assert!(pairs[0].body.contains(&"++TEN".to_owned()));
    assert_eq!(pairs[0].body.last().map(String::as_str), Some("  12"));
    // A blank line of the patch keeps its place (with or without its indent).
    assert!(pairs[0].body[2].trim().is_empty());
    assert_eq!(pairs[1].kind, PairKind::Same);
    assert!(pairs[1].body.is_empty());
}

#[test]
fn added_and_dropped_commits_have_one_side() {
    let out = [
        line(Some((1, 'a')), '<', None, "gone"),
        line(None, '>', Some((1, 'b')), "new"),
    ]
    .join("\n");
    let pairs = parse(&out);
    assert_eq!(pairs[0].kind, PairKind::Dropped);
    assert_eq!(
        (pairs[0].old.is_some(), pairs[0].new.is_none()),
        (true, true)
    );
    assert_eq!(pairs[1].kind, PairKind::Added);
    assert_eq!(
        (pairs[1].old.is_none(), pairs[1].new.is_some()),
        (true, true)
    );
    // A mark that doesn't fit the sides it has is no pair's line.
    assert!(parse(&line(None, '=', Some((1, 'b')), "x")).is_empty());
    assert!(parse(&line(Some((1, 'a')), '>', Some((1, 'b')), "x")).is_empty());
}

#[test]
fn reordered_commits_are_marked_moved() {
    let out = [
        line(Some((1, 'a')), '=', Some((1, 'b')), "one"),
        line(Some((3, 'c')), '=', Some((2, 'd')), "three"),
        line(Some((2, 'e')), '=', Some((3, 'f')), "two"),
    ]
    .join("\n");
    let old = vec![
        SeriesCommit {
            hash: "a".repeat(40),
            summary: "one".into(),
            index: 1,
            empty: false,
        },
        SeriesCommit {
            hash: "e".repeat(40),
            summary: "two".into(),
            index: 2,
            empty: false,
        },
        SeriesCommit {
            hash: "c".repeat(40),
            summary: "three".into(),
            index: 3,
            empty: false,
        },
    ];
    let diff = resolve(parse(&out), &old, &[]);
    let moved: Vec<bool> = diff.pairs.iter().map(|p| p.moved).collect();
    // The one that went up, not the two that kept their order.
    assert_eq!(moved, [false, true, false]);
    assert_eq!(diff.counts().moved, 1);
    assert!(!diff.counts().identical());
    // Unknown on the new side (not listed): read as printed.
    assert_eq!(
        diff.pairs[1].new.as_ref().map(|c| c.hash.clone()),
        Some("d".repeat(40))
    );
}

#[test]
fn renamed_files_and_unicode_summaries_are_kept() {
    let out = "1:  abcdef1 ! 1:  1234567 Fix naïve café ☕ – 日本語
    @@ Metadata
      ## Commit message ##
         Fix naïve café ☕ – 日本語

    - ## a.txt => b.txt ##
    + ## a.txt => c.txt ##
     @@
    - ün
    + ünï
";
    let pairs = parse(out);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].summary, "Fix naïve café ☕ – 日本語");
    assert!(pairs[0].body.contains(&"+ ## a.txt => c.txt ##".to_owned()));
    assert!(pairs[0].body.contains(&"+ ünï".to_owned()));
}

#[test]
fn a_patch_that_looks_like_range_diff_output_stays_in_the_body() {
    // gitcraft's own tests, say: indented four spaces, never a pair's line.
    let out = format!(
        "{}\n    +{}\n     {}\n",
        line(Some((1, 'a')), '!', Some((1, 'b')), "tests"),
        line(Some((1, 'c')), '=', Some((1, 'd')), "inner"),
        line(None, '>', Some((2, 'e')), "inner too"),
    );
    let pairs = parse(&out);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].body.len(), 2);
}

#[test]
fn a_very_long_series_with_padded_numbers_is_read_whole() {
    let n = MAX_SERIES;
    let width = n.to_string().len();
    let out: String = (1..=n)
        .map(|i| {
            format!(
                "{i:>width$}:  {:040x} = {i:>width$}:  {:040x} commit {i}\n",
                i,
                i + n
            )
        })
        .collect();
    let pairs = parse(&out);
    assert_eq!(pairs.len(), n);
    assert_eq!(pairs[0].old.as_ref().map(|o| o.0), Some(1));
    assert_eq!(pairs[n - 1].new.as_ref().map(|o| o.0), Some(n));
    let old: Vec<SeriesCommit> = (1..=n)
        .map(|i| SeriesCommit {
            hash: format!("{i:040x}"),
            summary: format!("commit {i}"),
            index: i,
            empty: false,
        })
        .collect();
    let diff = resolve(pairs, &old, &[]);
    assert_eq!(diff.counts().same, n);
    assert_eq!(diff.counts().moved, 0);
}

#[test]
fn hashes_not_listed_fall_back_to_their_place() {
    // Abbreviated by a git that ignored --no-abbrev, and not a prefix of anything listed.
    let out = "1:  0000000 = 1:  1111111 one\n";
    let old = commits('a', 1);
    let new = commits('b', 1);
    let diff = resolve(parse(out), &old, &new);
    assert_eq!(diff.pairs[0].old.as_ref(), old.first());
    assert_eq!(diff.pairs[0].new.as_ref(), new.first());
}

#[test]
fn one_sided_series_are_all_added_or_dropped() {
    let diff = one_sided(Vec::new(), commits('b', 2));
    assert_eq!(diff.counts().added, 2);
    let diff = one_sided(commits('a', 3), Vec::new());
    assert_eq!(diff.counts().dropped, 3);
    assert!(one_sided(Vec::new(), Vec::new()).pairs.is_empty());
}

#[test]
fn the_series_listing_tells_empty_commits_apart() {
    let out = format!(
        "{RS}{}{FS}one\n\n 1 file changed, 1 insertion(+)\n{RS}{}{FS}now empty\n{RS}{}{FS}ünï: mode only\n\n 1 file changed, 0 insertions(+), 0 deletions(-)\n",
        "a".repeat(40),
        "b".repeat(40),
        "c".repeat(40)
    );
    let list = parse_series(&out);
    assert_eq!(list.len(), 3);
    assert_eq!(
        list.iter().map(|c| c.empty).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(list[2].summary, "ünï: mode only");
    assert_eq!(list[2].index, 3);
}

/// A repository in a temporary folder, removed when dropped.
struct Repo(std::path::PathBuf);

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-rd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        run(Some(&self.0), args).map(|s| s.trim().to_owned())
    }

    /// Writes `text` to `file` and commits it as `message`; gives the commit.
    fn commit(&self, file: &str, text: &str, message: &str) -> Result<String, String> {
        std::fs::write(self.0.join(file), text).map_err(|e| e.to_string())?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--allow-empty", "-m", message])?;
        self.git(&["rev-parse", "HEAD"])
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Fifty lines, line `n` replaced by `with` for each `(n, with)`.
fn lines(changes: &[(usize, &str)]) -> String {
    (1..=50)
        .map(|n| {
            changes
                .iter()
                .find(|(m, _)| *m == n)
                .map_or(n.to_string(), |(_, w)| (*w).to_owned())
                + "\n"
        })
        .collect()
}

#[test]
fn a_rebased_series_with_one_changed_commit_pairs_up() -> Result<(), String> {
    let repo = Repo::new("rebase")?;
    let base = repo.commit("a.txt", &lines(&[]), "base")?;
    repo.commit("b.txt", &lines(&[]), "other file")?;
    repo.git(&["checkout", "-q", "-b", "topic", &base])?;
    repo.commit("a.txt", &lines(&[(5, "5555")]), "one")?;
    repo.commit("a.txt", &lines(&[(5, "5555"), (20, "2020")]), "two")?;
    let old_tip = repo.commit(
        "a.txt",
        &lines(&[(5, "5555"), (20, "2020"), (40, "4040")]),
        "three",
    )?;
    // Onto main, with the second commit changed on the way.
    repo.git(&["rebase", "-q", "main"])?;
    let main = repo.git(&["rev-parse", "main"])?;
    repo.git(&["checkout", "-q", "-b", "redo", &main])?;
    repo.commit("a.txt", &lines(&[(5, "5555")]), "one")?;
    repo.commit(
        "a.txt",
        &lines(&[(5, "5555"), (20, "2002"), (21, "2121")]),
        "two, reworked",
    )?;
    let new_tip = repo.commit(
        "a.txt",
        &lines(&[(5, "5555"), (20, "2002"), (21, "2121"), (40, "4040")]),
        "three",
    )?;
    let diff = range_diff(
        &repo.0,
        &Series::new(&base, &old_tip),
        &Series::new(&main, &new_tip),
    )?;
    let kinds: Vec<PairKind> = diff.pairs.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [PairKind::Same, PairKind::Changed, PairKind::Same],
        "{diff:#?}"
    );
    let changed = &diff.pairs[1];
    assert_eq!(
        changed.old.as_ref().map(|c| c.summary.as_str()),
        Some("two")
    );
    assert_eq!(
        changed.new.as_ref().map(|c| c.summary.as_str()),
        Some("two, reworked")
    );
    assert!(
        changed.body.iter().any(|l| l.contains("+2002")),
        "{changed:#?}"
    );
    assert_eq!(
        diff.pairs[2].new.as_ref().map(|c| c.hash.as_str()),
        Some(new_tip.as_str())
    );
    assert_eq!(diff.counts().moved, 0);
    Ok(())
}

#[test]
fn reordered_dropped_and_now_empty_commits() -> Result<(), String> {
    let repo = Repo::new("reorder")?;
    let base = repo.commit("a.txt", &lines(&[]), "base")?;
    repo.commit("a.txt", &lines(&[(5, "5555")]), "one")?;
    repo.commit("a.txt", &lines(&[(5, "5555"), (25, "2525")]), "two")?;
    let old_tip = repo.commit(
        "a.txt",
        &lines(&[(5, "5555"), (25, "2525"), (45, "4545")]),
        "three",
    )?;
    // three before two, one dropped, and an empty commit at the end.
    repo.git(&["checkout", "-q", "-b", "new", &base])?;
    repo.commit("a.txt", &lines(&[(45, "4545")]), "three")?;
    repo.commit("a.txt", &lines(&[(25, "2525"), (45, "4545")]), "two")?;
    repo.git(&["commit", "-q", "--allow-empty", "-m", "nothing left"])?;
    let new_tip = repo.git(&["rev-parse", "HEAD"])?;
    let diff = range_diff(
        &repo.0,
        &Series::new(&base, &old_tip),
        &Series::new(&base, &new_tip),
    )?;
    let counts = diff.counts();
    assert_eq!(
        (counts.same, counts.dropped, counts.added),
        (1, 1, 1),
        "{diff:#?}"
    );
    assert_eq!(counts.moved, 1, "{diff:#?}");
    let empty = diff.pairs.iter().find(|p| p.kind == PairKind::Added);
    assert!(empty.and_then(|p| p.new.as_ref()).is_some_and(|c| c.empty));
    Ok(())
}

#[test]
fn empty_series_need_no_range_diff() -> Result<(), String> {
    let repo = Repo::new("empty")?;
    let base = repo.commit("a.txt", "a\n", "base")?;
    let tip = repo.commit("a.txt", "b\n", "change")?;
    let all_new = range_diff(
        &repo.0,
        &Series::new(&base, &base),
        &Series::new(&base, &tip),
    )?;
    assert_eq!(all_new.counts().added, 1);
    let all_gone = range_diff(&repo.0, &Series::new(&base, &tip), &Series::new(&tip, &tip))?;
    assert_eq!(all_gone.counts().dropped, 1);
    let nothing = range_diff(
        &repo.0,
        &Series::new(&tip, &tip),
        &Series::new(&base, &base),
    )?;
    assert!(nothing.pairs.is_empty());
    assert!(nothing.counts().identical());
    // A lone commit's patch, every line marked.
    let patch = lone_patch(&repo.0, &tip, '+')?;
    assert_eq!(patch.first().map(String::as_str), Some("+change"));
    assert!(patch.iter().all(|l| l.starts_with('+')));
    assert!(patch.contains(&"++b".to_owned()), "{patch:?}");
    Ok(())
}

#[test]
fn a_revision_that_reads_as_an_option_is_refused() {
    let repo = Repo::new("option").ok();
    let dir = repo
        .as_ref()
        .map_or(std::path::Path::new("."), |r| r.0.as_path());
    let bad = Series::new("--output=x", "HEAD");
    assert!(range_diff(dir, &bad, &bad).is_err());
    assert!(lone_patch(dir, "--output=x", '+').is_err());
}

#[test]
fn a_carriage_return_in_a_patch_is_kept() {
    let out = "1:  abcdef1 ! 1:  1234567 crlf\r\n     @@ a.txt\r\n    -+20\n    ++20\r\n";
    let pairs = parse(out);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].summary, "crlf");
    assert_eq!(pairs[0].body, [" @@ a.txt\r", "-+20", "++20\r"]);
}

/// Numbered as range-diff numbers them, whatever order the listing came in: a series in
/// another order than the listing's isn't taken for moved.
#[test]
fn places_are_range_diffs_own() {
    let out = [
        line(Some((1, 'a')), '=', Some((1, 'b')), "one"),
        line(Some((2, 'c')), '=', Some((2, 'd')), "two"),
    ]
    .join("\n");
    let listed = |x: char, y: char| {
        vec![
            SeriesCommit {
                hash: y.to_string().repeat(40),
                summary: "two".into(),
                index: 1,
                empty: false,
            },
            SeriesCommit {
                hash: x.to_string().repeat(40),
                summary: "one".into(),
                index: 2,
                empty: false,
            },
        ]
    };
    let diff = resolve(parse(&out), &listed('a', 'c'), &listed('b', 'd'));
    assert_eq!(diff.counts().moved, 0, "{diff:#?}");
    assert_eq!(diff.pairs[0].old.as_ref().map(|c| c.index), Some(1));
}
