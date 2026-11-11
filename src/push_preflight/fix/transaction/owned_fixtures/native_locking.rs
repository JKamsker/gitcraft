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
