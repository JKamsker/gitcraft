use super::*;
#[test]
fn rotated_split_index_dependency_is_resolved_from_the_saved_index() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gitcraft-index-pair-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    let result = (|| {
        let old_shared = root.join("old.shared");
        let new_shared = root.join("new.shared");
        let live_index = root.join("index");
        let owned = root.join("evidence");
        std::fs::create_dir(&owned).map_err(|error| error.to_string())?;
        std::fs::write(&old_shared, b"old dependency").map_err(|error| error.to_string())?;
        std::fs::write(&new_shared, b"new dependency").map_err(|error| error.to_string())?;
        // A previous live lookup saw old.shared; an ordinary writer rotated before
        // our main snapshot was captured. Only the saved main index is authoritative.
        let previously_observed = old_shared;
        std::fs::write(&live_index, b"main referring to new.shared")
            .map_err(|error| error.to_string())?;
        let selected = snapshot_pair(
            live_index.to_str().ok_or("Fixture path is not UTF-8")?,
            &owned,
            |saved| {
                assert_eq!(saved, owned.join("before.index"));
                assert_eq!(
                    std::fs::read(saved).map_err(|error| error.to_string())?,
                    b"main referring to new.shared"
                );
                Ok(Some(new_shared.to_string_lossy().into_owned()))
            },
        )?;
        assert_eq!(selected.as_deref(), new_shared.to_str());
        assert_ne!(selected.as_deref(), previously_observed.to_str());
        assert_eq!(
            std::fs::read(owned.join("before.shared-index")).map_err(|error| error.to_string())?,
            b"new dependency"
        );
        let refused = root.join("refused");
        std::fs::create_dir(&refused).map_err(|error| error.to_string())?;
        let unresolved = snapshot_pair(
            live_index.to_str().ok_or("Fixture path is not UTF-8")?,
            &refused,
            |_| Err("The dependency named by the saved main index cannot be proven".into()),
        );
        assert!(unresolved.is_err());
        assert!(!refused.join("before.shared-index").exists());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(root);
    result
}

#[test]
fn synthetic_capability_versions_refuse_before_user_dispatch() {
    for text in [
        "git version 2.28.0",
        "git version 2.40.1",
        "git version 2.49.9",
        "unknown",
        "git version 2.50.GIT",
    ] {
        let mut dispatched = false;
        if supported_version(text.as_bytes()) {
            dispatched = true;
        }
        assert!(!dispatched);
    }
    assert!(supported_version(b"git version 2.50.0.windows.1\n"));
    assert!(supported_version(b"git version 3.0.0\n"));
    assert_eq!(
        transaction("old", "new"),
        "start\nupdate HEAD new old\nprepare\ncommit\n"
    );
    assert!(!transaction("old", "new").contains("symref-verify"));
    assert!(!transaction("old", "new").contains("HEAD.lock"));
}

#[test]
fn missing_backend_acknowledgement_cannot_qualify_user_dispatch() -> Result<(), String> {
    let directory =
        std::env::temp_dir().join(format!("gitcraft-ack-recording-{}", std::process::id()));
    std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let mut dispatched = false;
    if ack(&directory, "probe-prepared", b"probe\n")
        && ack(&directory, "probe-committed", b"probe\n")
    {
        dispatched = true;
    }
    assert!(!dispatched); // a backend that ignored the hook cannot pass
    std::fs::write(directory.join("probe-prepared.ok"), b"probe\n").map_err(|e| e.to_string())?;
    assert!(!ack(&directory, "probe-committed", b"probe\n"));
    let _ = std::fs::remove_dir_all(directory);
    Ok(())
}

#[test]
fn synthetic_uncertain_lifecycle_always_retains_owned_evidence() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    for state in [
        "queued",
        "timeout-before-prepared",
        "timeout-before-checkout",
        "timeout-after-checkout",
    ] {
        let directory = std::env::temp_dir().join(format!(
            "gitcraft-owned-evidence-{}-{stamp}-{state}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        if state == "timeout-after-checkout" {
            std::fs::write(directory.join("checkout-attempted"), b"owned")
                .map_err(|e| e.to_string())?;
        }
        let hook = Hook {
            directory: directory.clone(),
            keep: true,
            old: "1".repeat(40),
            new: "2".repeat(40),
            proof: "owned".into(),
            capability_acknowledged: false,
        };
        drop(hook);
        assert!(
            directory.exists(),
            "An uncertain {state} must never trigger cleanup"
        );
        std::fs::remove_dir_all(directory).map_err(|e| e.to_string())?;
    }
    // Actual production constructors refuse a remote location before even probing Git.
    for state in [
        "queued",
        "before-prepared",
        "marker-observed",
        "after-checkout",
    ] {
        let refused = Hook::new(
            Path::new("ssh://fixture/home/repository"),
            "refs/heads/main",
            "old",
            "new",
        )
        .err();
        assert!(
            refused.is_some_and(|error| error.contains("no remote repair was dispatched")),
            "{state}"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn recording_hook_changes_are_refused_before_checkout_and_keep_external_bytes() -> Result<(), String>
{
    recording_scenarios(
        &[
            "unsupported-hook-command",
            "veto",
            "head-other",
            "detached",
            "index-changed",
            "work-changed",
            "unchanged",
        ],
        false,
    )
}

#[cfg(unix)]
#[test]
fn recording_pre_hook_identity_guards_prevent_relay_and_checkout() -> Result<(), String> {
    recording_scenarios(
        &[
            "wrong-target",
            "wrong-old",
            "wrong-new",
            "duplicate-input",
            "extra-reference",
            "extra-field",
            "empty-input",
            "pre-head-other",
            "pre-head-detached",
            "pre-head-oid-changed",
        ],
        true,
    )
}

#[cfg(unix)]
fn recording_scenarios(scenarios: &[&str], guards: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::Duration;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gitcraft-hook-recording-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let result = (|| {
        let bin = root.join("bin");
        std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
        let fake = bin.join("git");
        std::fs::write(&fake, r#"#!/bin/sh
while :; do case "$1" in -c) shift 2 ;; --literal-pathspecs) shift ;; *) break ;; esac; done
printf '%s\n' "$*" >>"$REC_DIRECTORY/commands"
case "$1" in
hook)
    input=
    for arg in "$@"; do case "$arg" in --to-stdin=*) input=${arg#--to-stdin=} ;; esac; phase=$arg; done
    cat "$input" >"$REC_DIRECTORY/original-input-$phase"
    [ "$REC_SCENARIO" != unsupported-hook-command ] || exit 92
    [ "$REC_SCENARIO" != veto ] || exit 1
    if [ "$phase" = prepared ]; then
        case "$REC_SCENARIO" in
        head-other) printf '%s\n' refs/heads/other >"$REC_DIRECTORY/head" ;;
        detached) : >"$REC_DIRECTORY/head" ;;
        index-changed) printf 'external-index' >"$REC_DIRECTORY/index" ;;
        work-changed) printf 'external-work' >"$REC_DIRECTORY/work" ;;
        esac
    fi
    ;;
symbolic-ref) [ -s "$REC_DIRECTORY/head" ] || exit 1; cat "$REC_DIRECTORY/head" ;;
rev-parse) cat "$REC_DIRECTORY/oid" ;;
diff|diff-files) [ "$REC_SCENARIO" != work-changed ] ;;
diff-index|status) exit 0 ;;
read-tree) printf 'checkout\n' >>"$REC_DIRECTORY/checkout-record" ;;
*) exit 91 ;;
esac
"#).map_err(|e|e.to_string())?;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let old = "1".repeat(40);
        let new = "2".repeat(40);
        let proof = format!("refs/heads/main\n{old}\n{new}\n");
        let correct = format!("{old} {new} refs/heads/main\n");
        for &scenario in scenarios {
            let updates = match scenario {
                "wrong-target" => format!("{old} {new} refs/heads/other\n"),
                "wrong-old" => format!("{} {new} refs/heads/main\n", "3".repeat(40)),
                "wrong-new" => format!("{old} {} refs/heads/main\n", "3".repeat(40)),
                "duplicate-input" => format!("{correct}{correct}"),
                "extra-reference" => format!("{correct}{old} {new} refs/heads/other\n"),
                "extra-field" => format!("{old} {new} refs/heads/main unexpected\n"),
                "empty-input" => String::new(),
                _ => correct.clone(),
            };
            let directory = root.join(scenario);
            std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
            for (name, value) in [
                ("mode", "repair"),
                ("index", "original-index"),
                ("before.index", "original-index"),
                ("work", "original-work"),
                ("head", "refs/heads/main\n"),
                ("oid", old.as_str()),
            ] {
                std::fs::write(directory.join(name), value).map_err(|e| e.to_string())?;
            }
            match scenario {
                "pre-head-other" => std::fs::write(directory.join("head"), b"refs/heads/other\n"),
                "pre-head-detached" => std::fs::write(directory.join("head"), b""),
                "pre-head-oid-changed" => std::fs::write(directory.join("oid"), "3".repeat(40)),
                _ => Ok(()),
            }
            .map_err(|e| e.to_string())?;
            let head_before = std::fs::read(directory.join("head")).map_err(|e| e.to_string())?;
            let oid_before = std::fs::read(directory.join("oid")).map_err(|e| e.to_string())?;
            let wrapper = directory.join("reference-transaction");
            std::fs::write(
                &wrapper,
                script(HookScript {
                    original: "/owned/original/reference-transaction",
                    branch: "refs/heads/main",
                    old: &old,
                    new: &new,
                    index: &directory.join("index").to_string_lossy(),
                    shared: None,
                    affected: &["file.txt".into()],
                    proof: &proof,
                    needs_checkout: true,
                }),
            )
            .map_err(|e| e.to_string())?;
            let run = |phase: &str| -> Result<std::process::Output, String> {
                let mut command = Command::new("sh");
                command
                    .env_clear()
                    .arg(&wrapper)
                    .arg(phase)
                    .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                    .env("REC_SCENARIO", scenario)
                    .env("REC_DIRECTORY", &directory);
                crate::push_preflight::recording_input(
                    command,
                    Some(updates.as_bytes().to_vec()),
                    &|| true,
                    Duration::from_secs(5),
                    1024 * 1024,
                    1024 * 1024,
                )
            };
            let output = run("prepared")?;
            assert_eq!(
                output.status.success(),
                scenario == "unchanged",
                "{scenario}"
            );
            assert_eq!(
                directory.join("checkout-record").exists(),
                scenario == "unchanged",
                "{scenario}"
            );
            assert_eq!(
                std::fs::read_to_string(directory.join("index")).map_err(|e| e.to_string())?,
                if scenario == "index-changed" {
                    "external-index"
                } else {
                    "original-index"
                }
            );
            assert_eq!(
                std::fs::read_to_string(directory.join("work")).map_err(|e| e.to_string())?,
                if scenario == "work-changed" {
                    "external-work"
                } else {
                    "original-work"
                }
            );
            if guards {
                assert!(
                    !directory.join("original-input-prepared").exists(),
                    "{scenario}: original relay"
                );
                assert!(
                    !directory.join("prepared.ok").exists(),
                    "{scenario}: prepared ACK"
                );
                assert!(
                    !directory.join("checkout-attempted").exists(),
                    "{scenario}: checkout"
                );
                assert_eq!(
                    std::fs::read(directory.join("head")).map_err(|e| e.to_string())?,
                    head_before
                );
                assert_eq!(
                    std::fs::read(directory.join("oid")).map_err(|e| e.to_string())?,
                    oid_before
                );
                let commands =
                    std::fs::read_to_string(directory.join("commands")).unwrap_or_default();
                assert!(
                    !commands
                        .lines()
                        .any(|line| line.starts_with("hook ") || line.starts_with("read-tree ")),
                    "{scenario}: {commands}"
                );
                println!(
                    "guard {scenario}: refused; original_prepared=0 checkout=0; external HEAD/index/work preserved"
                );
            } else {
                assert_eq!(
                    std::fs::read_to_string(directory.join("original-input-prepared"))
                        .map_err(|e| e.to_string())?,
                    updates
                );
            }
            if scenario == "unchanged" {
                assert!(
                    ack(&directory, "prepared", proof.as_bytes())
                        && ack(&directory, "checkout", proof.as_bytes())
                );
                std::fs::write(directory.join("oid"), &new).map_err(|e| e.to_string())?;
                assert!(run("committed")?.status.success());
                assert!(ack(&directory, "committed", proof.as_bytes()));
            } else {
                assert!(!ack(&directory, "checkout", proof.as_bytes()));
            }
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(root);
    result
}
