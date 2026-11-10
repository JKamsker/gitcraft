use super::*;

fn node(path: &str) -> Dependency {
    Dependency {
        path: path.into(),
        repo: PathBuf::from(format!("/recorded/{path}")),
        required: BTreeSet::new(),
        remotes: vec!["child-host".into()],
        remote: "child-host".into(),
        fetch_url: "https://child.invalid/repo".into(),
        push_url: "https://child.invalid/repo".into(),
        source: "a".repeat(40),
        target: "child-release".into(),
        published: false,
        snapshot: None,
        problem: None,
        result: None,
    }
}

#[test]
fn children_are_deepest_first_and_failure_never_reaches_parent_dispatch() {
    for fail in [false, true] {
        let mut children = vec![node("child"), node("child/nested")];
        let mut sent = Vec::new();
        let failed = publish_in_order(&mut children, |child| {
            sent.push(child.path.clone());
            assert_eq!(child.remote, "child-host");
            assert_eq!(child.target, "child-release");
            if fail {
                Err("recorded child refusal".into())
            } else {
                Ok("published".into())
            }
        });
        assert_eq!(failed, fail);
        if fail {
            assert_eq!(sent, ["child/nested"]);
            assert!(
                children
                    .iter()
                    .all(|n| n.result.as_ref().is_some_and(Result::is_err))
            );
        } else {
            assert_eq!(sent, ["child/nested", "child"]);
            assert!(
                children
                    .iter()
                    .all(|n| n.result.as_ref().is_some_and(Result::is_ok))
            );
        }
        assert!(!sent.iter().any(|name| name == "parent"));
    }
}
