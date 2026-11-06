//! Parents recorded in a real repository's config, read back, and suggested from history.

use std::collections::BTreeMap;

use super::suggest::{Candidate, pick_parent};
use super::test_repo::TestRepo;
use super::*;

fn local(repo: &TestRepo) -> Result<Vec<String>, String> {
    Ok(repo
        .git(&["for-each-ref", "--format=%(refname:lstrip=2)", "refs/heads"])?
        .lines()
        .map(str::to_owned)
        .collect())
}

fn rec(pairs: &[(&str, &str)]) -> BTreeMap<String, Recorded> {
    pairs
        .iter()
        .map(|(b, p)| {
            (
                (*b).to_owned(),
                Recorded {
                    parent: (*p).to_owned(),
                    base: None,
                },
            )
        })
        .collect()
}

#[test]
fn parses_parents_and_their_bases() {
    let out = "branch.api.gitcraft-parent\nschema\0branch.api.gitcraft-parent-base\nabc\0\
               branch.feat.x.y.gitcraft-parent\nmain\0branch.Ünï.gitcraft-parent\nfeat.x.y\0\
               branch.lonely.gitcraft-parent-base\ndef\0branch.empty.gitcraft-parent\n\0";
    let got = parse_recorded(out);
    assert_eq!(got.len(), 3, "{got:?}");
    assert_eq!(
        got.get("api"),
        Some(&Recorded {
            parent: "schema".into(),
            base: Some("abc".into())
        })
    );
    // Dots in the branch's name: the key's name is after the last one.
    assert_eq!(got.get("feat.x.y").map(|r| r.parent.as_str()), Some("main"));
    assert_eq!(got.get("Ünï").map(|r| r.parent.as_str()), Some("feat.x.y"));
    // A base without a parent, or an empty parent, is no layer.
    assert!(!got.contains_key("lonely") && !got.contains_key("empty"));
    assert!(parse_recorded("").is_empty());
}

#[test]
fn a_parent_round_trips_through_the_config() -> Result<(), String> {
    let repo = TestRepo::new("round trip ünïcode")?;
    let path = repo.path();
    assert!(recorded(path)?.is_empty());
    let name = "feat/ü-x";
    repo.branch(name, "main")?;
    repo.commit("a.txt", "a\n", "a")?;
    let base = repo.tip("main")?;
    repo.steps(&set_parent_ops(name, "main", &base))?;
    let got = recorded(path)?;
    let before = got.get(name).cloned();
    assert_eq!(
        before,
        Some(Recorded {
            parent: "main".into(),
            base: Some(base.clone())
        })
    );
    // Changed, then put back as it was before.
    repo.steps(&set_parent_ops(name, "other", "abc"))?;
    repo.steps(&restore_ops(name, before.as_ref()))?;
    assert_eq!(recorded(path)?.get(name), before.as_ref());
    // A recorded parent without its base is put back without one.
    let no_base = Recorded {
        parent: "main".into(),
        base: None,
    };
    repo.steps(&restore_ops(name, Some(&no_base)))?;
    assert_eq!(recorded(path)?.get(name), Some(&no_base));
    // Taken out, twice: the second time there is nothing to unset, and it still succeeds.
    repo.steps(&unset_parent_ops(name))?;
    repo.steps(&unset_parent_ops(name))?;
    assert!(recorded(path)?.is_empty());
    // The branch's other settings stay.
    repo.git(&["config", &config_key(name, "description"), "kept"])?;
    repo.steps(&set_parent_ops(name, "main", &base))?;
    repo.steps(&restore_ops(name, None))?;
    assert_eq!(
        repo.git(&["config", &config_key(name, "description")])?
            .trim(),
        "kept"
    );
    Ok(())
}

#[test]
fn a_parent_on_the_branch_itself_or_above_it_is_a_loop() {
    let mut recorded = rec(&[("schema", "main"), ("api", "schema"), ("ui", "api")]);
    assert!(would_loop(&recorded, "schema", "schema"));
    assert!(would_loop(&recorded, "schema", "ui"));
    assert!(would_loop(&recorded, "api", "ui"));
    assert!(!would_loop(&recorded, "ui", "schema"));
    assert!(!would_loop(&recorded, "schema", "main"));
    // A loop already in the config ends, and is no loop for a branch outside it.
    recorded.extend(rec(&[("main", "ui")]));
    assert!(!would_loop(&recorded, "x", "main"));
    assert!(would_loop(&recorded, "api", "main"));
}

#[test]
fn layers_count_their_own_commits_and_see_a_parent_that_moved() -> Result<(), String> {
    let repo = TestRepo::new("layers")?;
    repo.stack(&["schema", "api", "ui"])?;
    repo.checkout("api")?;
    repo.commit("api2.txt", "2\n", "api two")?;
    let path = repo.path();
    let got = layers(path, &recorded(path)?, &local(&repo)?)?;
    let by = |b: &str| {
        got.iter()
            .find(|l| l.branch == b)
            .cloned()
            .ok_or(format!("{b} is no layer"))
    };
    let api = by("api")?;
    assert_eq!((api.parent.as_str(), api.own, api.behind), ("schema", 2, 0));
    assert!(!api.needs_restack());
    let ui = by("ui")?;
    assert_eq!((ui.own, ui.behind), (1, 1));
    assert!(ui.needs_restack());
    assert!(!by("schema")?.needs_restack());
    // The parent deleted: still a layer, counted from where it was based.
    repo.checkout("main")?;
    repo.git(&["branch", "-D", "schema"])?;
    let got = layers(path, &recorded(path)?, &local(&repo)?)?;
    let api = got
        .iter()
        .find(|l| l.branch == "api")
        .ok_or("api is no layer")?;
    assert!(api.parent_missing && !api.needs_restack());
    assert_eq!(api.own, 2);
    // A layer whose own branch is gone is not one.
    assert!(got.iter().all(|l| l.branch != "schema"));
    Ok(())
}

#[test]
fn picks_the_branch_forked_from_last() {
    let c = |name: &str, own, behind, in_stack| Candidate {
        name: name.to_owned(),
        own,
        behind,
        in_stack,
    };
    let pick = |list: &[Candidate]| pick_parent(list).map(|c| c.name.clone());
    assert_eq!(pick(&[]), None);
    let list = [
        c("main", 3, 0, false),
        c("schema", 1, 0, true),
        c("other", 1, 4, false),
    ];
    assert_eq!(pick(&list).as_deref(), Some("schema"));
    // One that contains the branch is based on it, not its parent.
    let list = [c("main", 2, 0, false), c("ui", 0, 2, true)];
    assert_eq!(pick(&list).as_deref(), Some("main"));
    // At the same commit: a layer before a trunk, a branch before a trunk.
    let list = [c("main", 0, 0, false), c("api", 0, 0, true)];
    assert_eq!(pick(&list).as_deref(), Some("api"));
    let list = [c("main", 0, 0, false), c("topic", 0, 0, false)];
    assert_eq!(pick(&list).as_deref(), Some("topic"));
}

#[test]
fn suggests_the_parent_from_ancestry() -> Result<(), String> {
    let repo = TestRepo::new("suggest")?;
    let path = repo.path();
    repo.branch("schema", "main")?;
    repo.commit("s.txt", "s\n", "schema")?;
    repo.branch("api", "schema")?;
    repo.commit("a.txt", "a\n", "api")?;
    repo.commit("a2.txt", "a\n", "api 2")?;
    repo.branch("ui", "api")?;
    repo.commit("u.txt", "u\n", "ui")?;
    // Unrelated history: never suggested.
    repo.git(&["checkout", "-q", "--orphan", "lonely"])?;
    repo.git(&["rm", "-rfq", "."])?;
    repo.commit("l.txt", "l\n", "lonely")?;
    let none = BTreeMap::new();
    let suggest = |b: &str, r: &BTreeMap<String, Recorded>| suggest_parent(path, b, r);
    assert_eq!(suggest("ui", &none)?.as_deref(), Some("api"));
    assert_eq!(suggest("api", &none)?.as_deref(), Some("schema"));
    assert_eq!(suggest("schema", &none)?.as_deref(), Some("main"));
    assert_eq!(suggest("lonely", &none)?, None);
    // schema amended: both main and schema are three commits below api, and schema moved on
    // while main didn't, so main is the nearer.
    repo.checkout("schema")?;
    repo.git(&["commit", "-q", "--amend", "-m", "schema amended"])?;
    assert_eq!(suggest("api", &none)?.as_deref(), Some("main"));
    // A branch recorded as based on api is never api's parent.
    assert_eq!(
        suggest("api", &rec(&[("main", "api")]))?.as_deref(),
        Some("schema")
    );
    // A new branch with no commits yet: based on the one it was made from.
    repo.branch("ui-fix", "ui")?;
    assert_eq!(suggest("ui-fix", &none)?.as_deref(), Some("ui"));
    Ok(())
}

#[test]
fn the_fork_point_is_the_parents_commit_the_branch_is_on() -> Result<(), String> {
    let repo = TestRepo::new("fork point")?;
    repo.stack(&["schema", "api"])?;
    let schema = repo.tip("schema")?;
    assert_eq!(fork_point(repo.path(), "api", "schema")?, schema);
    repo.checkout("schema")?;
    repo.commit("s2.txt", "2\n", "schema 2")?;
    assert_eq!(fork_point(repo.path(), "api", "schema")?, schema);
    repo.git(&["checkout", "-q", "--orphan", "lonely"])?;
    repo.git(&["rm", "-rfq", "."])?;
    repo.commit("l.txt", "l\n", "lonely")?;
    let err = fork_point(repo.path(), "lonely", "main")
        .err()
        .unwrap_or_default();
    assert!(err.contains("no commit in common"), "{err}");
    Ok(())
}

#[test]
fn a_branch_made_from_the_branch_is_never_its_suggested_parent() -> Result<(), String> {
    // main ← A1 ← A2 (A) ← B1 (B, made from A); then A gets A3. Both moved on since they
    // forked, and B is nearer A than main is: only B's reflog says it was made from A.
    let repo = TestRepo::new("child not parent")?;
    repo.branch("A", "main")?;
    repo.commit("a1.txt", "1\n", "A1")?;
    repo.commit("a2.txt", "2\n", "A2")?;
    repo.branch("B", "A")?;
    repo.commit("b1.txt", "1\n", "B1")?;
    repo.checkout("A")?;
    repo.commit("a3.txt", "3\n", "A3")?;
    let none = BTreeMap::new();
    assert_eq!(
        suggest_parent(repo.path(), "A", &none)?.as_deref(),
        Some("main")
    );
    assert_eq!(
        suggest_parent(repo.path(), "B", &none)?.as_deref(),
        Some("A")
    );
    Ok(())
}

#[test]
fn a_loop_the_branch_is_not_part_of_is_no_loop_for_it() {
    let recorded = rec(&[("x", "y"), ("y", "x")]);
    assert!(!would_loop(&recorded, "a", "x"));
    assert!(would_loop(&recorded, "x", "y"));
}

#[test]
fn keys_set_more_than_once_by_hand_are_set_and_unset() -> Result<(), String> {
    let repo = TestRepo::new("multi valued")?;
    repo.stack(&["api"])?;
    let key = config_key("api", PARENT_KEY);
    repo.git(&["config", "--local", "--add", &key, "other"])?;
    repo.steps(&set_parent_ops("api", "main", &repo.tip("main")?))?;
    assert_eq!(
        recorded(repo.path())?.get("api").map(|r| r.parent.as_str()),
        Some("main")
    );
    repo.git(&["config", "--local", "--add", &key, "other"])?;
    repo.steps(&unset_parent_ops("api"))?;
    assert!(recorded(repo.path())?.is_empty());
    Ok(())
}

#[test]
fn renaming_a_parent_takes_its_layers_along() -> Result<(), String> {
    let repo = TestRepo::new("rename parent")?;
    repo.stack(&["schema", "api", "ui"])?;
    let kids = children(&recorded(repo.path())?, "schema");
    assert_eq!(kids, ["api"]);
    let rename = |from: &str, to: &str| -> Result<(), String> {
        repo.git(&["branch", "-m", from, to])?;
        repo.steps(&reparent_ops(&kids, to))
    };
    rename("schema", "db-schema")?;
    let got = recorded(repo.path())?;
    assert_eq!(got.get("api").map(|r| r.parent.as_str()), Some("db-schema"));
    // The renamed branch keeps its own parent (git moves its config along).
    assert_eq!(
        got.get("db-schema").map(|r| r.parent.as_str()),
        Some("main")
    );
    rename("db-schema", "schema")?;
    assert_eq!(
        recorded(repo.path())?.get("api").map(|r| r.parent.as_str()),
        Some("schema")
    );
    Ok(())
}
