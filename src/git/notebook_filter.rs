//! "Strip outputs when committing" for Jupyter notebooks: a clean filter, set up for this
//! repository only (its own config and `.git/info/attributes`), that takes every code cell's
//! outputs and execution count out of what is staged, as nbstripout does. The notebook on disk
//! keeps them. The filter is nbstripout when it is installed where the repository is, else a
//! few lines of Python run there.

use std::path::Path;

use super::run;

/// The filter's name in the config and the attributes.
const FILTER: &str = "gitcraft-nbstrip";
const ATTRIBUTE: &str = "*.ipynb filter=gitcraft-nbstrip";

/// The Python a notebook goes through when nbstripout isn't there: outputs and counts out,
/// written back as Jupyter writes notebooks (sorted keys, one space of indent).
/// Read and written as UTF-8 bytes whatever the system's encoding (Windows').
const PYTHON: &str = "import json,sys; nb=json.loads(sys.stdin.buffer.read().decode(\"utf-8\")); \
[c.update(outputs=[],execution_count=None) for c in nb.get(\"cells\",[]) if c.get(\"cell_type\")==\"code\"]; \
sys.stdout.buffer.write((json.dumps(nb,indent=1,sort_keys=True,ensure_ascii=False)+\"\\n\").encode(\"utf-8\"))";

/// Whether `program` runs where the repository is: asked through git itself (an alias), so it
/// is that machine's PATH that counts.
fn runs_there(repo: &Path, program: &str) -> bool {
    let alias = format!("alias.gitcraft-probe=!{program} --version >/dev/null 2>&1");
    run(Some(repo), &["-c", &alias, "gitcraft-probe"]).is_ok()
}

/// The clean filter command for this machine: nbstripout, else Python 3.
pub fn strip_command(repo: &Path) -> Result<String, String> {
    if runs_there(repo, "nbstripout") {
        return Ok("nbstripout".to_owned());
    }
    for python in ["python3", "python"] {
        if runs_there(repo, python) {
            return Ok(format!("{python} -c '{PYTHON}'"));
        }
    }
    Err(
        "Neither nbstripout nor Python is installed where the repository is: install \
         nbstripout (pip install nbstripout) or Python 3"
            .to_owned(),
    )
}

/// The repository's own attributes file (`.git/info/attributes`), as a path for `remote`.
fn info_attributes(repo: &Path) -> Result<std::path::PathBuf, String> {
    let rel = run(Some(repo), &["rev-parse", "--git-path", "info/attributes"])?;
    Ok(crate::remote::resolve(repo, rel.trim()))
}

/// Whether outputs are stripped when committing in this repository.
pub fn strips_outputs(repo: &Path) -> bool {
    let set = run(
        Some(repo),
        &[
            "config",
            "--local",
            "--get",
            &format!("filter.{FILTER}.clean"),
        ],
    )
    .is_ok();
    set && info_attributes(repo)
        .ok()
        .and_then(|f| crate::remote::read_to_string(&f).ok())
        .is_some_and(|t| t.lines().any(|l| l.trim() == ATTRIBUTE))
}

/// Turns stripping outputs when committing on or off for this repository.
pub fn set_strip_outputs(repo: &Path, on: bool) -> Result<(), String> {
    let file = info_attributes(repo)?;
    let text = match crate::remote::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read {}: {e}", file.display())),
    };
    let mut lines: Vec<&str> = text.lines().filter(|l| l.trim() != ATTRIBUTE).collect();
    let key = format!("filter.{FILTER}.clean");
    if on {
        let command = strip_command(repo)?;
        run(Some(repo), &["config", "--local", &key, &command])?;
        lines.push(ATTRIBUTE);
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
        crate::remote::create_dir_all(dir)
            .map_err(|e| format!("Could not write {}: {e}", file.display()))?;
    }
    crate::remote::write(&file, out.as_bytes())
        .map_err(|e| format!("Could not write {}: {e}", file.display()))
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
        assert!(strips_outputs(&dir));
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
        set_strip_outputs(&dir, false)?;
        assert!(!strips_outputs(&dir));
        let attrs = std::fs::read_to_string(dir.join(".git/info/attributes")).unwrap_or_default();
        assert!(!attrs.contains(ATTRIBUTE));
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
