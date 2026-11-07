//! Deterministic local workflows. Mutations affect disposable repositories only.

use super::*;

mod data;
mod documents;
mod reads;
mod ui;

fn write(path: &std::path::Path, text: impl AsRef<[u8]>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn fixture() -> Result<git::fixture::Repo, String> {
    let repo = git::fixture::Repo::new("cpu-workflows")?;
    // Real git operations, with neither the user's hooks nor signing tools involved.
    let hooks = repo.dir.join(".git/no-hooks");
    std::fs::create_dir_all(&hooks).map_err(|e| e.to_string())?;
    repo.git(&["config", "core.hooksPath", &hooks.to_string_lossy()])?;
    repo.git(&["config", "gc.auto", "0"])?;
    repo.git(&["config", "maintenance.auto", "false"])?;
    for i in 0..400 {
        write(
            &repo.dir.join(format!("src/package{}/file{i}.rs", i % 20)),
            format!("// source file {i}\npub fn value() -> usize {{ {i} }}\n"),
        )?;
    }
    let source: String = (0..2000)
        .map(|i| format!("pub fn item_{i}() -> usize {{ {i} }}\n"))
        .collect();
    write(&repo.dir.join("src/main.rs"), &source)?;
    write(
        &repo.dir.join("README.md"),
        "# Example\n\nA useful project.\n",
    )?;
    write(&repo.dir.join(".gitignore"), "build/\n")?;
    repo.commit("initial project", &[])?;
    repo.commit(
        "update implementation",
        &[("src/main.rs", Some(&source.replace("{ 100 }", "{ 101 }")))],
    )?;
    Ok(repo)
}

pub(super) fn run(bench: &mut Bench, inputs: &Inputs) -> Result<(), String> {
    if bench.family(&[
        "repository/",
        "files/",
        "history/",
        "search/",
        "branches/",
        "metadata/",
        "operations/",
        "mcp/",
        "transport/",
        "diff/",
    ]) {
        reads::run(bench, inputs)?;
    }
    if bench.family(&[
        "owners/",
        "find/",
        "highlight/",
        "images/",
        "rich/",
        "analysis/",
        "text/",
    ]) {
        data::run(bench, inputs)?;
    }
    if bench.family(&["ui/", "startup/", "settings/", "terminal/"]) {
        ui::run(bench, inputs)?;
    }
    if bench.family(&["documents/"]) {
        documents::run(bench)?;
    }
    Ok(())
}
