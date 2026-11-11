use super::*;
#[test]
fn canceled_fetch_still_cleans_exact_refs_and_retains_both_failures() -> Result<(), String> {
    let name = "refs/gitcraft/dependency-check/owned/0";
    let oid = "a".repeat(40);
    let owned = Advertisement {
        namespace: "refs/gitcraft/dependency-check/owned".into(),
        refs: [(name.into(), oid.clone())].into(),
    };
    let mut calls = Vec::new();
    let cleanup = owned.cleanup(|args, input| {
        calls.push((
            args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            input,
        ));
        if args[0] == "for-each-ref" {
            Ok(format!("{name} {oid}\n"))
        } else {
            Ok(String::new())
        }
    });
    assert!(
        owned
            .finish(Err("cancelled after creation".into()), cleanup)
            .err()
            .ok_or("Expected operation failure")?
            .contains("cancelled after creation")
    );
    assert_eq!(calls.len(), 2);
    assert_eq!(
        String::from_utf8(calls[1].1.clone()).map_err(|e| e.to_string())?,
        format!("delete {name} {oid}\n")
    );
    let empty = owned.cleanup(|_, _| Ok(String::new()));
    assert!(
        owned
            .finish(Err("cancelled before creation".into()), empty)
            .is_err()
    );
    let listing = owned.cleanup(|_, _| Err("listing failure".into()));
    let error = owned
        .finish(Err("fetch failed".into()), listing)
        .err()
        .ok_or("Expected operation failure")?;
    assert!(
        error.contains("fetch failed")
            && error.contains("listing failure")
            && error.contains(&owned.namespace)
    );
    let mut deletes = 0;
    assert!(
        owned
            .cleanup(|args, _| {
                if args[0] == "for-each-ref" {
                    Ok(format!("{name} {}\n", "b".repeat(40)))
                } else {
                    deletes += 1;
                    Ok(String::new())
                }
            })
            .is_err()
    );
    assert_eq!(deletes, 0);
    let cleanup = owned.cleanup(|args, _| {
        if args[0] == "for-each-ref" {
            Ok(format!("{name} {oid}\n"))
        } else {
            Err("delete failure".into())
        }
    });
    assert!(
        owned
            .finish(Ok(String::new()), cleanup)
            .err()
            .ok_or("Expected operation failure")?
            .contains("delete failure")
    );
    Ok(())
}
