//! Tests of dependency changes from lockfiles.

use super::deps::{self, DepChange, Ecosystem, Pkg, version_cmp};
use super::*;

fn sides(old: &str, new: &str) -> Sides {
    Sides {
        old: Some(old.to_owned()),
        new: Some(new.to_owned()),
    }
}

fn said(c: &[DepChange]) -> Vec<String> {
    c.iter()
        .map(|c| {
            format!(
                "{:?} {} {} {}",
                c.change,
                c.name,
                c.old.clone().unwrap_or_default(),
                c.new.clone().unwrap_or_default()
            )
        })
        .collect()
}

const CARGO_OLD: &str = r#"version = 4

[[package]]
name = "serde"
version = "1.0.100"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "syn"
version = "1.0.109"

[[package]]
name = "syn"
version = "2.0.10"

[[package]]
name = "old-thing"
version = "0.3.0"
"#;

#[test]
fn cargo_lock_upgrades_downgrades_additions_and_removals() -> Result<(), String> {
    let new = CARGO_OLD
        .replace("1.0.100", "1.0.200")
        .replace("2.0.10", "2.0.9")
        .replace(
            "name = \"old-thing\"\nversion = \"0.3.0\"",
            "name = \"new-thing\"\nversion = \"1.0.0\"",
        );
    let d = deps::diff("Cargo.lock", &sides(CARGO_OLD, &new))?;
    assert_eq!(d.ecosystem, Ecosystem::Cargo);
    assert_eq!(
        said(&d.changes),
        [
            "Upgraded serde 1.0.100 1.0.200",
            "Downgraded syn 2.0.10 2.0.9",
            "Added new-thing  1.0.0",
            "Removed old-thing 0.3.0 ",
        ]
    );
    // Each has its line: the `[[package]]` it is in.
    assert_eq!(d.changes[0].new_line, Some(3));
    assert_eq!(d.changes[3].old_line, Some(16));
    Ok(())
}

#[test]
fn package_lock_v3_with_licences_and_scoped_names() -> Result<(), String> {
    let old = r#"{
  "name": "app", "lockfileVersion": 3,
  "packages": {
    "": {"name": "app", "version": "1.0.0"},
    "node_modules/left-pad": {"version": "1.0.0", "license": "MIT"},
    "node_modules/@scope/pkg": {"version": "2.0.0", "license": "ISC"},
    "node_modules/a/node_modules/@scope/pkg": {"version": "1.0.0"}
  }
}"#;
    let new = old
        .replace(
            "\"1.0.0\", \"license\": \"MIT\"",
            "\"1.0.0\", \"license\": \"GPL-3.0\"",
        )
        .replace(
            "\"2.0.0\", \"license\": \"ISC\"",
            "\"2.1.0\", \"license\": \"ISC\"",
        );
    let d = deps::diff("web/package-lock.json", &sides(old, &new))?;
    assert_eq!(
        said(&d.changes),
        [
            "Upgraded @scope/pkg 2.0.0 2.1.0",
            "Relicensed left-pad 1.0.0 1.0.0"
        ]
    );
    assert_eq!(
        d.changes[1].license,
        Some((Some("MIT".to_owned()), Some("GPL-3.0".to_owned())))
    );
    Ok(())
}

#[test]
fn other_lockfiles_are_read_too() -> Result<(), String> {
    let v1 = r#"{"lockfileVersion": 1, "dependencies": {"a": {"version": "1.0.0", "dependencies": {"b": {"version": "2.0.0"}}}}}"#;
    let pkgs = deps::packages("package-lock.json", v1)?;
    assert_eq!(
        pkgs.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    let pnpm = "lockfileVersion: '9.0'\npackages:\n  '@babel/core@7.24.0':\n    resolution: {integrity: x}\n  /lodash@4.17.21:\n    resolution: {integrity: y}\n  /react-dom/18.2.0(react@18.2.0):\n    resolution: {integrity: z}\n";
    let pkgs = deps::packages("pnpm-lock.yaml", pnpm)?;
    let got: Vec<(&str, &str)> = pkgs
        .iter()
        .map(|p| (p.name.as_str(), p.version.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("@babel/core", "7.24.0"),
            ("lodash", "4.17.21"),
            ("react-dom", "18.2.0")
        ]
    );
    let composer = r#"{"packages": [{"name": "monolog/monolog", "version": "3.5.0", "license": ["MIT"]}], "packages-dev": [{"name": "phpunit/phpunit", "version": "10.0.0", "license": ["BSD-3-Clause"]}]}"#;
    assert_eq!(deps::packages("composer.lock", composer)?.len(), 2);
    let gosum = "github.com/a/b v1.2.3 h1:x=\ngithub.com/a/b v1.2.3/go.mod h1:y=\ngolang.org/x/net v0.20.0/go.mod h1:z=\n";
    let pkgs = deps::packages("go.sum", gosum)?;
    assert_eq!(pkgs.len(), 2);
    let poetry = "[[package]]\nname = \"requests\"\nversion = \"2.31.0\"\n";
    assert_eq!(deps::packages("poetry.lock", poetry)?[0].name, "requests");
    // Odd keys don't break the pnpm reader.
    assert!(
        deps::packages(
            "pnpm-lock.yaml",
            "packages:\n  '':\n    a: 1\n  'é':\n    a: 1\n  '@':\n    a: 1\n"
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn versions_compare_by_number() {
    use std::cmp::Ordering::*;
    assert_eq!(version_cmp("1.10.0", "1.9.0"), Greater);
    assert_eq!(version_cmp("v0.20.0", "v0.3.0"), Greater);
    assert_eq!(version_cmp("2.0.0", "2.0.0"), Equal);
    // A pre-release comes before its release.
    assert_eq!(version_cmp("1.0.0-beta", "1.0.0"), Less);
    assert_eq!(version_cmp("1.0.0-rc.10", "1.0.0-rc.9"), Greater);
    assert_eq!(version_cmp("2.0.0rc1", "2.0.0"), Less);
    assert_eq!(version_cmp("1.2.3+build.5", "1.2.3"), Equal);
    assert_eq!(
        version_cmp(
            "v0.0.0-20231001000000-abcdef",
            "v0.0.0-20230101000000-123456"
        ),
        Greater
    );
}

#[test]
fn many_versions_of_one_package_are_added_and_removed() {
    let p = |v: &str| Pkg {
        name: "x".to_owned(),
        version: v.to_owned(),
        license: None,
        line: 1,
        registry: None,
        public_registry: false,
    };
    let c = deps::compare(
        &[p("1.0.0"), p("2.0.0")],
        &[p("2.0.0"), p("3.0.0"), p("4.0.0")],
    );
    assert_eq!(
        said(&c),
        ["Added x  3.0.0", "Added x  4.0.0", "Removed x 1.0.0 "]
    );
}

#[test]
fn links_go_to_the_registry_and_to_what_changed() {
    assert_eq!(
        Ecosystem::Cargo.registry("serde", "1.0.0").0,
        "https://crates.io/crates/serde/1.0.0"
    );
    assert_eq!(
        Ecosystem::Npm.changes("@a/b", "1.0.0", "1.1.0").0,
        "https://npmdiff.dev/@a/b/1.0.0/1.1.0/"
    );
    assert_eq!(
        Ecosystem::Go.registry("golang.org/x/net", "v0.20.0").0,
        "https://pkg.go.dev/golang.org/x/net@v0.20.0"
    );
}

#[test]
fn a_broken_lockfile_falls_back_and_kinds_follow_the_name() {
    let d = deps::diff("Cargo.lock", &sides(CARGO_OLD, "[[package]\n"));
    assert!(d.is_err_and(|e| e.contains("Cargo.lock after the change isn't valid")));
    assert_eq!(RichKind::of("sub dir/Cargo.lock"), [RichKind::Deps]);
    assert_eq!(
        RichKind::of("package-lock.json"),
        [RichKind::Deps, RichKind::Config]
    );
    assert!(RichKind::of("cargo.lock").is_empty());
}

/// A big lockfile compares quickly.
#[test]
fn a_big_lockfile_compares_quickly() -> Result<(), String> {
    let make = |bump: bool| -> String {
        (0..5000)
            .map(|n| {
                let v = if bump && n == 2500 { "2.0.0" } else { "1.0.0" };
                format!("[[package]]\nname = \"crate{n}\"\nversion = \"{v}\"\n\n")
            })
            .collect()
    };
    let started = std::time::Instant::now();
    let d = deps::diff("Cargo.lock", &sides(&make(false), &make(true)))?;
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(said(&d.changes), ["Upgraded crate2500 1.0.0 2.0.0"]);
    Ok(())
}

/// Read from a real repository, and a click on a package goes to its line.
#[test]
fn dependency_changes_from_a_repository_and_the_view() -> Result<(), String> {
    use crate::git;
    use egui_kittest::kittest::Queryable;
    let dir = std::env::temp_dir().join(format!("gitcraft-rich-deps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(dir.join("crâte dir"));
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    let name = "crâte dir/Cargo.lock";
    let _ = std::fs::write(dir.join(name), CARGO_OLD);
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "one"])?;
    let _ = std::fs::write(dir.join(name), CARGO_OLD.replace("1.0.100", "1.0.200"));
    git::run(Some(&dir), &["commit", "-qam", "two"])?;
    let key = DiffKey::Commit {
        hash: git::head(&dir),
        path: name.to_owned(),
    };
    let read = load(&dir, &key, RichKind::Deps, None);
    let _ = std::fs::remove_dir_all(&dir);
    let Ok(Rich::Ready(diff)) = read else {
        return Err("not read as dependencies".to_owned());
    };
    let acts = std::cell::RefCell::new(Vec::new());
    let state = std::cell::RefCell::new(RichState::default());
    let fonts = std::cell::Cell::new(false);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([800.0, 300.0])
        .build_ui(|ui| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            show(ui, &diff, &mut state.borrow_mut(), &mut acts.borrow_mut());
        });
    harness.run();
    assert!(harness.query_by_label("1.0.100 → 1.0.200").is_some());
    assert!(harness.query_by_label("crates.io").is_some());
    assert!(harness.query_by_label("what changed").is_some());
    harness
        .get_by_label("Package serde. Go to line 3 of the lockfile")
        .click();
    harness.run();
    assert!(acts.borrow().iter().any(|a| matches!(
        a,
        Action::DiffGoToLine {
            old: None,
            new: Some(3)
        }
    )));
    Ok(())
}

/// What a review of the first version found read wrongly.
#[test]
fn copies_peers_old_pnpm_go_sum_git_aliases_and_local_packages() -> Result<(), String> {
    // npm: one version installed in two places, upgraded in both, is one upgrade.
    let npm = |v: &str| {
        format!(
            r#"{{"lockfileVersion": 3, "packages": {{"": {{"version": "1.0.0"}}, "node_modules/a/node_modules/foo": {{"version": "{v}"}}, "node_modules/b/node_modules/foo": {{"version": "{v}"}}, "packages/web": {{"name": "web", "version": "0.1.0"}}}}}}"#
        )
    };
    let d = deps::diff("package-lock.json", &sides(&npm("2.0.0"), &npm("2.1.0")))?;
    assert_eq!(said(&d.changes), ["Upgraded foo 2.0.0 2.1.0"]);
    // A workspace package of the project: no registry links.
    let ws = deps::packages("package-lock.json", &npm("1.0.0"))?;
    assert!(ws.iter().any(|p| p.name == "web" && p.registry.is_none()));
    // pnpm 5: `name/version_peers`.
    let pnpm = |v: &str| {
        format!(
            "lockfileVersion: 5.4\npackages:\n  /foo/{v}_react@18.2.0:\n    resolution: {{integrity: x}}\n  /@s/bar/{v}_react@18.2.0:\n    resolution: {{integrity: y}}\n"
        )
    };
    let d = deps::diff("pnpm-lock.yaml", &sides(&pnpm("1.0.0"), &pnpm("1.1.0")))?;
    assert_eq!(
        said(&d.changes),
        ["Upgraded @s/bar 1.0.0 1.1.0", "Upgraded foo 1.0.0 1.1.0"]
    );
    // go.sum keeps the old version's go.mod line: still an upgrade.
    let old = "golang.org/x/text v0.3.7 h1:a=\ngolang.org/x/text v0.3.7/go.mod h1:b=\n";
    let new = "golang.org/x/text v0.3.7/go.mod h1:b=\ngolang.org/x/text v0.14.0 h1:c=\ngolang.org/x/text v0.14.0/go.mod h1:d=\n";
    let d = deps::diff("go.sum", &sides(old, new))?;
    assert_eq!(
        said(&d.changes),
        ["Upgraded golang.org/x/text v0.3.7 v0.14.0"]
    );
    // A git dependency moved to another commit, same version: seen.
    let git = |rev: &str| {
        format!(
            "[[package]]\nname = \"dep\"\nversion = \"0.1.0\"\nsource = \"git+https://example.com/dep#{rev}\"\n\n[[package]]\nname = \"app\"\nversion = \"0.2.0\"\n"
        )
    };
    let d = deps::diff("Cargo.lock", &sides(&git("aaaaaaaa1"), &git("bbbbbbbb2")))?;
    assert_eq!(
        said(&d.changes),
        ["Upgraded dep 0.1.0 (git aaaaaaa) 0.1.0 (git bbbbbbb)"]
    );
    assert_eq!(d.changes[0].registry, (None, None));
    let app = deps::packages("Cargo.lock", &git("a"))?;
    assert!(app.iter().any(|p| p.name == "app" && p.registry.is_none()));
    // npm v1 aliases link to the package they stand for.
    let alias = r#"{"lockfileVersion": 1, "dependencies": {"foo": {"version": "npm:bar@1.2.0"}}}"#;
    let pkgs = deps::packages("package-lock.json", alias)?;
    assert_eq!(
        pkgs[0].registry,
        Some(("bar".to_owned(), "1.2.0".to_owned()))
    );
    // A pre-release to its release is an upgrade.
    let rc = CARGO_OLD.replace("1.0.100", "1.0.0-rc.1");
    let d = deps::diff(
        "Cargo.lock",
        &sides(&rc, &CARGO_OLD.replace("1.0.100", "1.0.0")),
    )?;
    assert_eq!(said(&d.changes), ["Upgraded serde 1.0.0-rc.1 1.0.0"]);
    Ok(())
}
