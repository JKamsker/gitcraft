//! Real local fetches verify CLI/native catalogs without contacting a forge.
use super::*;
use crate::git::fixture::Repo;

#[test]
fn fetched_catalog_keeps_equal_tips_and_literal_head_branches() -> Result<(), String> {
    let source = Repo::new("233-source")?;
    source.commit("base", &[("file", Some("hello"))])?;
    source.git(&["branch", "topic"])?;
    source.git(&["branch", "nested/HEAD"])?;
    let local = Repo::new("233-local")?;
    let bare_root = tempfile::tempdir().map_err(|e| e.to_string())?;
    for alias in ["origin", "alice", "my/fork"] {
        let bare = bare_root.path().join(alias.replace('/', "-"));
        let bare = bare.to_str().ok_or("non-UTF8 fixture")?;
        source.git(&[
            "clone",
            "--bare",
            source.path().to_str().ok_or("non-UTF8 source")?,
            bare,
        ])?;
        local.git(&["remote", "add", alias, bare])?;
        local.git(&["fetch", alias])?;
    }
    local.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ])?;
    local.git(&[
        "symbolic-ref",
        "refs/remotes/alice/default",
        "refs/remotes/alice/main",
    ])?;
    let cli = super::super::refs::refs_cli(local.path())?;
    assert_eq!(native::refs(local.path())?, cli);
    assert_eq!(cli.remotes.len(), 3);
    for (alias, branches) in cli.remotes {
        assert_eq!(branches.len(), 3);
        assert!(branches.iter().any(|b| b.name == format!("{alias}/topic")));
        assert!(
            branches
                .iter()
                .any(|b| b.name == format!("{alias}/nested/HEAD"))
        );
        assert!(branches.iter().all(|b| b.target == branches[0].target));
    }
    Ok(())
}
