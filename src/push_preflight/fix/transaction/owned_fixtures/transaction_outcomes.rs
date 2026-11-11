//! Assert transaction outcomes without changing owned fixture lifetimes.
use super::*;

pub(super) fn success(f: &Fixture, after: &Value, directory: &Path) -> Result<(), String> {
    let root = &f.root;
    assert_eq!(after["head"], f.new);
    assert_eq!(after["symref"], "refs/heads/main");
    assert_eq!(
        fs::read(f.repo.join("file.txt")).map_err(|error| error.to_string())?,
        b"after\n"
    );
    run(
        &f.repo,
        &["diff-index", "--cached", "--quiet", &f.new, "--"],
        b"",
        None,
    )?;
    for phase in ["prepared", "checkout", "committed"] {
        assert_eq!(
            fs::read(root.join(format!("observed-{phase}.ok")))
                .map_err(|error| error.to_string())?,
            format!("refs/heads/main\n{}\n{}\n", f.old, f.new).as_bytes()
        );
    }
    let input = fs::read(root.join("original-input-prepared")).map_err(|e| e.to_string())?;
    assert!(
        input
            .split_inclusive(|b| *b == b'\n')
            .any(|line| line == format!("{} {} refs/heads/main\n", f.old, f.new).as_bytes())
    );
    assert_eq!(
        fs::read(root.join("original-args-prepared")).map_err(|error| error.to_string())?,
        b"prepared\0"
    );
    assert_eq!(
        fs::read_to_string(root.join("original-config-prepared"))
            .map_err(|error| error.to_string())?
            .trim(),
        f.root
            .join("original hooks ' \\ path")
            .to_str()
            .ok_or("Owned fixture path is not UTF-8")?
    );
    assert!(!directory.exists());
    Ok(())
}

pub(super) fn failure(
    f: &Fixture,
    case: &str,
    directory: &Path,
    elapsed: u128,
    before: &Value,
    after: &Value,
) -> Result<(), String> {
    let root = &f.root;
    if case != "missing-ack" {
        assert!(directory.exists());
        assert!(directory.join("before.index").exists());
        assert!(directory.join("recovery.txt").exists());
    }
    mutation_bytes(f, case, directory, before, after)?;
    if [
        "veto",
        "held-ref-lock",
        "switch-head",
        "detach-head",
        "index-change",
        "work-change",
        "missing-ack",
        "prelock-switch-head",
        "prelock-detach-head",
        "reftable-switch-head",
        "reftable-detach-head",
    ]
    .contains(&case)
    {
        assert!(!directory.join("checkout-attempted").exists());
    }
    if [
        "switch-head",
        "detach-head",
        "reftable-switch-head",
        "reftable-detach-head",
    ]
    .contains(&case)
    {
        assert!(
            fs::read_to_string(root.join("mutation.stderr"))
                .map_err(|error| error.to_string())?
                .contains(if case.starts_with("reftable") {
                    "lock"
                } else {
                    "HEAD.lock"
                })
        );
    }
    if case.starts_with("prelock-") {
        assert!(!root.join("original-input-prepared").exists());
    }
    if case == "timeout" {
        assert!((14_000..25_000).contains(&elapsed));
    }
    Ok(())
}

fn mutation_bytes(
    f: &Fixture,
    case: &str,
    directory: &Path,
    before: &Value,
    after: &Value,
) -> Result<(), String> {
    let root = &f.root;
    match case {
        "index-change" => {
            assert_eq!(after["head"], before["head"]);
            assert_eq!(
                fs::read(f.repo.join(".git/index")).map_err(|error| error.to_string())?,
                fs::read(root.join("external.index")).map_err(|error| error.to_string())?
            );
            assert_eq!(after["work"], before["work"]);
        }
        "work-change" => {
            assert_eq!(after["head"], before["head"]);
            assert_eq!(after["index"], before["index"]);
            assert_eq!(
                fs::read(f.repo.join("file.txt")).map_err(|error| error.to_string())?,
                b"external-work\n"
            );
        }
        "committed-failure" => {
            assert_eq!(after["head"], f.new);
            assert_eq!(
                fs::read(f.repo.join("file.txt")).map_err(|error| error.to_string())?,
                b"external-after-commit\n"
            );
            assert!(directory.join("checkout.ok").is_file());
        }
        _ => assert_eq!(before, after, "{case} changed repository"),
    }
    Ok(())
}
