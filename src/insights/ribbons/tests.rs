use std::collections::HashMap;

use super::*;
use crate::git::fixture::Repo;

fn commit(hash: &str, index: usize) -> SeriesCommit {
    SeriesCommit {
        hash: hash.to_owned(),
        summary: format!("commit {hash}"),
        index,
        empty: false,
    }
}

fn pair(kind: PairKind, old: Option<(&str, usize)>, new: Option<(&str, usize)>) -> Pair {
    Pair {
        kind,
        old: old.map(|(h, i)| commit(h, i)),
        new: new.map(|(h, i)| commit(h, i)),
        moved: false,
        body: Vec::new(),
    }
}

fn patch(id: &str, lines: &[&str]) -> Patch {
    Patch {
        id: Some(id.to_owned()),
        lines: lines.iter().map(|l| (*l).to_owned()).collect(),
    }
}

fn classes(r: &Ribbons) -> Vec<(String, String, Class, Evidence)> {
    let mut out: Vec<_> = r
        .ribbons
        .iter()
        .map(|x| {
            (
                r.old[x.old].commit.hash.clone(),
                r.new[x.new].commit.hash.clone(),
                x.class,
                x.evidence,
            )
        })
        .collect();
    out.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    out
}

#[test]
fn exact_matches_are_observed_and_similar_ones_inferred() {
    let mut moved = pair(PairKind::Same, Some(("c", 3)), Some(("c2", 1)));
    moved.moved = true;
    let diff = RangeDiff {
        pairs: vec![
            moved,
            pair(PairKind::Same, Some(("a", 1)), Some(("a2", 2))),
            Pair {
                body: [
                    "@@ Metadata",
                    "  ## Commit message ##",
                    "-    commit b",
                    "+    commit b, reworded",
                ]
                .map(str::to_owned)
                .to_vec(),
                ..pair(PairKind::Changed, Some(("b", 2)), Some(("b2", 3)))
            },
            pair(PairKind::Changed, Some(("d", 4)), Some(("d2", 4))),
            Pair {
                body: [" ## f ##", "-+new line here", "++new  line   here"]
                    .map(str::to_owned)
                    .to_vec(),
                ..pair(PairKind::Changed, Some(("e", 5)), Some(("e2", 5)))
            },
        ],
    };
    let patches: HashMap<String, Patch> = [
        ("b", patch("p-b", &["+f\u{1f}x"])),
        ("b2", patch("p-b", &["+f\u{1f}x"])),
        ("d", patch("p-d", &["+g\u{1f}y"])),
        ("d2", patch("p-d2", &["+g\u{1f}z"])),
        ("e", patch("p-e", &["+f\u{1f}x"])),
        ("e2", patch("p-e", &["+f\u{1f}x"])),
    ]
    .into_iter()
    .map(|(h, p)| (h.to_owned(), p))
    .collect();
    let r = classify(&diff, &patches);
    assert_eq!(
        classes(&r),
        [
            (
                "a".into(),
                "a2".into(),
                Class::Unchanged,
                Evidence::Observed
            ),
            ("b".into(), "b2".into(), Class::Reworded, Evidence::Observed),
            (
                "c".into(),
                "c2".into(),
                Class::Reordered,
                Evidence::Observed
            ),
            ("d".into(), "d2".into(), Class::Modified, Evidence::Inferred),
            // The same patch-id, the message the same: the patch changed in whitespace.
            ("e".into(), "e2".into(), Class::Modified, Evidence::Observed),
        ]
    );
    // Old commits oldest first, new ones too.
    let olds: Vec<&str> = r.old.iter().map(|e| e.commit.hash.as_str()).collect();
    assert_eq!(olds, ["a", "b", "c", "d", "e"]);
    let news: Vec<&str> = r.new.iter().map(|e| e.commit.hash.as_str()).collect();
    assert_eq!(news, ["c2", "a2", "b2", "d2", "e2"]);
    assert!(r.new[0].moved);
    // Without patch-ids a changed pair stays a similarity match.
    let r = classify(&diff, &HashMap::new());
    let d = r.ribbons.iter().find(|x| r.old[x.old].commit.hash == "b");
    assert_eq!(
        d.map(|x| (x.class, x.evidence)),
        Some((Class::Modified, Evidence::Inferred))
    );
}

#[test]
fn an_unpaired_commit_folded_into_a_paired_one_is_a_squash() {
    let diff = RangeDiff {
        pairs: vec![
            pair(PairKind::Changed, Some(("a", 1)), Some(("ab", 1))),
            pair(PairKind::Dropped, Some(("b", 2)), None),
        ],
    };
    let patches: HashMap<String, Patch> = [
        ("a", patch("1", &["+a\u{1f}alpha", "+a\u{1f}beta"])),
        ("b", patch("2", &["+b\u{1f}gamma", "+b\u{1f}delta"])),
        (
            "ab",
            patch(
                "3",
                &[
                    "+a\u{1f}alpha",
                    "+a\u{1f}beta",
                    "+b\u{1f}gamma",
                    "+b\u{1f}delta",
                ],
            ),
        ),
    ]
    .into_iter()
    .map(|(h, p)| (h.to_owned(), p))
    .collect();
    let r = classify(&diff, &patches);
    assert_eq!(
        classes(&r),
        [
            ("a".into(), "ab".into(), Class::Squashed, Evidence::Inferred),
            ("b".into(), "ab".into(), Class::Squashed, Evidence::Inferred),
        ]
    );
    assert!(r.old.iter().all(|e| e.class == Class::Squashed));
    assert_eq!(r.new[0].class, Class::Squashed);
    // Nothing range-diff paired: the squash is still found.
    let diff = RangeDiff {
        pairs: vec![
            pair(PairKind::Dropped, Some(("a", 1)), None),
            pair(PairKind::Dropped, Some(("b", 2)), None),
            pair(PairKind::Added, None, Some(("ab", 1))),
        ],
    };
    let r = classify(&diff, &patches);
    assert_eq!(r.ribbons.len(), 2);
    assert!(
        r.ribbons
            .iter()
            .all(|x| x.class == Class::Squashed && x.pair.is_none())
    );
}

#[test]
fn a_new_commit_carved_out_of_a_paired_one_is_a_split() {
    let diff = RangeDiff {
        pairs: vec![
            pair(PairKind::Changed, Some(("x", 1)), Some(("x1", 1))),
            pair(PairKind::Added, None, Some(("x2", 2))),
            pair(PairKind::Dropped, Some(("gone", 2)), None),
        ],
    };
    let patches: HashMap<String, Patch> = [
        (
            "x",
            patch(
                "1",
                &[
                    "+f\u{1f}one",
                    "+f\u{1f}two",
                    "+g\u{1f}three",
                    "+g\u{1f}four",
                ],
            ),
        ),
        ("x1", patch("2", &["+f\u{1f}one", "+f\u{1f}two"])),
        ("x2", patch("3", &["+g\u{1f}three", "+g\u{1f}four"])),
        ("gone", patch("4", &["+h\u{1f}nothing like the others"])),
    ]
    .into_iter()
    .map(|(h, p)| (h.to_owned(), p))
    .collect();
    let r = classify(&diff, &patches);
    assert_eq!(
        classes(&r),
        [
            ("x".into(), "x1".into(), Class::Split, Evidence::Inferred),
            ("x".into(), "x2".into(), Class::Split, Evidence::Inferred),
        ]
    );
    // What nothing holds stays dropped, with no ribbon.
    let gone = r.old.iter().find(|e| e.commit.hash == "gone");
    assert_eq!(gone.map(|e| e.class), Some(Class::Dropped));
    let counts = r.counts();
    assert!(counts.contains(&(Class::Split, 2)));
    assert!(counts.contains(&(Class::Dropped, 1)));
}

#[test]
fn changed_lines_are_read_from_hunks_only() {
    let out = "commit aaa\n\ndiff --git a/x.sql b/x.sql\nindex 1..2 100644\n--- a/x.sql\n+++ b/x.sql\n@@ -1,2 +1,2 @@\n context\n--- a comment\n+-- a new comment\n+}\ncommit bbb\n\ndiff --git a/gone.txt b/gone.txt\ndeleted file mode 100644\n--- a/gone.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n";
    let found = parse_patches(out);
    assert_eq!(
        found.get("aaa").map(|p| p.lines.clone()),
        Some(vec![
            "-x.sql\u{1f}-- a comment".to_owned(),
            "+x.sql\u{1f}-- a new comment".to_owned(),
        ])
    );
    assert_eq!(
        found.get("bbb").map(|p| p.lines.clone()),
        Some(vec!["-gone.txt\u{1f}bye".to_owned()])
    );
}

#[test]
fn nothing_to_compare_lists_no_source() {
    assert!(sources(&[], &[], None, &[]).is_empty());
    // In step with its upstream, or only ahead: nothing rewritten.
    let branches: Vec<Tracking> = vec![
        ("a".into(), "origin/a".into(), (1, 0)),
        ("b".into(), String::new(), (0, 0)),
    ];
    // A pull request never seen, or seen at its head: nothing to compare either.
    let prs: Vec<PrSeen> = vec![
        (1, "t".into(), "h".into(), "main".into(), None),
        (2, "t".into(), "h".into(), "main".into(), Some("h".into())),
    ];
    assert!(sources(&[], &branches, Some("a"), &prs).is_empty());
}

#[test]
fn sources_put_the_checked_out_branch_first() {
    let rewrite = Rewrite {
        branch: "topic".into(),
        time: 10,
        old: Series::new("b", "o"),
        new: Series::new("b", "n"),
    };
    let branches: Vec<Tracking> = vec![
        ("other".into(), "origin/other".into(), (2, 1)),
        ("mine".into(), "origin/mine".into(), (1, 3)),
    ];
    let prs: Vec<PrSeen> = vec![(
        7,
        "Fix".into(),
        "new".into(),
        "main".into(),
        Some("old".into()),
    )];
    let list = sources(
        std::slice::from_ref(&rewrite),
        &branches,
        Some("mine"),
        &prs,
    );
    assert_eq!(list.len(), 4);
    assert_eq!(list[0], Source::Rewrite(rewrite));
    assert!(matches!(&list[1], Source::Upstream { branch, .. } if branch == "mine"));
    assert!(list[1].sides());
    assert!(matches!(&list[3], Source::Pr { number: 7, .. }));
}

/// A branch of six commits, and the same branch rewritten: the first reworded, the second
/// modified, the third dropped, the fourth and fifth squashed, the sixth kept as it was.
pub(crate) fn rewritten_repo(name: &str) -> Result<(Repo, Series, Series), String> {
    let repo = Repo::new(name)?;
    let base = repo.commit("base", &[("README.md", Some("readme text\n"))])?;
    repo.git(&["checkout", "-q", "-b", "topic"])?;
    let two = "two line one\ntwo line two\ntwo line three\ntwo line four\ntwo line five\n";
    let mut old = Vec::new();
    for (msg, file, text) in [
        ("add one", "one.txt", "one alpha\none beta\none gamma\n"),
        ("add two", "two.txt", two),
        ("add three", "three.txt", "three alpha\nthree beta\n"),
        (
            "add four",
            "four.txt",
            "four alpha\nfour beta\nfour gamma\n",
        ),
        (
            "add five",
            "five.txt",
            "five alpha\nfive beta\nfive gamma\n",
        ),
        ("add six", "six.txt", "six alpha\nsix beta\n"),
    ] {
        old.push(repo.commit(msg, &[(file, Some(text))])?);
    }
    let old_tip = repo.rev("HEAD")?;
    repo.git(&["checkout", "-q", "-b", "rewritten", &base])?;
    repo.git(&["cherry-pick", &old[0]])?;
    repo.git(&["commit", "-q", "--amend", "-m", "add one, reworded"])?;
    repo.commit(
        "add two",
        &[(
            "two.txt",
            Some("two line one\ntwo line two\ntwo line 3 changed\ntwo line four\ntwo line five\n"),
        )],
    )?;
    repo.git(&["cherry-pick", "-n", &old[3]])?;
    repo.git(&["cherry-pick", "-n", &old[4]])?;
    repo.git(&["commit", "-q", "-m", "add four and five"])?;
    repo.git(&["cherry-pick", &old[5]])?;
    let new_tip = repo.rev("HEAD")?;
    Ok((
        repo,
        Series::new(&base, &old_tip),
        Series::new(&base, &new_tip),
    ))
}

fn by_summary(r: &Ribbons) -> Vec<(String, String, Class)> {
    let mut out: Vec<_> = r
        .ribbons
        .iter()
        .map(|x| {
            (
                r.old[x.old].commit.summary.clone(),
                r.new[x.new].commit.summary.clone(),
                x.class,
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn a_rebased_branch_is_classed_commit_by_commit() -> Result<(), String> {
    let (repo, old, new) = rewritten_repo("ribbons-classed")?;
    let r = read(repo.path(), &old, &new, ("before", "after"), "topic")?;
    assert_eq!(
        by_summary(&r),
        [
            (
                "add five".into(),
                "add four and five".into(),
                Class::Squashed
            ),
            (
                "add four".into(),
                "add four and five".into(),
                Class::Squashed
            ),
            (
                "add one".into(),
                "add one, reworded".into(),
                Class::Reworded
            ),
            ("add six".into(), "add six".into(), Class::Unchanged),
            ("add two".into(), "add two".into(), Class::Modified),
        ]
    );
    let reworded = r.ribbons.iter().find(|x| x.class == Class::Reworded);
    assert_eq!(reworded.map(|x| x.evidence), Some(Evidence::Observed));
    let modified = r.ribbons.iter().find(|x| x.class == Class::Modified);
    assert_eq!(modified.map(|x| x.evidence), Some(Evidence::Inferred));
    assert!(
        r.ribbons
            .iter()
            .filter(|x| x.class == Class::Squashed)
            .all(|x| x.evidence == Evidence::Inferred)
    );
    let dropped: Vec<&str> = r
        .old
        .iter()
        .filter(|e| e.class == Class::Dropped)
        .map(|e| e.commit.summary.as_str())
        .collect();
    assert_eq!(dropped, ["add three"]);
    assert!(r.new.iter().all(|e| e.class != Class::Added));
    assert_eq!(r.provenance.partial, None);
    // The patch of a squash ribbon range-diff didn't make is read all the same.
    let squash = r
        .ribbons
        .iter()
        .position(|x| x.pair.is_none())
        .ok_or("no inferred ribbon")?;
    assert!(!ribbon_patch(repo.path(), &r, squash)?.is_empty());
    Ok(())
}

#[test]
fn a_new_commit_on_the_rewritten_branch_is_seen() -> Result<(), String> {
    let (repo, old, new) = rewritten_repo("ribbons-moved")?;
    let names = ("before", "refs/heads/rewritten");
    let first = read(repo.path(), &old, &new, names, "topic")?;
    let mut provenance = first.provenance.clone();
    let now = |name: &str| repo.rev(name).ok();
    provenance.check_tips(|name| {
        if name == "before" {
            Some(old.tip.clone())
        } else {
            now(name)
        }
    });
    assert_eq!(provenance.stale, None);
    repo.commit("add seven", &[("seven.txt", Some("seven alpha\n"))])?;
    provenance.check_tips(|name| {
        if name == "before" {
            Some(old.tip.clone())
        } else {
            now(name)
        }
    });
    assert!(provenance.stale.is_some());
    let moved = Series::new(&new.base, &repo.rev("HEAD")?);
    let second = read(repo.path(), &old, &moved, names, "topic")?;
    let added: Vec<&str> = second
        .new
        .iter()
        .filter(|e| e.class == Class::Added)
        .map(|e| e.commit.summary.as_str())
        .collect();
    assert_eq!(added, ["add seven"]);
    assert_eq!(second.new.len(), first.new.len() + 1);
    Ok(())
}

#[test]
fn two_empty_versions_have_nothing_to_draw() -> Result<(), String> {
    let repo = Repo::new("ribbons-empty")?;
    let base = repo.commit("base", &[("a.txt", Some("a\n"))])?;
    let same = Series::new(&base, &base);
    let r = read(repo.path(), &same, &same, ("old", "new"), "nothing")?;
    assert!(r.old.is_empty() && r.new.is_empty() && r.ribbons.is_empty());
    // A pull request never seen has no version to compare with.
    let err = pr_series(repo.path(), "h/o/r", 3, &base, &[]);
    assert!(err.is_err());
    Ok(())
}
