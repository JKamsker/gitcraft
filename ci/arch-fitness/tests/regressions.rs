//! Regressions from review: each case once measured or rewrote the baseline wrongly.

mod support;

use gitcraft_arch_fitness::policy::Policy;
use gitcraft_arch_fitness::ratchet::{self, Why};
use gitcraft_arch_fitness::tab::Mode;
use gitcraft_arch_fitness::{analyze, markdown};
use support::{adopted, analysis, files, outcome, policy, rules, tree};

#[test]
fn public_items_of_a_one_domain_library_are_exported_to_dependent_crates() {
    let json = r#"{
      "schema": 1, "version": "test",
      "crates": [
        {"name": "os", "root": "os/lib.rs", "domain": "os"},
        {"name": "app", "root": "app/main.rs", "domain": "app", "externs": {"os": "os"}}
      ],
      "layers": [
        {"name": "services", "peers": false, "description": ""},
        {"name": "features", "peers": false, "description": ""}
      ],
      "domains": {
        "os": {"layer": "services", "owner": "o", "description": "", "modules": []},
        "app": {"layer": "features", "owner": "o", "description": "", "modules": []}
      },
      "allowed_edges": [],
      "repo_tab": {"path": "app::Tab", "coordinators": [], "owners": {}, "owner_layers": [], "default_owner": "app"},
      "symbol_budget_domains": []
    }"#;
    let policy: Policy = serde_json::from_str(json).unwrap_or_else(|e| unreachable!("{e}"));
    let sources = files(&[
        (
            "os/lib.rs",
            "pub fn used() {} pub fn spare() {} pub(crate) fn inner() {} mod hidden { pub fn h() {} }",
        ),
        ("app/main.rs", "pub struct Tab; fn main() { os::used(); }"),
    ]);
    let a = analyze(&sources, &policy, Default::default());
    assert_eq!(a.domains["os"].exported, 2, "{:?}", a.domains["os"]);
    let unused = &a.exported_unused["os"];
    assert!(
        unused.len() == 1 && unused[0].starts_with("os::spare"),
        "{unused:?}"
    );
    // Nothing depends on the binary, so its `pub` items stay inside it.
    assert_eq!(a.domains["app"].exported, 0);
}

#[test]
fn baseline_rewrites_are_refused_while_the_analysis_has_gaps() {
    let gap = outcome(
        &tree(&[("src/stray.rs", "fn lost() {}")]),
        &adopted(&tree(&[])),
    );
    assert!(ratchet::refuse_with_gaps(&gap).is_err());
    let clean = outcome(&tree(&[]), &adopted(&tree(&[])));
    assert!(ratchet::refuse_with_gaps(&clean).is_ok());
}

#[test]
fn markdown_lists_unresolved_path_sites() {
    let a = analysis(&tree(&[(
        "src/alpha.rs",
        "pub struct State; fn f() { crate::nowhere::x(); }",
    )]));
    let md = markdown::render(&a, None);
    assert!(md.contains("- `src/alpha.rs:1` crate::nowhere::x"), "{md}");
}

fn tab_act(body: &str) -> String {
    format!("pub struct State; impl crate::hub::Tab {{ fn act(&mut self) {{ {body} }} }}")
}

#[test]
fn destructuring_assignment_writes_every_element() {
    for body in [
        "(self.status,) = (String::new(),);",
        "[self.status, _] = [String::new(), String::new()];",
    ] {
        let a = analysis(&tree(&[("src/alpha.rs", &*tab_act(body))]));
        let modes: Vec<Mode> = a
            .repo_tab
            .cross_mutations
            .iter()
            .flat_map(|m| m.samples.iter().map(|s| s.mode))
            .collect();
        assert_eq!(modes, [Mode::Write], "{body}");
    }
}

#[test]
fn unrelated_shared_methods_do_not_hide_std_mutations() {
    let source = format!(
        "{} impl State {{ pub fn clear(&self) {{}} }}",
        tab_act("self.status.clear();")
    );
    let a = analysis(&tree(&[("src/alpha.rs", &*source)]));
    assert_eq!(a.repo_tab.cross_mutations.len(), 1);
}

#[test]
fn adopting_a_split_cycle_keeps_every_component() {
    let ring = tree(&[
        ("src/base.rs", "pub fn util() { crate::peer::help(); }"),
        ("src/peer.rs", "pub fn help() { crate::other::more(); }"),
        ("src/other.rs", "pub fn more() { crate::alpha::run(); }"),
    ]);
    let split = tree(&[
        ("src/base.rs", "pub fn util() { crate::peer::help(); }"),
        ("src/peer.rs", "pub fn help() { crate::base::util(); }"),
        ("src/other.rs", "pub fn more() { crate::alpha::run(); }"),
        (
            "src/alpha.rs",
            "pub struct State; pub fn run() { crate::other::more(); }",
        ),
    ]);
    assert_eq!(analysis(&split).cycles.len(), 2);
    let why = Why {
        reason: "r".into(),
        owner: "o".into(),
        remediation: "m".into(),
    };
    let new = ratchet::adopt(&analysis(&split), &policy(), &adopted(&ring), &why);
    let out = outcome(&split, &new);
    assert!(out.violations.is_empty(), "{:#?}", out.violations);
}

#[test]
fn cycle_exempt_edges_do_not_deepen_a_known_cycle() {
    // hub -> alpha (the Tab field) -> base -> hub is a cycle; alpha -> hub is `cycles: false`.
    let base = (
        "src/base.rs",
        "pub fn util() { let _ = crate::hub::Tab::new(); }",
    );
    let known = tree(&[base]);
    let deeper = tree(&[
        base,
        (
            "src/alpha.rs",
            "pub struct State; pub fn run() { crate::base::util(); let _ = crate::hub::Tab::new(); }",
        ),
    ]);
    assert_eq!(analysis(&known).cycles.len(), 1);
    let found = rules(&outcome(&deeper, &adopted(&known)));
    assert!(!found.iter().any(|r| r == "cycle-edge"), "{found:?}");
}
