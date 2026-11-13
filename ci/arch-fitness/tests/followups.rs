//! Regressions from the second review of #215: each case once measured or rewrote wrongly.

mod support;

use std::collections::BTreeMap;

use gitcraft_arch_fitness::ratchet;
use gitcraft_arch_fitness::{check, syntax};
use support::{LIB, adopted, analysis, has_edge, outcome, policy, rules, tree};

fn mutations(alpha: &str) -> usize {
    analysis(&tree(&[("src/alpha.rs", alpha)]))
        .repo_tab
        .cross_mutations
        .len()
}

#[test]
fn constant_cfg_predicates_hold_without_known_flags() {
    let active = |attr: &str| {
        let item: syn::ItemFn =
            syn::parse_str(&format!("{attr} fn f() {{}}")).unwrap_or_else(|e| unreachable!("{e}"));
        syntax::cfg_active(&item.attrs, &BTreeMap::new())
    };
    assert!(!active("#[cfg(any())]"));
    assert!(!active("#[cfg(not(all()))]"));
    assert!(active("#[cfg(all())]"));
    assert!(active("#[cfg(windows)]"));
}

#[test]
fn unstable_sorts_by_key_or_comparator_mutate() {
    for call in [
        "sort_unstable_by(|a, b| a.cmp(b))",
        "sort_unstable_by_key(|x| *x)",
    ] {
        let source =
            format!("pub struct State; pub fn f(t: &mut crate::hub::Tab) {{ t.list.{call}; }}");
        assert_eq!(mutations(&source), 1, "{call}");
    }
}

#[test]
fn block_scoped_imports_name_the_tab_type() {
    let source = "pub struct State; pub fn f(tabs: &mut [crate::hub::Tab]) { \
        use crate::hub::Tab as T; let t: &mut T = &mut tabs[0]; t.status.clear(); }";
    assert_eq!(mutations(source), 1);
}

#[test]
fn block_scoped_value_imports_do_not_hide_the_tab_type() {
    let source = "use crate::hub::Tab as T; pub struct State; pub fn f(tabs: &mut [T]) { \
        use crate::beta::go as T; let t: &mut T = &mut tabs[0]; t.status.clear(); }";
    assert_eq!(mutations(source), 1);
}

#[test]
fn cfg_and_test_attributes_of_other_items_scope_their_references() {
    let test_only = [
        "pub struct State; #[cfg(test)] const FIXTURE: crate::beta::Thing = crate::beta::Thing;",
        "pub struct State; impl State { #[cfg(test)] m!(crate::beta::go()); }",
    ];
    for source in test_only {
        let a = analysis(&tree(&[("src/alpha.rs", source)]));
        assert!(!has_edge(&a, "alpha", "beta"), "{source}");
        let test_edge = a
            .test_edges
            .iter()
            .any(|e| e.from == "alpha" && e.to == "beta");
        assert!(test_edge, "{source}");
    }
    let windows = "pub struct State; #[cfg(windows)] pub struct Win(crate::beta::Thing);";
    let a = analysis(&tree(&[("src/alpha.rs", windows)]));
    let edge = a.edges.iter().find(|e| e.from == "alpha" && e.to == "beta");
    assert!(
        edge.is_some_and(|e| e.samples[0].cfg == ["windows"]),
        "{edge:?}"
    );
}

#[test]
fn exported_macros_resolve_at_the_crate_root() {
    let a = analysis(&tree(&[
        (
            "src/beta.rs",
            "pub struct Thing; #[macro_export] macro_rules! mk { () => {} }",
        ),
        (
            "src/alpha.rs",
            "pub struct State; pub fn f() { crate::mk!(); }",
        ),
    ]));
    assert!(has_edge(&a, "alpha", "beta"), "{:?}", a.edges);
}

#[test]
fn baseline_rewrites_are_refused_while_modules_are_unmapped() {
    let lib = format!("{LIB} mod stray;");
    let files = tree(&[("src/lib.rs", &lib), ("src/stray.rs", "pub fn lost() {}")]);
    let out = outcome(&files, &adopted(&files));
    assert_eq!(rules(&out), ["unmapped-module"], "{:#?}", out.violations);
    assert!(ratchet::refuse_with_gaps(&out).is_err());
}

#[test]
fn a_symbol_budgeted_domain_without_a_symbol_limit_is_a_violation() {
    let files = tree(&[]);
    let mut baseline = adopted(&files);
    assert!(outcome(&files, &baseline).violations.is_empty());
    if let Some(budget) = baseline.budgets.get_mut("alpha") {
        budget.symbols = None;
    }
    let out = outcome(&files, &baseline);
    assert_eq!(rules(&out), ["fan-out-budget"], "{:#?}", out.violations);
    assert!(out.violations[0].message.contains("no symbol limit"));
}

#[test]
fn enforcing_test_code_is_rejected_until_it_is_implemented() {
    let mut policy = policy();
    policy.enforce_tests = true;
    let files = tree(&[]);
    let out = check(&analysis(&files), &policy, &adopted(&files));
    assert_eq!(rules(&out), ["policy"], "{:#?}", out.violations);
}
