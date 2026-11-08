//! "Strip outputs when committing" for Jupyter notebooks: a clean filter, set up for this
//! clone only (its own config and `.git/info/attributes`), that takes every code cell's outputs
//! and execution count out of what is staged, like nbstripout. The notebook on disk keeps them.
//! The filter is nbstripout when it is installed where the repository is, else a few lines of
//! Python run there.

use std::path::Path;

use super::run;

/// The filter's name in the config and the attributes.
const FILTER: &str = "gitcraft-nbstrip";
const ATTRIBUTE: &str = "*.ipynb filter=gitcraft-nbstrip";

/// The Python a notebook goes through when nbstripout isn't there: outputs, execution counts,
/// their timings and the widgets' state out; written back as Jupyter writes notebooks (sorted
/// keys, one space of indent, the file's own line ends). A notebook with nothing to take out,
/// or that doesn't read (conflict markers), goes through as it is. Bytes in and out, whatever
/// the system's encoding.
const PYTHON: &str = r#"import json,sys
raw=sys.stdin.buffer.read()
def strip(nb):
 ch=False
 for c in nb.get("cells",[]):
  if not isinstance(c,dict) or c.get("cell_type")!="code": continue
  if c.get("outputs") or c.get("execution_count") is not None: ch=True
  c["outputs"]=[]
  c["execution_count"]=None
  m=c.get("metadata")
  if isinstance(m,dict) and "execution" in m: m.pop("execution"); ch=True
 m=nb.get("metadata")
 if isinstance(m,dict) and "widgets" in m: m.pop("widgets"); ch=True
 return ch
try:
 nb=json.loads(raw.decode("utf-8"))
 ok=isinstance(nb,dict) and strip(nb)
except Exception:
 ok=False
if ok:
 out=json.dumps(nb,indent=1,sort_keys=True,ensure_ascii=False)+"\n"
 if b"\r\n" in raw: out=out.replace("\n","\r\n")
 sys.stdout.buffer.write(out.encode("utf-8"))
else:
 sys.stdout.buffer.write(raw)
"#;

/// Whether `check` (a shell command) succeeds where the repository is: asked through git
/// itself (an alias), so it is that machine's PATH that counts.
fn runs_there(repo: &Path, check: &str) -> bool {
    let alias = format!("alias.gitcraft-probe=!{check} >/dev/null 2>&1");
    run(Some(repo), &["-c", &alias, "gitcraft-probe"]).is_ok()
}

/// The clean filter command for this machine: nbstripout, else Python 3 (a `python` that is
/// Python 2 doesn't count).
pub fn strip_command(repo: &Path) -> Result<String, String> {
    if runs_there(repo, "nbstripout --version") {
        return Ok("nbstripout".to_owned());
    }
    for python in ["python3", "python"] {
        if runs_there(
            repo,
            &format!("{python} -c \"import sys; sys.exit(sys.version_info<(3,))\""),
        ) {
            return Ok(format!("{python} -c '{PYTHON}'"));
        }
    }
    Err(crate::i18n::text(
        "backend-neither-nbstripout-nor-python-3-is-installed-where-the-repository-c298bb",
    )
    .to_owned())
}

/// The repository's own attributes file (`.git/info/attributes`), as a path for `remote`.
fn info_attributes(repo: &Path) -> Result<std::path::PathBuf, String> {
    let rel = run(Some(repo), &["rev-parse", "--git-path", "info/attributes"])?;
    Ok(crate::remote::resolve(repo, rel.trim()))
}

/// How notebooks' outputs are stripped when committing in this repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StripState {
    Off,
    /// By gitcraft's filter, which runs this.
    Ours(String),
    /// By another filter (nbstripout installed with `nbstripout --install`, one in the
    /// committed `.gitattributes`), by its name.
    Other(String),
}

/// What strips notebooks' outputs here: the `filter` attribute git gives a notebook, and
/// whether that filter has a clean command.
pub fn strip_state(repo: &Path) -> StripState {
    let probe = "gitcraft-probe.ipynb";
    let filter = run(Some(repo), &["check-attr", "-z", "filter", "--", probe])
        .ok()
        .and_then(|out| out.split('\0').nth(2).map(str::to_owned))
        .unwrap_or_default();
    if matches!(filter.as_str(), "" | "unspecified" | "unset" | "set") {
        return StripState::Off;
    }
    let clean = run(
        Some(repo),
        &["config", "--get", &format!("filter.{filter}.clean")],
    )
    .map(|v| v.trim().to_owned())
    .unwrap_or_default();
    match (filter == FILTER, clean.is_empty()) {
        (_, true) => StripState::Off,
        (true, false) => StripState::Ours(clean),
        (false, false) => StripState::Other(filter),
    }
}

/// Whether outputs are stripped when committing in this repository, by any filter.
#[cfg(test)]
pub fn strips_outputs(repo: &Path) -> bool {
    strip_state(repo) != StripState::Off
}

/// Turns gitcraft's stripping of outputs when committing on or off for this clone; what it
/// runs when turned on.
pub fn set_strip_outputs(repo: &Path, on: bool) -> Result<Option<String>, String> {
    let file = info_attributes(repo)?;
    let text = match crate::remote::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(crate::i18n::message(
                "backend-could-not-read-e-6dfcf3",
                &[
                    ("path", (file.display()).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            ));
        }
    };
    let mut lines: Vec<&str> = text.lines().filter(|l| l.trim() != ATTRIBUTE).collect();
    let key = format!("filter.{FILTER}.clean");
    let mut command = None;
    if on {
        let c = strip_command(repo)?;
        run(Some(repo), &["config", "--local", &key, &c])?;
        lines.push(ATTRIBUTE);
        command = Some(c);
    } else {
        // Exit 5: it wasn't set.
        super::run_ok_codes(
            Some(repo),
            &["config", "--local", "--unset-all", &key],
            &[0, 5],
        )?;
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    if let Some(dir) = file.parent() {
        crate::remote::create_dir_all(dir).map_err(|e| {
            crate::i18n::message(
                "backend-could-not-write-e-6c22d7",
                &[
                    ("path", (file.display()).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })?;
    }
    crate::remote::write(&file, out.as_bytes()).map_err(|e| {
        crate::i18n::message(
            "backend-could-not-write-e-6c22d7",
            &[
                ("path", (file.display()).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })?;
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turned on, a notebook staged has its outputs and counts taken out (the file on disk keeps
    /// them); turned off, it is staged as it is. Skipped where neither nbstripout nor Python is.
    #[test]
    fn outputs_stripped_when_staged() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-nbstrip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        run(Some(&dir), &["init", "-q"])?;
        if strip_command(&dir).is_err() {
            eprintln!("skipped: neither nbstripout nor Python is installed here");
            return Ok(());
        }
        assert!(!strips_outputs(&dir));
        set_strip_outputs(&dir, true)?;
        assert!(matches!(strip_state(&dir), StripState::Ours(_)));
        set_strip_outputs(&dir, true)?;
        let attrs = std::fs::read_to_string(dir.join(".git/info/attributes")).unwrap_or_default();
        assert_eq!(attrs.matches(ATTRIBUTE).count(), 1, "{attrs}");
        let nb = r#"{"cells":[{"cell_type":"code","execution_count":7,"id":"a","metadata":{},"outputs":[{"name":"stdout","output_type":"stream","text":["héllo\n"]}],"source":["print('héllo')"]}],"metadata":{},"nbformat":4,"nbformat_minor":5}"#;
        std::fs::write(dir.join("Analysis ü.ipynb"), nb).map_err(|e| e.to_string())?;
        run(Some(&dir), &["add", "--", "Analysis ü.ipynb"])?;
        let staged = run(Some(&dir), &["show", ":0:Analysis ü.ipynb"])?;
        assert!(!staged.contains("héllo\\n\""), "{staged}");
        assert!(staged.contains("\"execution_count\": null"), "{staged}");
        assert!(
            staged.contains("print('héllo')"),
            "the source stays: {staged}"
        );
        let on_disk = std::fs::read_to_string(dir.join("Analysis ü.ipynb")).unwrap_or_default();
        assert!(on_disk.contains("\"execution_count\":7"));
        // A notebook with nothing to strip goes through byte for byte (indent 2, CRLF).
        let clean = "{\r\n  \"cells\": [],\r\n  \"metadata\": {},\r\n  \"nbformat\": 4\r\n}";
        std::fs::write(dir.join("clean.ipynb"), clean).map_err(|e| e.to_string())?;
        run(
            Some(&dir),
            &["-c", "core.autocrlf=false", "add", "--", "clean.ipynb"],
        )?;
        let staged = crate::git::run_bytes(Some(&dir), &["show", ":0:clean.ipynb"])?;
        assert_eq!(staged, clean.as_bytes());
        // One that doesn't read (conflict markers) goes through too, rather than failing.
        std::fs::write(dir.join("broken.ipynb"), "<<<<<<< ours\n{}\n")
            .map_err(|e| e.to_string())?;
        run(Some(&dir), &["add", "--", "broken.ipynb"])?;
        set_strip_outputs(&dir, false)?;
        assert!(!strips_outputs(&dir));
        // Another tool's filter is told apart.
        run(
            Some(&dir),
            &["config", "filter.nbstripout.clean", "nbstripout"],
        )?;
        std::fs::write(dir.join(".gitattributes"), "*.ipynb filter=nbstripout\n")
            .map_err(|e| e.to_string())?;
        assert_eq!(
            strip_state(&dir),
            StripState::Other("nbstripout".to_owned())
        );
        let attrs = std::fs::read_to_string(dir.join(".git/info/attributes")).unwrap_or_default();
        assert!(!attrs.contains(ATTRIBUTE));
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
