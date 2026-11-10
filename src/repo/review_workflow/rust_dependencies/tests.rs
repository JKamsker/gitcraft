use super::*;
#[test]
fn unchanged_rust_callers_are_invalidated_by_direct_and_transitive_callee_changes()
-> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("rust-freshness")?;
    let caller = "use crate::errors::result; pub fn caller() -> i32 { result() }\n";
    r.commit("src/caller.rs", caller, "caller")?;
    r.commit(
        "src/errors.rs",
        "pub fn result() -> i32 { leaf() } pub fn leaf() -> i32 { 1 }\n",
        "old behavior",
    )?;
    let before = r.tip("HEAD")?;
    let after = r.commit(
        "src/errors.rs",
        "pub fn result() -> i32 { leaf() } pub fn leaf() -> i32 { 2 }\n",
        "new behavior",
    )?;
    let report = compare(
        &load(r.path(), &before)?,
        &load(r.path(), &after)?,
        "src/caller.rs",
    );
    assert_eq!(report.checked, 1);
    assert!(
        report
            .witnesses
            .iter()
            .any(|s| s.contains("root::caller::caller") && s.contains("root::errors::leaf"))
    );
    assert!(
        compare(
            &load(r.path(), &before)?,
            &load(r.path(), &before)?,
            "src/caller.rs"
        )
        .witnesses
        .is_empty()
    );
    Ok(())
}
#[test]
fn identical_function_names_in_other_modules_are_not_dependencies() -> Result<(), String> {
    let mut a = Index::default();
    let mut b = Index::default();
    let old=syn::parse_file("mod a { pub fn leaf()->i32 {1} pub fn caller()->i32 {leaf()} } mod b {pub fn leaf()->i32 {1}} ").map_err(|e| e.to_string())?;
    let new=syn::parse_file("mod a { pub fn leaf()->i32 {1} pub fn caller()->i32 {leaf()} } mod b {pub fn leaf()->i32 {2}} ").map_err(|e| e.to_string())?;
    scan(&mut a, "src/lib.rs", &old.items, &["root".into()]);
    scan(&mut b, "src/lib.rs", &new.items, &["root".into()]);
    assert_eq!(compare(&a, &b, "src/lib.rs").checked, 2);
    assert!(compare(&a, &b, "src/lib.rs").witnesses.is_empty());
    Ok(())
}
#[test]
fn imported_call_target_changes_and_opaque_calls_are_explicit() -> Result<(), String> {
    fn index(source: &str) -> Result<Index, String> {
        let mut index = Index::default();
        let source = syn::parse_file(source).map_err(|e| e.to_string())?;
        scan(&mut index, "src/lib.rs", &source.items, &["root".into()]);
        Ok(index)
    }
    let before = index(
        "mod a {pub fn value()->i32 {1}} mod b {pub fn value()->i32 {2}} use crate::a::value; pub fn caller()->i32 {value()} pub fn opaque(x: String) {x.len();}",
    )?;
    let after = index(
        "mod a {pub fn value()->i32 {1}} mod b {pub fn value()->i32 {2}} use crate::b::value; pub fn caller()->i32 {value()} pub fn opaque(x: String) {x.len();}",
    )?;
    let report = compare(&before, &after, "src/lib.rs");
    assert!(
        report
            .witnesses
            .iter()
            .any(|s| s.contains("static call targets changed"))
    );
    assert!(report.limitations.iter().any(|s| s.contains("Dynamic")));
    Ok(())
}
#[test]
fn local_bindings_are_not_mistaken_for_repository_functions() -> Result<(), String> {
    let mut before = Index::default();
    let mut after = Index::default();
    let old = syn::parse_file("fn leaf()->i32 {1} fn caller(leaf: fn()->i32)->i32 {leaf()}")
        .map_err(|e| e.to_string())?;
    let new = syn::parse_file("fn leaf()->i32 {2} fn caller(leaf: fn()->i32)->i32 {leaf()}")
        .map_err(|e| e.to_string())?;
    scan(&mut before, "src/lib.rs", &old.items, &["root".into()]);
    scan(&mut after, "src/lib.rs", &new.items, &["root".into()]);
    let report = compare(&before, &after, "src/lib.rs");
    assert!(report.witnesses.is_empty());
    assert!(!report.limitations.is_empty());
    Ok(())
}
