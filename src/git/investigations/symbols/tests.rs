#![cfg(feature = "symbol-history")]
use super::*;
use crate::git::investigations::tests::Repo;

#[test]
fn pure_symbol_identity_survives_constant_and_complete_body_rewrites()
-> Result<(), Box<dyn std::error::Error>> {
    let parse = |body| syntax::parse("a.rs", body, &"a".repeat(40), &|| true);
    let versions = [
        parse("fn calc() -> i32 { 1 }")?,
        parse("fn calc() -> i32 { 2 }")?,
        parse("fn calc() -> i32 { let values = [12, 37]; values.iter().sum() }")?,
    ];
    for pair in versions.windows(2) {
        for (a, b) in [(&pair[0][0], &pair[1][0]), (&pair[1][0], &pair[0][0])] {
            let mut candidates = parse("fn unrelated() -> i32 { 900 }")?;
            candidates.push(b.clone());
            assert_eq!(
                choose(a, &candidates)?
                    .ok_or("missing chosen symbol")?
                    .0
                    .qualified,
                "calc"
            );
            assert!(choose(a, &[b.clone(), b.clone()]).is_err());
        }
    }
    let unrelated = parse(
        "fn unrelated() { let x = [\"entirely different\"]; for item in x { println!(\"{}\", item); } }",
    )?;
    assert!(choose(&versions[0][0], &unrelated).is_err());
    assert!(choose(&versions[0][0], &[])?.is_none());
    Ok(())
}

#[test]
fn pure_symbol_boundary_uses_stored_parents_instead_of_shallow_walk_roots()
-> Result<(), Box<dyn std::error::Error>> {
    let tree = "a".repeat(40);
    let parent = "b".repeat(40);
    let shallow = format!(
        "tree {tree}\nparent {parent}\nauthor Fixture <f@invalid> 1 +0000\n\nUnrelated boundary\n"
    );
    assert_eq!(
        boundary_event(shallow.as_bytes(), b"\n")?,
        ("Earlier shallow boundary (creation unproven)", true)
    );
    assert_eq!(
        boundary_event(shallow.as_bytes(), parent.as_bytes())?,
        ("Earlier boundary (creation unproven)", true)
    );
    let root = format!("tree {tree}\nauthor Fixture <f@invalid> 1 +0000\n\nRoot\n");
    assert_eq!(boundary_event(root.as_bytes(), b"")?, ("Created", false));
    assert!(boundary_event(b"tree invalid\n\n", b"").is_err());
    Ok(())
}

// Runtime regressions are intentionally separate from the pure gate while the
// orchestrator holds actual Git/process tests.
#[test]
fn symbol_body_rewrites_trace_backward_and_forward_without_false_lifetime_events()
-> Result<(), Box<dyn std::error::Error>> {
    let r = Repo::new()?;
    let first = save(&r, "a.rs", "fn calc() -> i32 { 1 }\n", 1_700_100_000)?;
    save(&r, "a.rs", "fn calc() -> i32 { 2 }\n", 1_700_100_001)?;
    let last = save(
        &r,
        "a.rs",
        "fn calc() -> i32 { let values = [12, 37]; values.iter().sum() }\n",
        1_700_100_002,
    )?;
    for revision in [&first, &last] {
        let h = history(investigate(
            &r.0,
            revision,
            "a.rs",
            "calc",
            None,
            256,
            &|| true,
        )?)?;
        assert!(!h.limited, "{h:?}");
        assert_eq!(
            h.steps
                .iter()
                .filter(|s| s.event == "Created")
                .map(|s| &s.commit)
                .collect::<Vec<_>>(),
            vec![&first]
        );
        assert_eq!(
            h.steps
                .iter()
                .filter(|s| s.event.contains("Body edited"))
                .count(),
            2
        );
        assert!(!h.steps.iter().any(|s| s.event.contains("Deleted")));
    }
    Ok(())
}

#[test]
fn symbol_shallow_clone_does_not_attribute_creation_to_an_unrelated_cut_point()
-> Result<(), Box<dyn std::error::Error>> {
    let r = Repo::new()?;
    let actual = save(&r, "a.rs", "fn calc() -> i32 { 1 }\n", 1_700_200_000)?;
    for i in 1..5 {
        save(&r, "other", &format!("{i}\n"), 1_700_200_000 + i)?;
    }
    let clone = r.0.join("owned-shallow");
    r.git(&[
        "clone",
        "--quiet",
        "--depth=2",
        &format!("file://{}", r.0.display()),
        clone.to_str().ok_or("clone path is not UTF-8")?,
    ])?;
    let h = history(investigate(
        &clone,
        "HEAD",
        "a.rs",
        "calc",
        None,
        256,
        &|| true,
    )?)?;
    assert!(h.limited);
    assert!(
        h.steps
            .iter()
            .any(|s| s.event == "Earlier shallow boundary (creation unproven)")
    );
    assert!(
        !h.steps
            .iter()
            .any(|s| s.event == "Created" || s.commit == actual)
    );
    assert!(h.notes.iter().any(|s| s.contains("shallow")));
    Ok(())
}
#[test]
fn parses_structural_functions_and_scopes_across_supported_grammars()
-> Result<(), Box<dyn std::error::Error>> {
    for (path, text, expected) in [
        (
            "a.rs",
            "struct A; impl A { fn calc(&self, x:i32)->i32 { x+1 } }",
            "A::calc",
        ),
        (
            "a.py",
            "class A:\n def calc(self, x):\n  return x+1\n",
            "A::calc",
        ),
        ("a.js", "class A { calc(x) { return x+1; } }", "A::calc"),
        (
            "a.ts",
            "class A { calc(x:number):number { return x+1; } }",
            "A::calc",
        ),
        (
            "a.tsx",
            "const calc = (x:number) => { return <div>{x}</div>; };",
            "calc",
        ),
        (
            "a.go",
            "package main\nfunc calc(x int) int { return x+1 }",
            "calc",
        ),
        ("a.c", "int calc(int x) { return x+1; }", "calc"),
        (
            "a.cpp",
            "namespace A { int calc(int x) { return x+1; } }",
            "A::calc",
        ),
    ] {
        let parsed = syntax::parse(path, text, &"a".repeat(40), &|| true)
            .map_err(|e| format!("{path}: {e}"))?;
        assert!(
            parsed.iter().any(|s| s.qualified == expected),
            "{path}: {parsed:?}"
        );
    }
    assert!(!syntax::supported("opaque.xyz"));
    assert!(syntax::parse("a.rs", "fn broken(", "", &|| true).is_err());
    assert!(syntax::parse("a.rs", "fn ok(){}", "", &|| false).is_err());
    Ok(())
}
fn save(r: &Repo, path: &str, text: &str, time: i64) -> Result<String, Box<dyn std::error::Error>> {
    let file = r.0.join(path);
    std::fs::create_dir_all(file.parent().ok_or("fixture file needs a parent")?)?;
    std::fs::write(file, text)?;
    r.git(&["add", "-A"])?;
    r.commit("author@example.invalid", time).map_err(Into::into)
}
fn history(answer: Answer) -> Result<Biography, String> {
    match answer {
        Answer::History(h) => Ok(h),
        _ => Err("Expected unambiguous history".into()),
    }
}
#[test]
fn follows_creation_edit_rename_move_and_deletion_without_mutating_repository()
-> Result<(), Box<dyn std::error::Error>> {
    let r = Repo::new()?;
    let first = save(
        &r,
        "a.rs",
        "fn calc(x:i32)->i32 { let a=x+1; let b=a*2; b-3 }\n",
        1_700_000_000,
    )?;
    save(
        &r,
        "a.rs",
        "fn calc(x:i32)->i32 { let a=x+1; let b=a*2; b-4 }\n",
        1_700_000_001,
    )?;
    save(
        &r,
        "a.rs",
        "fn compute(x:i32)->i32 { let a=x+1; let b=a*2; b-4 }\n",
        1_700_000_002,
    )?;
    r.git(&["mv", "a.rs", "b.rs"])?;
    r.commit("author@example.invalid", 1_700_000_003)?;
    std::fs::remove_file(r.0.join("b.rs"))?;
    r.git(&["add", "-A"])?;
    r.commit("author@example.invalid", 1_700_000_004)?;
    let refs = r.git(&["show-ref"])?;
    let index = std::fs::read(r.0.join(".git/index"))?;
    let h = history(investigate(
        &r.0,
        &first,
        "a.rs",
        "calc",
        None,
        256,
        &|| true,
    )?)?;
    for event in ["Created", "Body edited", "Renamed", "Moved", "Deleted"] {
        assert!(
            h.steps.iter().any(|s| s.event.contains(event)),
            "missing {event}: {h:?}"
        );
    }
    assert!(!h.limited);
    assert_eq!(
        h.steps.last().ok_or("missing final history step")?.symbol,
        None
    );
    assert_eq!(refs, r.git(&["show-ref"])?);
    assert_eq!(index, std::fs::read(r.0.join(".git/index"))?);
    assert!(!r.0.join("b.rs").exists());
    let deleted = history(investigate(
        &r.0,
        "HEAD",
        "b.rs",
        "compute",
        None,
        256,
        &|| true,
    )?)?;
    assert!(
        deleted
            .steps
            .last()
            .ok_or("missing deletion step")?
            .event
            .contains("Deleted")
    );
    assert!(investigate(&r.0, "HEAD", "b.rs", "compute", None, 256, &|| false).is_err());
    Ok(())
}
#[test]
fn ambiguity_is_visible_and_split_candidates_preserve_structural_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let r = Repo::new()?;
    let first = save(
        &r,
        "a.rs",
        "mod one { fn calc(x:i32)->i32{x+1} } mod two { fn calc(x:i32)->i32{x+2} }",
        1_700_000_000,
    )?;
    assert!(
        matches!(investigate(&r.0,&first,"a.rs","calc",None,256,&||true)?,Answer::Choices(v) if v.len()==2)
    );
    let answer = investigate(&r.0, &first, "a.rs", "", Some(100), 256, &|| true)?;
    assert!(matches!(answer,Answer::Choices(v) if v.len()==2));
    let parse = |s: &str| syntax::parse("a.rs", s, &"a".repeat(40), &|| true);
    let before = parse(
        "fn calc(x:i32)->i32 { let a=x+1; let b=a*2; let c=b-3; let d=c/4; let e=d+5; let f=e*6; let g=f-7; let h=g/8; h }",
    )?;
    let after = parse(
        "fn calc(x:i32)->i32 { let a=x+1; let b=a*2; let c=b-3; let d=c/4; helper(d) } fn helper(d:i32)->i32 { let e=d+5; let f=e*6; let g=f-7; let h=g/8; h }",
    )?;
    assert!(
        splits(&before[0], &after[0], &after)
            .iter()
            .any(|s| s.name == "helper")
    );
    let mut a = before[0].clone();
    a.path = "b.rs".into();
    let mut b = a.clone();
    b.path = "c.rs".into();
    assert!(choose(&before[0], &[a, b]).is_err());
    Ok(())
}

#[test]
fn backend_fluent_shallow_boundary_keeps_creation_classification() -> Result<(), String> {
    let raw = format!(
        "tree {}\nparent {}\n\nsubject\n",
        "a".repeat(40),
        "b".repeat(40)
    );
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        let (boundary, unproven) = boundary_event(raw.as_bytes(), b"\n")?;
        assert_eq!(boundary, "Earlier shallow boundary (creation unproven)");
        assert!(boundary.contains("shallow") && unproven);
        assert_eq!(
            display_boundary(boundary),
            if locale == "de" {
                "Frühere flache Grenze (Erstellung unbewiesen)"
            } else {
                boundary
            }
        );
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
