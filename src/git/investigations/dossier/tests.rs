use super::*;
use crate::git::investigations::tests::Repo;
fn commit(r: &Repo, path: &str, body: &str, time: i64) -> Result<String, String> {
    std::fs::write(r.0.join(path), body).map_err(|e| e.to_string())?;
    r.git(&["add", "--", path])?;
    r.commit("line@test.invalid", time)
}
#[test]
fn repeated_blame_follows_actual_versions_renames_and_formatting_without_index_or_work_writes()
-> Result<(), String> {
    let r = Repo::new()?;
    let first = commit(
        &r,
        "old.rs",
        "before\nlet number = 1;\nafter\n",
        1_700_000_000,
    )?;
    let semantic = commit(
        &r,
        "old.rs",
        "before\nlet number = 2;\nafter\n",
        1_700_000_001,
    )?;
    let path = "-new ü '$(literal).rs";
    r.git(&["mv", "--", "old.rs", path])?;
    let rename = r.commit("line@test.invalid", 1_700_000_002)?;
    let formatting = commit(&r, path, "before\nlet   number=2;\nafter\n", 1_700_000_003)?;
    let head = commit(
        &r,
        path,
        "prefix\nbefore\nlet   number=2;\nafter\n",
        1_700_000_004,
    )?;
    let refs = r.git(&["show-ref"])?;
    let index = std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?;
    let work = std::fs::read(r.0.join(path)).map_err(|e| e.to_string())?;
    let result = trace(&r.0, &head, path, 3, &|| true)?;
    let ids: BTreeSet<_> = result.cards.iter().map(|c| c.at.commit.as_str()).collect();
    for id in [&first, &semantic, &rename, &formatting] {
        assert!(
            ids.contains(id.as_str()),
            "Missing actual line version {id}"
        );
    }
    assert_eq!(result.start.line, 3);
    assert!(
        result
            .cards
            .iter()
            .any(|c| c.at.commit == rename && c.at.path == path)
    );
    assert!(
        result
            .cards
            .iter()
            .any(|c| c.at.commit == first && c.text == "let number = 1;")
    );
    assert!(
        result
            .cards
            .iter()
            .any(|c| c.at.commit == formatting && c.whitespace_only)
    );
    assert!(
        result
            .cards
            .iter()
            .all(|c| c.attribution.commit == first || c.attribution.commit == semantic)
    );
    assert!(result.stopped.contains("root"));
    assert_eq!(refs, r.git(&["show-ref"])?);
    assert_eq!(
        index,
        std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?
    );
    assert_eq!(
        work,
        std::fs::read(r.0.join(path)).map_err(|e| e.to_string())?
    );
    Ok(())
}
#[test]
fn unequal_replacement_stops_without_guessing_and_merge_parents_remain_explicit()
-> Result<(), String> {
    let r = Repo::new()?;
    commit(&r, "f", "left\nalpha\nbeta\nright\n", 1_700_000_000)?;
    let head = commit(&r, "f", "left\ncombined\nright\n", 1_700_000_001)?;
    let result = trace(&r.0, &head, "f", 2, &|| true)?;
    assert_eq!(result.cards.len(), 1);
    assert!(result.stopped.contains("ambiguous"));
    r.git(&["branch", "feature"])?;
    commit(&r, "f", "left\nmain line\nright\n", 1_700_000_002)?;
    r.git(&["checkout", "-q", "feature"])?;
    commit(&r, "f", "left\nbranch line\nright\n", 1_700_000_003)?;
    r.git(&["checkout", "-q", "main"])?;
    assert!(
        r.git(&["merge", "--no-ff", "--no-commit", "feature"])
            .is_err()
    );
    r.git(&["rev-parse", "MERGE_HEAD"])?;
    let merge = commit(&r, "f", "left\nresolved line\nright\n", 1_700_000_004)?;
    let result = trace(&r.0, &merge, "f", 2, &|| true)?;
    assert_eq!(result.alternatives.len(), 2);
    assert!(result.stopped.contains("Merge boundary"));
    assert!(
        result
            .alternatives
            .iter()
            .all(|s| s.line == 2 && oid(&s.commit))
    );
    Ok(())
}
#[test]
fn partial_copy_uses_verified_blame_origin_when_whole_file_similarity_is_low() -> Result<(), String>
{
    let r = Repo::new()?;
    let remembered = "let copied_calculation_with_enough_distinctive_characters = expensive_algorithm(original_input, options);";
    let mut original = (0..100)
        .map(|i| format!("unrelated original source row {i} with distinct filler\n"))
        .collect::<Vec<_>>();
    original[50] = format!("{remembered}\n");
    let source = commit(&r, "source.rs", &original.concat(), 1_700_000_000)?;
    let body = format!("fresh prefix\n{remembered}\nfresh suffix\n");
    let copy = commit(&r, "copy.rs", &body, 1_700_000_001)?;
    let result = trace(&r.0, &copy, "copy.rs", 2, &|| true)?;
    assert!(
        result
            .cards
            .iter()
            .any(|c| c.at.commit == source && c.at.path == "source.rs" && c.at.line == 51)
    );
    assert!(
        result
            .cards
            .iter()
            .any(|c| c.at.commit == copy && c.note.contains("copy/move"))
    );
    Ok(())
}
#[test]
fn parsers_refuse_malformed_ids_quoted_paths_and_ambiguous_duplicate_lines() -> Result<(), String> {
    assert!(blame_frame(b"--all 1 1 1\nfilename f\n\tx\n").is_err());
    assert!(quoted_path("\"..\\057escape\"").is_err());
    assert_eq!(quoted_path("\"caf\\303\\251\\tline\"")?, "café\tline");
    assert!(quoted_path("\"truncated\\3\"").is_err());
    assert_eq!(previous_line("x\ny\nx\n", "x\nz\nz\nx\n", 2)?, None);
    assert_eq!(previous_line("red\nred\n", "blue\nblue\n", 1)?, None);
    assert_eq!(
        previous_line("x\nx\n", "x\nx\n", 2)?.map(|(line, _)| line),
        Some(2)
    );
    assert!(!path_ok("../x"));
    assert!(!path_ok(".git/config"));
    assert!(path_ok("-f ü '$(literal)"));
    assert!(
        trace(
            Path::new("/recording/nonexistent"),
            "--all",
            "f",
            1,
            &|| true
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn binary_large_nonregular_and_cancelled_sources_fail_explicitly() -> Result<(), String> {
    let r = Repo::new()?;
    let head = commit(&r, "binary", "a\0b", 1_700_000_000)?;
    assert!(trace(&r.0, &head, "binary", 1, &|| true).is_err());
    assert!(trace(&r.0, &head, "binary", 1, &|| false).is_err());
    let head = commit(&r, "large", &"x".repeat(FILE + 1), 1_700_000_001)?;
    assert!(trace(&r.0, &head, "large", 1, &|| true).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/dev/zero", r.0.join("link")).map_err(|e| e.to_string())?;
        r.git(&["add", "link"])?;
        let head = r.commit("line@test.invalid", 1_700_000_002)?;
        assert!(trace(&r.0, &head, "link", 1, &|| true).is_err());
    }
    Ok(())
}
