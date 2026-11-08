use super::*;
#[test]
fn verify_pack_parser_keeps_actual_delta_numbers() -> Result<(), String> {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let rows = pack_rows(&format!(
        "{a} blob 9000 22 12 1 {b}\n{b} blob 8990 38 34\nnon delta: 1 object\n"
    ))?;
    assert_eq!(rows[&a].disk, 22);
    assert_eq!(rows[&a].base.as_ref(), Some(&b));
    assert_eq!(rows[&a].depth, 1);
    assert_eq!(rows[&b].offset, 34);
    assert!(pack_rows(&format!("{a} blob 2 3 4 1 bad\n")).is_err());
    Ok(())
}

#[test]
fn inspect_loose_packed_tree_and_file_version() -> Result<(), String> {
    let r = crate::git::fixture::Repo::new("object-inspector")?;
    r.commit("one", &[("hello.txt", Some("hello\n"))])?;
    let file = inspect(r.path(), "HEAD:hello.txt", &|| true)?;
    assert_eq!(file.object.as_ref().map(|o| o.kind.as_str()), Some("blob"));
    assert_eq!(file.object.as_ref().map(|o| o.size), Some(6));
    assert!(file.representations.iter().any(|p| p.loose_bytes.is_some()));
    let tree = inspect(r.path(), "HEAD^{tree}", &|| true)?;
    assert_eq!(tree.entries[0].name, "hello.txt");
    assert_eq!(
        tree.entries[0].oid,
        file.object.as_ref().ok_or("no blob")?.oid
    );
    r.git(&["gc", "--prune=now"])?;
    let file = inspect(r.path(), "HEAD:hello.txt", &|| true)?;
    assert!(file.representations.iter().any(|p| !p.chain.is_empty()));
    assert!(inspect(r.path(), "HEAD", &|| false).is_err());
    assert!(inspect(r.path(), "--help", &|| true).is_err());
    Ok(())
}
