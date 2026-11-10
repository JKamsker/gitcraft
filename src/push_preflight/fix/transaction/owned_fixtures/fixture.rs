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
