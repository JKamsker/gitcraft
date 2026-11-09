//! Gate 2 only: isolated, explicitly selected owned-local fixtures. Never default tests.
use super::*;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::Duration;

const TEST: &str = "push_preflight::fix::transaction::owned_fixtures::gate2_owned_local_fixture";
fn fixture_git() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("GITCRAFT_GATE2_GIT") {
        return Ok(PathBuf::from(path));
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .ok_or("No Git executable for owned fixture".into())
}
fn version(git: &Path) -> Result<Vec<u8>, String> {
    let mut command = Command::new(git);
    command.env_clear().arg("--version");
    let output = crate::push_preflight::recording_input(
        command,
        None,
        &|| true,
        Duration::from_secs(5),
        4096,
        4096,
    )?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err("Owned Git version probe failed".into())
    }
}

fn run(repo: &Path, args: &[&str], input: &[u8], index: Option<&Path>) -> Result<String, String> {
    let mut command = Command::new(fixture_git()?);
    command.arg("-C").arg(repo).args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = crate::push_preflight::recording_input(
        command,
        Some(input.to_vec()),
        &|| true,
        Duration::from_secs(15),
        1024 * 1024,
        1024 * 1024,
    )?;
    if !output.status.success() {
        return Err(format!(
            "owned Git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim_end().to_owned())
        .map_err(|e| e.to_string())
}
fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| e.to_string())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn file(path: &Path) -> Value {
    match (fs::read(path), fs::metadata(path)) {
        (Ok(bytes), Ok(metadata)) => {
            json!({"hex":hex(&bytes),"mode":metadata.permissions().mode()})
        }
        (Err(error), _) | (_, Err(error)) => json!({"error":error.to_string()}),
    }
}
fn snapshot(repo: &Path) -> Result<Value, String> {
    let shared = run(
        repo,
        &["rev-parse", "--path-format=absolute", "--shared-index-path"],
        b"",
        None,
    )?;
    Ok(json!({
        "head":run(repo,&["rev-parse","HEAD"],b"",None)?,
        "symref":run(repo,&["symbolic-ref","-q","HEAD"],b"",None).ok(),
        "refs":run(repo,&["show-ref"],b"",None)?,
        "index":file(&repo.join(".git/index")),
        "work":file(&repo.join("file.txt")),
        "shared_path":shared,"shared":if shared.is_empty(){Value::Null}else{file(Path::new(&shared))}
    }))
}
struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    old: String,
    new: String,
}
impl Fixture {
    fn new(root: &Path, refs: &str, objects: &str) -> Result<Self, String> {
        let repo = root.join("repository ' quoted \\ path");
        fs::create_dir(&repo).map_err(|e| e.to_string())?;
        let mut init = vec![
            "init".to_owned(),
            "--template=".into(),
            "--initial-branch=main".into(),
        ];
        if supported_version(&version(&fixture_git()?)?) {
            init.extend([
                format!("--ref-format={refs}"),
                format!("--object-format={objects}"),
            ]);
        } else if refs != "files" || objects != "sha1" {
            return Err("Older Git fixture must use default files/sha1 for refusal proof".into());
        }
        run(
            &repo,
            &init.iter().map(String::as_str).collect::<Vec<_>>(),
            b"",
            None,
        )?;
        for (key, value) in [
            ("user.name", "Gate2 Fixture"),
            ("user.email", "gate2@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
            ("core.fsmonitor", "false"),
        ] {
            run(&repo, &["config", key, value], b"", None)?;
        }
        write(repo.join("file.txt"), b"before\n")?;
        run(&repo, &["add", "--", "file.txt"], b"", None)?;
        run(
            &repo,
            &["commit", "--no-gpg-sign", "-m", "Owned initial"],
            b"",
            None,
        )?;
        let old = run(&repo, &["rev-parse", "HEAD"], b"", None)?;
        let index = root.join("candidate.index");
        run(&repo, &["read-tree", &old], b"", Some(&index))?;
        let blob = run(&repo, &["hash-object", "-w", "--stdin"], b"after\n", None)?;
        run(
            &repo,
            &[
                "update-index",
                "--cacheinfo",
                &format!("100644,{blob},file.txt"),
            ],
            b"",
            Some(&index),
        )?;
        let tree = run(&repo, &["write-tree"], b"", Some(&index))?;
        let new = run(
            &repo,
            &[
                "commit-tree",
                "--no-gpg-sign",
                &tree,
                "-p",
                &old,
                "-m",
                "Owned rewritten",
            ],
            b"",
            None,
        )?;
        Ok(Self {
            root: root.into(),
            repo,
            old,
            new,
        })
    }
    fn observer(&self, mode: &str) -> Result<(), String> {
        let hooks = self.root.join("original hooks ' \\ path");
        fs::create_dir(&hooks).map_err(|e| e.to_string())?;
        let root = remote::sh_quote(&self.root.to_string_lossy());
        let repo = remote::sh_quote(&self.repo.to_string_lossy());
        let old = remote::sh_quote(&self.old);
        let action = if mode.ends_with("switch-head") {
            "switch-head"
        } else if mode.ends_with("detach-head") {
            "detach-head"
        } else {
            mode
        };
        let script = format!(
            r#"#!/bin/sh
set -eu
if [ -n "${{GITCRAFT_GATE2_NESTED:-}}" ]; then cat >/dev/null; exit 0; fi
export GITCRAFT_GATE2_NESTED=1
root={root}
repo={repo}
cat >"$root/original-input-$1"
printf '%s\0' "$@" >"$root/original-args-$1"
git config --get core.hooksPath >"$root/original-config-$1"
if [ "$1" = prepared ]; then
 case {action} in
 veto) exit 1 ;;
 switch-head) git symbolic-ref HEAD refs/heads/other >"$root/mutation.stdout" 2>"$root/mutation.stderr" || exit $? ;;
 detach-head) git update-ref --no-deref HEAD {old} >"$root/mutation.stdout" 2>"$root/mutation.stderr" || exit $? ;;
 index-change) cp -- "$root/external.index" "$repo/.git/index" ;;
 work-change) printf 'external-work\n' >"$repo/file.txt" ;;
 timeout) sleep 20 ;;
 full-fix)
   for directory in "$TMPDIR"/gitcraft-push-ref-transaction-*; do
     [ -d "$directory" ] || continue
     [ "$(cat "$directory/mode")" = repair ] || continue
     for phase in prepared checkout committed; do
       (set -C; : >"$directory/$phase.ok") || exit 1
       ln -- "$directory/$phase.ok" "$root/observed-$phase.ok" || exit 1
     done
   done ;;
 esac
fi
if [ "$1" = committed ] && [ {mode} = committed-failure ]; then
 printf 'external-after-commit\n' >"$repo/file.txt"
 exit 1
fi
"#
        );
        let path = hooks.join("reference-transaction");
        write(&path, script)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        run(
            &self.repo,
            &["config", "core.hooksPath", &hooks.to_string_lossy()],
            b"",
            None,
        )?;
        Ok(())
    }
}
fn retain_acks(hook: &Hook, root: &Path) -> Result<(), String> {
    for phase in ["prepared", "checkout", "committed"] {
        let path = hook.directory.join(format!("{phase}.ok"));
        // Empty placeholders cannot satisfy the nonempty production proof. Hardlinks
        // preserve actual wrapper writes even after successful owned-directory cleanup.
        write(&path, b"")?;
        fs::hard_link(path, root.join(format!("observed-{phase}.ok")))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn evidence(directory: &Path) -> Value {
    let mut result = serde_json::Map::new();
    for name in [
        "recovery.txt",
        "before.index",
        "before.shared-index",
        "prepared.ok",
        "checkout.ok",
        "committed.ok",
        "checkout-attempted",
        "updates-prepared",
        "updates-committed",
        "updates-aborted",
        "probe-updates-prepared",
        "probe-prepared.ok",
        "probe-committed.ok",
    ] {
        if directory.join(name).is_file() {
            result.insert(name.into(), file(&directory.join(name)));
        }
    }
    Value::Object(result)
}
fn execute_case(root: &Path, case: &str) -> Result<(), String> {
    if case.starts_with("native-") {
        return native_case(root, case);
    }
    if case == "full-fix" {
        return full_fix_case(root);
    }
    if case == "split-index" {
        return split_case(root);
    }
    let refs = if case.starts_with("reftable") {
        "reftable"
    } else {
        "files"
    };
    let objects = if case.ends_with("sha256") {
        "sha256"
    } else {
        "sha1"
    };
    let f = Fixture::new(root, refs, objects)?;
    if case.ends_with("switch-head") || case.starts_with("prelock-") {
        run(&f.repo, &["branch", "other", &f.old], b"", None)?;
    }
    if case == "index-change" {
        let external = root.join("external.index");
        fs::copy(f.repo.join(".git/index"), &external).map_err(|e| e.to_string())?;
        run(
            &f.repo,
            &["update-index", "--assume-unchanged", "file.txt"],
            b"",
            Some(&external),
        )?;
    }
    f.observer(case)?;
    let before = snapshot(&f.repo)?;
    if case == "old-git" || case == "capability-refusal" {
        // The outer fixture supplied the old-Git PATH before this process started.
        let result = Hook::new(&f.repo, "refs/heads/main", &f.old, &f.new);
        let after = snapshot(&f.repo)?;
        let error = result
            .err()
            .ok_or("unsupported Git unexpectedly qualified")?;
        write(
            root.join("case.json"),
            serde_json::to_vec_pretty(
                &json!({"case":case,"before":before,"after":after,"error":error}),
            )
            .map_err(|error| error.to_string())?,
        )?;
        assert_eq!(before, after);
        assert!(error.contains("Git 2.50"), "{error}");
        return Ok(());
    }
    let hook_result = Hook::new(&f.repo, "refs/heads/main", &f.old, &f.new);
    let mut hook = match hook_result {
        Ok(hook) => hook,
        Err(error) => {
            if case == "files-sha1-diagnostic" {
                // Repeat only the owned bare capability transaction, using the existing
                // production bounded transport while retaining normally withheld stderr.
                for entry in fs::read_dir(std::env::temp_dir()).map_err(|e| e.to_string())? {
                    let directory = entry.map_err(|e| e.to_string())?.path();
                    let probe = directory.join("capability.git");
                    if !probe.is_dir() {
                        continue;
                    }
                    let old = run(&probe, &["rev-parse", "HEAD"], b"", None)?;
                    let objects = run(
                        &probe,
                        &[
                            "cat-file",
                            "--batch-all-objects",
                            "--batch-check=%(objectname) %(objecttype)",
                        ],
                        b"",
                        None,
                    )?;
                    let new = objects
                        .lines()
                        .filter_map(|line| line.strip_suffix(" commit"))
                        .find(|oid| *oid != old)
                        .ok_or("missing owned candidate commit")?;
                    let hooks = format!("core.hooksPath={}", directory.display());
                    let input = transaction(&old, new);
                    let output = git::local_repair_input(
                        &f.repo,
                        &[
                            "-C",
                            &probe.to_string_lossy(),
                            "-c",
                            &hooks,
                            "update-ref",
                            "--stdin",
                        ],
                        input.as_bytes(),
                        &[],
                    )?;
                    write(root.join("diagnostic-input"), &input)?;
                    write(root.join("diagnostic.stdout"), &output.stdout)?;
                    write(root.join("diagnostic.stderr"), &output.stderr)?;
                    write(root.join("diagnostic-status"), output.status.to_string())?;
                }
            }
            write(root.join("case.json"),serde_json::to_vec_pretty(&json!({"case":case,"constructor_error":error,"before":before,"after":snapshot(&f.repo)?})).map_err(|error| error.to_string())?)?;
            return Err(format!(
                "supported {refs}/{objects} capability refused: {error}"
            ));
        }
    };
    let directory = hook.directory.clone();
    let capability = evidence(&directory);
    retain_acks(&hook, root)?;
    if case == "missing-ack" {
        hook.capability_acknowledged = false;
    }
    if case == "prelock-switch-head" {
        run(
            &f.repo,
            &[
                "-c",
                "core.hooksPath=",
                "symbolic-ref",
                "HEAD",
                "refs/heads/other",
            ],
            b"",
            None,
        )?;
    } else if case == "prelock-detach-head" {
        run(
            &f.repo,
            &[
                "-c",
                "core.hooksPath=",
                "update-ref",
                "--no-deref",
                "HEAD",
                &f.old,
            ],
            b"",
            None,
        )?;
    }
    let before = if case.starts_with("prelock-") {
        snapshot(&f.repo)?
    } else {
        before
    };
    let held_lock = if case == "held-ref-lock" || case == "checkout-failure" {
        let path = if case == "held-ref-lock" {
            f.repo.join(".git/refs/heads/main.lock")
        } else {
            f.repo.join(".git/index.lock")
        };
        let lock = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        Some(lock)
    } else {
        None
    };
    let started = std::time::Instant::now();
    let result = hook.execute(&f.repo);
    let elapsed = started.elapsed().as_millis();
    let after = snapshot(&f.repo)?;
    let retained = evidence(&directory);
    write(root.join("case.json"),serde_json::to_vec_pretty(&json!({"case":case,"backend":refs,"object_format":objects,"old":f.old,"new":f.new,"before":before,"after":after,"result":result,"elapsed_ms":elapsed,"directory":directory,"directory_retained":directory.exists(),"capability":capability,"evidence":retained})).map_err(|error| error.to_string())?)?;
    drop(held_lock); // Owned file handle only. The owned lock file is deliberately retained.
    if matches!(
        case,
        "files-sha1" | "files-sha256" | "reftable-sha1" | "reftable-sha256"
    ) {
        result?;
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
    } else {
        assert!(result.is_err(), "{case} unexpectedly succeeded");
        if case != "missing-ack" {
            assert!(directory.exists());
            assert!(directory.join("before.index").exists());
            assert!(directory.join("recovery.txt").exists());
        }
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
    }
    Ok(())
}
fn full_fix_case(root: &Path) -> Result<(), String> {
    let f = Fixture::new(root, "files", "sha1")?;
    let receiver = root.join("owned-receiver.git");
    run(
        &f.repo,
        &["init", "--template=", "--bare", &receiver.to_string_lossy()],
        b"",
        None,
    )?;
    run(
        &f.repo,
        &["remote", "add", "origin", &receiver.to_string_lossy()],
        b"",
        None,
    )?;
    run(
        &f.repo,
        &["push", "--set-upstream", "origin", "main"],
        b"",
        None,
    )?;
    write(
        f.repo.join("private.txt"),
        b"password=fixture_dummy_value_123\n",
    )?;
    run(&f.repo, &["add", "--", "private.txt"], b"", None)?;
    run(
        &f.repo,
        &["commit", "--no-gpg-sign", "-m", "Owned outgoing fixture"],
        b"",
        None,
    )?;
    let outgoing = run(&f.repo, &["rev-parse", "HEAD"], b"", None)?;
    f.observer("full-fix")?;
    let before = snapshot(&f.repo)?;
    let profile = crate::settings::profiles::UserProfile {
        author_name: "Gate2 Fixture".into(),
        author_email: "gate2@example.invalid".into(),
        ..Default::default()
    };
    let plan = crate::push_preflight::scan(&f.repo, vec![vec!["push".into()]], profile, &|| true)?;
    assert!(
        plan.findings
            .iter()
            .any(|finding| finding.kind == crate::push_preflight::Kind::Secret)
    );
    let result = crate::push_preflight::fix(&f.repo, &plan, "secrets");
    let after = snapshot(&f.repo)?;
    write(root.join("case.json"), serde_json::to_vec_pretty(&json!({"case":"full-fix","before":before,"after":after,"result":result,"secret_file_exists_after":f.repo.join("private.txt").exists(),"receiver_head":run(&receiver,&["rev-parse","refs/heads/main"],b"",None)?,"baseline":f.old})).map_err(|error| error.to_string())?)?;
    result?;
    let new = run(&f.repo, &["rev-parse", "HEAD"], b"", None)?;
    assert_ne!(new, outgoing);
    assert_eq!(run(&f.repo, &["rev-parse", "HEAD^"], b"", None)?, f.old);
    assert!(!f.repo.join("private.txt").exists());
    assert!(
        run(
            &f.repo,
            &["status", "--porcelain=v1", "--untracked-files=all"],
            b"",
            None
        )?
        .is_empty()
    );
    assert_eq!(
        run(&receiver, &["rev-parse", "refs/heads/main"], b"", None)?,
        f.old
    );
    for phase in ["prepared", "checkout", "committed"] {
        assert_eq!(
            fs::read(root.join(format!("observed-{phase}.ok")))
                .map_err(|error| error.to_string())?,
            format!("refs/heads/main\n{outgoing}\n{new}\n").as_bytes()
        );
    }
    assert_eq!(
        fs::read(root.join("original-input-prepared")).map_err(|error| error.to_string())?,
        format!("{outgoing} {new} refs/heads/main\n").as_bytes()
    );
    assert_eq!(
        fs::read(root.join("original-args-prepared")).map_err(|error| error.to_string())?,
        b"prepared\0"
    );
    assert!(
        crate::push_preflight::scan(&f.repo, vec![vec!["push".into()]], plan.profile, &|| true)?
            .findings
            .iter()
            .all(|finding| finding.kind != crate::push_preflight::Kind::Secret)
    );
    Ok(())
}

fn split_case(root: &Path) -> Result<(), String> {
    let f = Fixture::new(root, "files", "sha1")?;
    run(&f.repo, &["update-index", "--split-index"], b"", None)?;
    let current = root.join("saved-pair");
    fs::create_dir(&current).map_err(|e| e.to_string())?;
    let index = f.repo.join(".git/index");
    let before = snapshot(&f.repo)?;
    let mut selected = String::new();
    let dependency = snapshot_pair(
        index.to_str().ok_or("Owned fixture path is not UTF-8")?,
        &current,
        |saved| {
            // Force an actual new dependency only after immutable before.index was captured.
            run(&f.repo, &["update-index", "--no-split-index"], b"", None)?;
            run(
                &f.repo,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!(
                        "100755,{},file.txt",
                        run(
                            &f.repo,
                            &["rev-parse", &format!("{}:file.txt", f.old)],
                            b"",
                            None
                        )?
                    ),
                ],
                b"",
                None,
            )?;
            run(&f.repo, &["update-index", "--split-index"], b"", None)?;
            selected = path_line(local(
                &f.repo,
                &["rev-parse", "--path-format=absolute", "--shared-index-path"],
                b"",
                &[("GIT_INDEX_FILE", saved.to_string_lossy().into_owned())],
            )?)?;
            Ok(Some(selected.clone()))
        },
    )?;
    let live = run(
        &f.repo,
        &["rev-parse", "--path-format=absolute", "--shared-index-path"],
        b"",
        None,
    )?;
    assert_ne!(selected, live, "fixture did not rotate dependency");
    assert_eq!(dependency.as_deref(), Some(selected.as_str()));
    assert_eq!(
        fs::read(current.join("before.shared-index")).map_err(|error| error.to_string())?,
        fs::read(&selected).map_err(|error| error.to_string())?
    );
    assert_eq!(before["shared_path"], selected);
    // Only remove this fixture's now-unreferenced dependency; live index uses another.
    fs::remove_file(&selected).map_err(|e| e.to_string())?;
    let missing = root.join("missing-pair");
    fs::create_dir(&missing).map_err(|e| e.to_string())?;
    let result = snapshot_pair(
        current
            .join("before.index")
            .to_str()
            .ok_or("Owned fixture path is not UTF-8")?,
        &missing,
        |saved| {
            let bytes = local(
                &f.repo,
                &["rev-parse", "--path-format=absolute", "--shared-index-path"],
                b"",
                &[("GIT_INDEX_FILE", saved.to_string_lossy().into_owned())],
            )?;
            path_line(bytes).map(Some)
        },
    );
    let after = snapshot(&f.repo)?;
    write(root.join("case.json"),serde_json::to_vec_pretty(&json!({"case":"split-index","before":before,"after":after,"saved_dependency":selected,"live_dependency":live,"missing_result":result,"saved_pair":evidence(&current)})).map_err(|error| error.to_string())?)?;
    assert!(result.is_err(), "missing dependency unexpectedly qualified");
    assert_eq!(after["head"], before["head"]);
    assert_eq!(after["work"], before["work"]);
    Ok(())
}

// Diagnostic only: native Git HEAD update in an owned fixture, without invoking
// changed production code or checking out any working tree.
fn native_case(root: &Path, case: &str) -> Result<(), String> {
    let refs = if case.contains("reftable") {
        "reftable"
    } else {
        "files"
    };
    let objects = if case.ends_with("sha256") {
        "sha256"
    } else {
        "sha1"
    };
    let f = Fixture::new(root, refs, objects)?;
    run(&f.repo, &["branch", "other", &f.old], b"", None)?;
    let hooks = root.join("native diagnostic hooks ' path");
    fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    let script = format!(
        r#"#!/bin/sh
if [ -n "${{GITCRAFT_GATE2_NESTED:-}}" ]; then cat >/dev/null; exit 0; fi
export GITCRAFT_GATE2_NESTED=1
root={root}
cat >"$root/native-input-$1" || exit 1
printf '%s\0' "$@" >"$root/native-args-$1" || exit 1
if [ "$1" = prepared ]; then
    branch=0
    while read -r before after reference; do
        [ "$before" = {old} ] && [ "$after" = {new} ] || exit 1
        case "$reference" in refs/heads/main) branch=$((branch+1)) ;; *) exit 1 ;; esac
    done <"$root/native-input-$1"
    [ "$branch" = 1 ] || exit 1
    [ "$(git symbolic-ref -q HEAD)" = refs/heads/main ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    case {case} in
    native-switch-head)
        git symbolic-ref HEAD refs/heads/other >"$root/mutation.stdout" 2>"$root/mutation.stderr"
        code=$?
        printf '%s\n' "$code" >"$root/mutation-status"
        [ "$code" -ne 0 ] || exit 1
        ;;
    native-detach-head)
        git update-ref --no-deref HEAD {old} >"$root/mutation.stdout" 2>"$root/mutation.stderr"
        code=$?
        printf '%s\n' "$code" >"$root/mutation-status"
        [ "$code" -ne 0 ] || exit 1
        ;;
    esac
    [ "$(git symbolic-ref -q HEAD)" = refs/heads/main ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {old} ] || exit 1
    printf 'owned-native-prepared\n' >"$root/native-prepared.ok"
elif [ "$1" = committed ]; then
    [ "$(git symbolic-ref -q HEAD)" = refs/heads/main ] || exit 1
    [ "$(git rev-parse --verify HEAD)" = {new} ] || exit 1
    printf 'owned-native-committed\n' >"$root/native-committed.ok"
fi
"#,
        root = remote::sh_quote(&root.to_string_lossy()),
        old = remote::sh_quote(&f.old),
        new = remote::sh_quote(&f.new),
        case = remote::sh_quote(case)
    );
    let path = hooks.join("reference-transaction");
    write(&path, script)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    run(
        &f.repo,
        &["config", "core.hooksPath", &hooks.to_string_lossy()],
        b"",
        None,
    )?;
    if case == "native-prelock-switch" {
        run(
            &f.repo,
            &[
                "-c",
                "core.hooksPath=",
                "symbolic-ref",
                "HEAD",
                "refs/heads/other",
            ],
            b"",
            None,
        )?;
    }
    let before = snapshot(&f.repo)?;
    let input = format!("start\nupdate HEAD {} {}\nprepare\ncommit\n", f.new, f.old);
    let output =
        git::local_repair_input(&f.repo, &["update-ref", "--stdin"], input.as_bytes(), &[])?;
    let after = snapshot(&f.repo)?;
    write(root.join("native-transaction-input"), &input)?;
    write(root.join("native.stdout"), &output.stdout)?;
    write(root.join("native.stderr"), &output.stderr)?;
    write(root.join("case.json"),serde_json::to_vec_pretty(&json!({"case":case,"diagnostic_only":true,"production_protocol_unchanged":true,"before":before,"after":after,"old":f.old,"new":f.new,"status":output.status.to_string(),"backend":refs,"object_format":objects,"no_checkout_attempted":true})).map_err(|error| error.to_string())?)?;
    assert_eq!(before["index"], after["index"]);
    assert_eq!(before["work"], after["work"]);
    if case == "native-prelock-switch" {
        assert!(!output.status.success());
        assert_eq!(
            before, after,
            "same-tip external HEAD switch must remain intact"
        );
        assert!(!root.join("native-prepared.ok").exists());
    } else {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(after["head"], f.new);
        assert_eq!(after["symref"], "refs/heads/main");
        assert_eq!(
            fs::read(root.join("native-prepared.ok")).map_err(|error| error.to_string())?,
            b"owned-native-prepared\n"
        );
        assert_eq!(
            fs::read(root.join("native-committed.ok")).map_err(|error| error.to_string())?,
            b"owned-native-committed\n"
        );
        assert_eq!(
            fs::read(root.join("native-args-prepared")).map_err(|error| error.to_string())?,
            b"prepared\0"
        );
        if ["native-switch-head", "native-detach-head"].contains(&case) {
            assert_ne!(
                fs::read_to_string(root.join("mutation-status"))
                    .map_err(|error| error.to_string())?
                    .trim(),
                "0"
            );
            assert!(
                fs::read_to_string(root.join("mutation.stderr"))
                    .map_err(|error| error.to_string())?
                    .contains("HEAD.lock")
            );
        }
    }
    Ok(())
}

#[test]
fn gate2_owned_local_fixture() -> Result<(), String> {
    let Some(case) = std::env::var_os("GITCRAFT_GATE2_CASE") else {
        return default_owned_suite();
    };
    let case = case.to_str().ok_or("Owned case must be UTF-8")?;
    let root = PathBuf::from(
        std::env::var_os("GITCRAFT_GATE2_ROOT").ok_or("Explicit owned root required")?,
    );
    if std::env::var_os("GITCRAFT_GATE2_CHILD").is_some() {
        return execute_case(&root, case);
    }
    owned_child(&root, case, &fixture_git()?)
}
fn default_owned_suite() -> Result<(), String> {
    let git = fixture_git()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gitcraft-owned-repair-default-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    if !supported_version(&version(&git)?) {
        println!(
            "Owned repair: older Git must refuse before mutation; evidence {}",
            root.display()
        );
        return owned_child(&root.join("capability-refusal"), "capability-refusal", &git);
    }
    for case in [
        "files-sha1",
        "files-sha256",
        "reftable-sha1",
        "reftable-sha256",
        "prelock-switch-head",
        "prelock-detach-head",
        "switch-head",
        "detach-head",
        "reftable-switch-head",
        "reftable-detach-head",
        "held-ref-lock",
        "veto",
        "index-change",
        "work-change",
        "checkout-failure",
        "committed-failure",
        "timeout",
        "split-index",
        "missing-ack",
        "full-fix",
    ] {
        println!("Owned default case {case}: {}", root.display());
        owned_child(&root.join(case), case, &git)?;
    }
    let old = Path::new("/usr/bin/git");
    if old.is_file() && !supported_version(&version(old)?) {
        owned_child(&root.join("old-git"), "old-git", &git)?;
    }
    Ok(())
}
fn owned_child(root: &Path, case: &str, git: &Path) -> Result<(), String> {
    fs::create_dir(root).map_err(|e| e.to_string())?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    for name in ["home", "xdg", "tmp", "template"] {
        fs::create_dir(root.join(name)).map_err(|e| e.to_string())?;
    }
    write(root.join("global.gitconfig"), b"")?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .env_clear()
        .current_dir(root)
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(
            "PATH",
            if case == "old-git" {
                "/usr/bin:/bin".into()
            } else {
                std::env::join_paths([
                    git.parent().ok_or("Git executable has no parent")?,
                    Path::new("/usr/bin"),
                    Path::new("/bin"),
                ])
                .map_err(|e| e.to_string())?
            },
        )
        .env("GITCRAFT_GATE2_GIT", git)
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("xdg"))
        .env("TMPDIR", root.join("tmp"))
        .env("GIT_CONFIG_GLOBAL", root.join("global.gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TEMPLATE_DIR", root.join("template"))
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GITCRAFT_GATE2_CASE", case)
        .env("GITCRAFT_GATE2_ROOT", root)
        .env("GITCRAFT_GATE2_CHILD", "1");
    let output = crate::push_preflight::recording_input(
        command,
        None,
        &|| true,
        Duration::from_secs(45),
        1024 * 1024,
        1024 * 1024,
    )?;
    write(root.join("child.stdout"), &output.stdout)?;
    write(root.join("child.stderr"), &output.stderr)?;
    println!("owned case {case}, evidence {}", root.display());
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
