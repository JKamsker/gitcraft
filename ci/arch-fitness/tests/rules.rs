//! Negative controls: each rule fails on a deliberate violation and is green once reverted.

mod support;

use gitcraft_arch_fitness::policy::{AllowedEdge, Budget};
use gitcraft_arch_fitness::{check, ratchet};
use support::{Files, adopted, analysis, outcome, policy, rules, tree};

/// Green on `clean`, exactly `rule` on `broken`, green again on `clean`. Budgets are
/// relaxed unless the rule is a budget, so that each control isolates one rule.
fn control(clean: &Files, broken: &Files, rule: &str) {
    let mut baseline = adopted(clean);
    if !rule.ends_with("-budget") {
        for budget in baseline.budgets.values_mut() {
            *budget = Budget {
                domains: 99,
                symbols: Some(99),
                exported: 99,
            };
        }
    }
    assert!(
        outcome(clean, &baseline).violations.is_empty(),
        "{:?}",
        outcome(clean, &baseline)
    );
    let failed = outcome(broken, &baseline);
    assert_eq!(rules(&failed), [rule], "{:#?}", failed.violations);
    assert!(failed.violations.iter().all(|v| !v.fix.is_empty()));
    assert!(outcome(clean, &baseline).violations.is_empty());
}

#[test]
fn forbidden_edge() {
    let broken = tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { crate::base::util(); crate::beta::go(); }",
    )]);
    control(&tree(&[]), &broken, "forbidden-edge");
    let failed = outcome(&broken, &adopted(&tree(&[])));
    let v = failed
        .violations
        .iter()
        .find(|v| v.rule == "forbidden-edge")
        .unwrap_or(&failed.violations[0]);
    assert_eq!(v.location.as_deref(), Some("src/alpha.rs:1"));
    assert!(
        v.symbol
            .as_deref()
            .is_some_and(|s| s.contains("fx::beta::go")),
        "{v:?}"
    );
    assert!(
        v.message.contains("alpha (features) -> beta (features)"),
        "{}",
        v.message
    );
}

#[test]
fn new_domain_cycle() {
    let clean = tree(&[("src/base.rs", "pub fn util() { crate::peer::help(); }")]);
    let broken = tree(&[
        ("src/base.rs", "pub fn util() { crate::peer::help(); }"),
        ("src/peer.rs", "pub fn help() { crate::base::util(); }"),
    ]);
    control(&clean, &broken, "domain-cycle");
}

#[test]
fn new_edge_inside_existing_cycle() {
    let cycle = [
        ("src/base.rs", "pub fn util() { crate::peer::help(); }"),
        ("src/peer.rs", "pub fn help() { crate::other::more(); }"),
        ("src/other.rs", "pub fn more() { crate::base::util(); }"),
    ];
    let mut deeper = cycle;
    deeper[0].1 = "pub fn util() { crate::peer::help(); crate::other::more(); }";
    control(&tree(&cycle), &tree(&deeper), "cycle-edge");
}

#[test]
fn cross_feature_tab_mutation() {
    let write = |body: &str| {
        tree(&[(
            "src/alpha.rs",
            &*format!(
                "pub struct State; impl crate::hub::Tab {{ fn act(&mut self) {{ {body} }} }}"
            ),
        )])
    };
    let clean = write("let _ = self.status.len(); self.alpha = State;");
    for broken in [
        "self.status = String::new();",
        "self.list.push(1);",
        "let s = &mut self.status; s.clear();",
        "self.status += \"x\";",
    ] {
        control(&clean, &write(broken), "repo-tab-mutation");
    }
}

#[test]
fn tab_field_attribution_follows_aliases_and_bindings() {
    let cases = [
        "use crate::hub::Tab as T; pub struct State; fn f(t: &mut T) { t.status.clear(); }",
        "pub struct State; fn f() { let mut t = crate::hub::Tab::new(); t.status = String::new(); }",
        "pub struct State; fn f(v: &mut Vec<crate::hub::Tab>) { v.iter_mut().for_each(|t: &mut crate::hub::Tab| t.list.clear()); }",
    ];
    for case in cases {
        let a = analysis(&tree(&[("src/alpha.rs", case)]));
        assert_eq!(
            a.repo_tab.cross_mutations.len(),
            1,
            "{case}: {:?}",
            a.repo_tab.cross_mutations
        );
    }
    // Reads through a shared reference and the owner's own field are not cross-feature writes.
    let read = analysis(&tree(&[(
        "src/alpha.rs",
        "pub struct State; fn f(t: &crate::hub::Tab) -> usize { t.status.len() }",
    )]));
    assert!(read.repo_tab.cross_mutations.is_empty());
    let alpha = read
        .repo_tab
        .fields
        .iter()
        .find(|f| f.name == "alpha")
        .map(|f| f.owner.as_str());
    assert_eq!(
        alpha,
        Some("alpha"),
        "a field whose type a feature defines belongs to it"
    );
    assert_eq!(
        read.repo_tab
            .fields
            .iter()
            .find(|f| f.name == "status")
            .map(|f| f.readers.len()),
        Some(1)
    );
}

#[test]
fn visibility_reexport() {
    let broken = tree(&[(
        "src/base.rs",
        "pub fn util() {} pub(crate) use crate::peer::help;",
    )]);
    control(&tree(&[]), &broken, "visibility-reexport");
}

#[test]
fn visibility_budget() {
    let clean = tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { crate::base::util(); } fn private() {}",
    )]);
    let broken = tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { crate::base::util(); } pub(crate) fn private() {}",
    )]);
    control(&clean, &broken, "visibility-budget");
    let v = &outcome(&broken, &adopted(&clean)).violations[0];
    assert!(
        v.symbol
            .as_deref()
            .is_some_and(|s| s.contains("fx::alpha::private")),
        "{v:?}"
    );
}

#[test]
fn fan_out_budget() {
    let broken = tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { crate::base::util(); crate::peer::help(); }",
    )]);
    control(&tree(&[]), &broken, "fan-out-budget");
}

#[test]
fn unmapped_module_and_unreached_file() {
    let unmapped = tree(&[
        ("src/lib.rs", &*format!("{} mod gamma;", support::LIB)),
        ("src/gamma.rs", ""),
    ]);
    control(&tree(&[]), &unmapped, "unmapped-module");
    control(
        &tree(&[]),
        &tree(&[("src/stray.rs", "fn lost() {}")]),
        "analysis-gap",
    );
}

#[test]
fn stale_entries_fail_and_ratchet_only_lowers() {
    let coupled = tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { crate::base::util(); crate::beta::go(); }",
    )]);
    let baseline = adopted(&coupled);
    assert_eq!(baseline.forbidden_edges.len(), 1);
    let clean = tree(&[]);
    assert!(rules(&outcome(&clean, &baseline)).contains(&"stale-baseline".to_owned()));
    let lowered = ratchet::ratchet(
        &analysis(&clean),
        &policy().symbol_budget_domains,
        &baseline,
    );
    assert!(lowered.forbidden_edges.is_empty());
    assert!(outcome(&clean, &lowered).violations.is_empty());
    // Ratcheting never raises a budget: the coupled tree now fails against it.
    let raised = ratchet::ratchet(
        &analysis(&coupled),
        &policy().symbol_budget_domains,
        &lowered,
    );
    assert_eq!(raised.budgets, lowered.budgets);
    assert!(!outcome(&coupled, &raised).violations.is_empty());
    let budget = raised
        .budgets
        .get("alpha")
        .copied()
        .unwrap_or(Budget::default());
    assert_eq!(budget.domains, 1);
}

#[test]
fn declared_edges_are_allowed() {
    let broken = tree(&[(
        "src/alpha.rs",
        "pub struct State; pub fn run() { crate::base::util(); crate::beta::go(); }",
    )]);
    let mut policy = policy();
    policy.allowed_edges.push(AllowedEdge {
        from: "alpha".into(),
        to: "beta".into(),
        reason: "reviewed".into(),
        cycles: true,
    });
    let a = gitcraft_arch_fitness::analyze(&broken, &policy, Default::default());
    let mut baseline = adopted(&tree(&[]));
    baseline.budgets.insert(
        "alpha".into(),
        Budget {
            domains: 2,
            symbols: Some(2),
            exported: 2,
        },
    );
    assert!(
        check(&a, &policy, &baseline).violations.is_empty(),
        "{:?}",
        check(&a, &policy, &baseline)
    );
}

#[test]
fn baseline_rewrites_are_refused_in_ci() {
    // The CLI calls this before `ratchet`/`adopt`; GitHub Actions always sets CI.
    let ci = std::env::var_os("CI").is_some_and(|v| !v.is_empty());
    assert_eq!(
        ratchet::refuse_in_ci().is_err(),
        ci || std::env::var_os("GITHUB_ACTIONS").is_some()
    );
}
