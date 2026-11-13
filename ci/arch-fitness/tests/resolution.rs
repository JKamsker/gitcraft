//! Regression fixtures: a dependency must survive every way of spelling or hiding it.

mod support;

use support::{analysis, edge_symbols, has_edge, tree};

#[test]
fn baseline_fixture_has_only_allowed_edges() {
    let a = analysis(&tree(&[]));
    assert!(has_edge(&a, "alpha", "base"));
    assert!(has_edge(&a, "hub", "alpha"));
    assert!(!has_edge(&a, "alpha", "beta"));
    assert!(
        a.edges.iter().all(|e| e.policy != "forbidden"),
        "{:?}",
        a.edges
    );
}

#[test]
fn qualified_paths() {
    let a = analysis(&tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { crate::beta::go(); }",
    )]));
    assert_eq!(edge_symbols(&a, "alpha", "beta"), ["fx::beta::go"]);
    let a = analysis(&tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { super::beta::Thing::new(); }",
    )]));
    assert_eq!(edge_symbols(&a, "alpha", "beta"), ["fx::beta::Thing"]);
}

#[test]
fn aliases_module_and_block_imports() {
    let renamed =
        "use crate::beta::Thing as Other; pub struct State; pub fn run() { Other::new(); }";
    assert!(has_edge(
        &analysis(&tree(&[("src/alpha.rs", renamed)])),
        "alpha",
        "beta"
    ));
    let module = "use crate::beta as b; pub struct State; pub fn run() { b::go(); }";
    assert_eq!(
        edge_symbols(
            &analysis(&tree(&[("src/alpha.rs", module)])),
            "alpha",
            "beta"
        )
        .len(),
        2
    );
    let block = "pub struct State; pub fn run() { use crate::beta::go as start; start(); }";
    assert!(has_edge(
        &analysis(&tree(&[("src/alpha.rs", block)])),
        "alpha",
        "beta"
    ));
    let alias = "pub struct State; type T = crate::beta::Thing; pub fn run() -> T { T::new() }";
    assert!(has_edge(
        &analysis(&tree(&[("src/alpha.rs", alias)])),
        "alpha",
        "beta"
    ));
}

#[test]
fn helper_indirection_keeps_or_moves_the_dependency() {
    // A helper module inside the same domain does not hide the edge.
    let inside = tree(&[
        (
            "src/alpha.rs",
            "mod helper; pub struct State; pub fn run() { helper::call(); }",
        ),
        (
            "src/alpha/helper.rs",
            "pub fn call() { crate::beta::go(); }",
        ),
    ]);
    assert!(has_edge(&analysis(&inside), "alpha", "beta"));
    // Moving the call into a shared domain makes that domain depend upward instead.
    let moved = tree(&[
        (
            "src/alpha.rs",
            "pub struct State; pub fn run() { crate::base::call(); }",
        ),
        (
            "src/base.rs",
            "pub fn util() {} pub fn call() { crate::beta::go(); }",
        ),
    ]);
    let a = analysis(&moved);
    assert!(!has_edge(&a, "alpha", "beta"));
    assert!(
        a.edges
            .iter()
            .any(|e| e.from == "base" && e.to == "beta" && e.policy == "forbidden")
    );
}

#[test]
fn reexports_resolve_to_the_owner_and_are_reported() {
    let files = tree(&[
        (
            "src/base.rs",
            "pub fn util() {} pub(crate) use crate::beta::Thing;",
        ),
        (
            "src/alpha.rs",
            "pub struct State; pub fn run() { crate::base::Thing::new(); }",
        ),
    ]);
    let a = analysis(&files);
    assert_eq!(edge_symbols(&a, "alpha", "beta"), ["fx::beta::Thing"]);
    assert_eq!(a.reexports.len(), 1);
    assert_eq!(
        (
            a.reexports[0].domain.as_str(),
            a.reexports[0].target_domain.as_str()
        ),
        ("base", "beta")
    );
    let glob = tree(&[
        (
            "src/base.rs",
            "pub fn util() {} pub(crate) use crate::beta::*;",
        ),
        (
            "src/alpha.rs",
            "use crate::base::go; pub struct State; pub fn run() { go(); }",
        ),
    ]);
    assert_eq!(
        edge_symbols(&analysis(&glob), "alpha", "beta"),
        ["fx::beta::go"]
    );
}

#[test]
fn glob_imports_respect_privacy() {
    let files = tree(&[
        (
            "src/beta.rs",
            "pub struct Thing; fn go() {} pub mod inner { pub fn go() {} }",
        ),
        (
            "src/alpha.rs",
            "use crate::beta::*; pub struct State; pub fn run() { go(); inner::go(); }",
        ),
    ]);
    // The private `beta::go` is not imported by the glob; `inner::go` is.
    assert_eq!(
        edge_symbols(&analysis(&files), "alpha", "beta"),
        ["fx::beta", "fx::beta::inner::go"]
    );
    let child = tree(&[(
        "src/beta.rs",
        "fn secret() {} pub struct Thing; mod kid { use super::*; fn f() { secret(); } }",
    )]);
    assert_eq!(analysis(&child).summary.stats.names_unresolved, 0);
}

#[test]
fn path_attribute_and_inline_modules() {
    let files = tree(&[
        (
            "src/lib.rs",
            "mod base; mod peer; mod other; #[path = \"features/a.rs\"] mod alpha; mod beta; pub mod hub;",
        ),
        (
            "src/features/a.rs",
            "mod deep; pub struct State; pub mod inline { #[path = \"x.rs\"] pub mod x; }",
        ),
        ("src/features/deep.rs", "fn f() { crate::beta::go(); }"),
        (
            "src/features/inline/x.rs",
            "fn g() { crate::beta::Thing::new(); }",
        ),
    ]);
    let mut files = files;
    files.remove("src/alpha.rs");
    let a = analysis(&files);
    assert_eq!(
        edge_symbols(&a, "alpha", "beta"),
        ["fx::beta::Thing", "fx::beta::go"]
    );
    assert!(a.gaps.items.is_empty(), "{:?}", a.gaps.items);
}

#[test]
fn cfg_variants_are_all_analyzed_and_labelled() {
    let files = tree(&[
        (
            "src/alpha.rs",
            "pub struct State; #[cfg(windows)] mod imp; #[cfg(unix)] #[path = \"alpha/unix.rs\"] mod imp;",
        ),
        ("src/alpha/imp.rs", "fn f() { crate::beta::go(); }"),
        ("src/alpha/unix.rs", "fn f() { crate::peer::help(); }"),
    ]);
    let a = analysis(&files);
    let edge = a.edges.iter().find(|e| e.from == "alpha" && e.to == "beta");
    assert!(
        edge.is_some_and(|e| e.samples[0].cfg == ["windows"]),
        "{edge:?}"
    );
    assert!(has_edge(&a, "alpha", "peer"));
    // Test-only code is inventoried separately, and flags the crate fixes are respected.
    let test_only = tree(&[(
        "src/alpha.rs",
        "pub struct State; #[cfg(test)] mod tests { fn t() { crate::beta::go(); } } #[cfg(recording)] fn r() { crate::other::more(); }",
    )]);
    let a = analysis(&test_only);
    assert!(!has_edge(&a, "alpha", "beta") && !has_edge(&a, "alpha", "other"));
    assert!(
        a.test_edges
            .iter()
            .any(|e| e.from == "alpha" && e.to == "beta")
    );
}

#[test]
fn types_impls_macros_and_includes() {
    let cases = [
        "pub struct State(crate::beta::Thing);",
        "pub struct State; impl From<crate::beta::Thing> for State { fn from(_: crate::beta::Thing) -> Self { State } }",
        "pub struct State; fn f() { let _ = format!(\"{}\", crate::beta::Thing::new().0); }",
        "pub struct State; fn f(x: u8) -> bool { matches!(x, _ if crate::beta::ok()) }",
        "pub struct State; include!(\"alpha/more.rs\");",
    ];
    for case in cases {
        let files = tree(&[
            ("src/alpha.rs", case),
            ("src/alpha/more.rs", "fn more() { crate::beta::go(); }"),
            (
                "src/beta.rs",
                "pub struct Thing(pub u8); impl Thing { pub fn new() -> Self { Thing(0) } } pub fn go() {} pub fn ok() -> bool { true }",
            ),
        ]);
        assert!(has_edge(&analysis(&files), "alpha", "beta"), "{case}");
    }
}

#[test]
fn locals_and_generics_shadow_module_names() {
    let files = tree(&[(
        "src/alpha.rs",
        "use crate::beta; pub struct State; fn f<Thing>(beta: u8) -> u8 { let go = beta; go } fn g(_: Thing2) {} struct Thing2;",
    )]);
    let a = analysis(&files);
    // Only the `use` itself remains: the value `beta` and generic `Thing` are locals.
    assert_eq!(
        a.edges
            .iter()
            .find(|e| e.from == "alpha" && e.to == "beta")
            .map(|e| e.references),
        Some(1)
    );
}

#[test]
fn ambiguity_and_gaps_are_reported() {
    let files = tree(&[
        (
            "src/alpha.rs",
            "pub struct State; mod missing; fn f() { crate::nowhere::x(); }",
        ),
        ("src/stray.rs", "fn lost() {}"),
    ]);
    let a = analysis(&files);
    let kinds: Vec<&str> = a.gaps.items.iter().map(|g| g.kind.as_str()).collect();
    assert!(
        kinds.contains(&"missing-module-file") && kinds.contains(&"unreached-file"),
        "{kinds:?}"
    );
    assert_eq!(a.gaps.unresolved_paths[0].detail, "crate::nowhere::x");
}

#[test]
fn reports_are_deterministic() {
    let files = tree(&[(
        "src/alpha.rs",
        "pub struct State; fn f() { crate::beta::go(); crate::peer::help(); }",
    )]);
    let one = serde_json::to_string(&analysis(&files)).ok();
    let two = serde_json::to_string(&analysis(&files)).ok();
    assert!(one.is_some() && one == two);
}
