use std::collections::BTreeMap;
use std::path::Path;
static REFS: std::sync::Mutex<BTreeMap<std::path::PathBuf, BTreeMap<String, String>>> =
    std::sync::Mutex::new(BTreeMap::new());
fn with_refs<T>(repo: &Path, f: impl FnOnce(&mut BTreeMap<String, String>) -> T) -> T {
    f(REFS.lock().unwrap().entry(repo.to_path_buf()).or_default())
}
#[derive(Default)]
pub struct Call {
    pub env: Vec<(&'static str, String)>,
    pub stdin: Option<Vec<u8>>,
    /// Match the production call's transport-duration option and default.
    pub long: bool,
}
pub fn now() -> i64 {
    0
}
pub fn reset_refs(repo: &Path) {
    with_refs(repo, |r| {
        r.clear();
        r.insert("refs/heads/main^{commit}".into(), "head".into());
    });
}
pub fn extra_tracked(repo: &Path, path: &str) {
    with_refs(repo, |r| {
        r.insert("fake:tracked".into(), path.into());
    });
}
pub fn added(repo: &Path, path: &str) {
    with_refs(repo, |r| {
        r.insert("fake:added".into(), path.into());
    });
}
pub fn run(repo: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let repo = repo.unwrap();
    let gd = repo.join(".git");
    let value = match args {
        ["rev-parse", "--absolute-git-dir"] => gd.display().to_string(),
        ["rev-parse", "--show-toplevel"] => repo.display().to_string(),
        ["rev-parse", "--verify", "HEAD"] | ["rev-parse", "HEAD"] => "head".into(),
        ["rev-parse", value] if value.ends_with("^{tree}") => "tree".into(),
        ["rev-parse", "-q", "--verify", name] if *name == "MERGE_HEAD" => {
            if !gd.join(name).exists() {
                return Err("absent".into());
            }
            "theirs".into()
        }
        ["rev-parse", "-q", "--verify", name] => {
            return with_refs(repo, |r| r.get(*name).cloned().ok_or("absent".into()));
        }
        ["symbolic-ref", "-q", "--short", "HEAD"] => "main".into(),
        ["name-rev", ..] => "topic".into(),
        ["config", "--bool", ..] => "false".into(),
        ["config", ..] => String::new(),
        ["ls-files", "-v", "-z"] => "H file\0".into(),
        ["ls-files", "--cached", "-z"] => with_refs(repo, |r| {
            format!(
                "file\0{}",
                r.get("fake:tracked")
                    .map(|p| format!("{p}\0"))
                    .unwrap_or_default()
            )
        }),
        ["ls-files", "-u"] => {
            if std::fs::read(gd.join("index")).unwrap() == b"DIRC-original" {
                "conflict\n".into()
            } else {
                String::new()
            }
        }
        ["ls-tree", ..] => "100644 blob object\tfile\0".into(),
        ["diff-tree", rest @ ..] if rest.contains(&"--diff-filter=A") => {
            with_refs(repo, |r| r.get("fake:added").cloned().unwrap_or_default())
        }
        ["diff-tree", ..] => String::new(),
        ["diff-index", ..] | ["diff-files", ..] => String::new(),
        ["status", ..] => String::new(),
        ["update-index", ..] => String::new(),
        ["update-ref", "-d", name, ..] => {
            with_refs(repo, |r| r.remove(*name));
            String::new()
        }
        ["update-ref", name, value, ..] => {
            with_refs(repo, |r| r.insert(name.to_string(), value.to_string()));
            String::new()
        }
        ["reset", "--hard", "-q", "HEAD"] => {
            std::fs::write(repo.join("file"), b"clean").unwrap();
            std::fs::write(gd.join("index"), b"clean-index").unwrap();
            std::fs::remove_file(gd.join("MERGE_HEAD")).unwrap();
            String::new()
        }
        ["read-tree", "-m", "-u", ..] => {
            std::fs::write(repo.join("file"), b"filtered").unwrap();
            std::fs::write(gd.join("index"), b"tree-index").unwrap();
            String::new()
        }
        _ => return Err(format!("Unimplemented fake Git: {args:?}")),
    };
    Ok(value)
}
pub fn run_with_index(repo: &Path, index: &str, args: &[&str]) -> Result<String, String> {
    match args {
        ["ls-files", "-u", "-z"] | ["add", ..] => Ok(String::new()),
        ["write-tree"] => {
            let _ = (repo, index);
            Ok("tree".into())
        }
        _ => Err(format!("fake index {args:?}")),
    }
}
pub fn run_call(_: Option<&Path>, _: &[&str], _: Call) -> Result<String, String> {
    Ok(String::new())
}
pub fn run_env(_: Option<&Path>, _: &[&str], _: &[(&str, &str)]) -> Result<String, String> {
    Ok("snapshot".into())
}
pub fn run_bytes(_: Option<&Path>, _: &[&str]) -> Result<Vec<u8>, String> {
    Ok(b"clean".to_vec())
}
