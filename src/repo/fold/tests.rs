//! Tests for which files fold, why, what their folded line says, and what a tab remembers of
//! their attributes; on real repositories, through what the lists and diffs use.

use std::collections::HashMap;

use super::list::group_title;
use super::rules::{glob_for_tests as glob, thousands};
use super::*;
use crate::git::{self, Area, FoldAttr, StatusFile};

fn rules() -> Rules {
    Rules::new(true, "")
}

#[test]
fn lockfiles_and_minified_files_fold_by_name_anywhere() {
    let r = rules();
    for path in [
        "Cargo.lock",
        "web/package-lock.json",
        "yarn.lock",
        "a/b/pnpm-lock.yaml",
        "poetry.lock",
        "go.sum",
        "Gemfile.lock",
        "composer.lock",
        "with space/Cargo.lock",
    ] {
        assert_eq!(r.reason(path, None), Some(FoldReason::Lockfile), "{path}");
    }
    for path in ["dist/app.min.js", "ünï/stÿle.min.css", "x.min.mjs"] {
        assert_eq!(r.reason(path, None), Some(FoldReason::Minified), "{path}");
    }
    for path in [
        "Cargo.toml",
        "cargo.lock.bak",
        "src/lock.rs",
        "package.json",
        ".min.js",
        "admin.js",
        "Cargo.lock/",
        "",
    ] {
        assert_eq!(r.reason(path, None), None, "{path}");
    }
}

#[test]
fn attributes_decide_before_names() {
    let r = rules();
    assert_eq!(
        r.reason("src/gen.rs", Some(FoldAttr::Generated)),
        Some(FoldReason::Generated)
    );
    assert_eq!(
        r.reason("data.bin", Some(FoldAttr::NoDiff)),
        Some(FoldReason::NoDiff)
    );
    // `linguist-generated=false` keeps even a lockfile open.
    assert_eq!(r.reason("Cargo.lock", Some(FoldAttr::NotGenerated)), None);
    assert_eq!(
        r.reason("Cargo.lock", Some(FoldAttr::Unspecified)),
        Some(FoldReason::Lockfile)
    );
}

#[test]
fn folding_turned_off_folds_nothing() {
    let off = Rules::new(false, "*.snap");
    assert_eq!(off.reason("Cargo.lock", Some(FoldAttr::Generated)), None);
    assert_eq!(off.reason("a.snap", None), None);
}

#[test]
fn patterns_of_the_settings_fold_too() {
    let r = Rules::new(true, "*.snap, dist/**\n/top.txt  docs/*.html");
    assert_eq!(
        r.reason("tests/__snapshots__/a b.snap", None),
        Some(FoldReason::Pattern)
    );
    assert_eq!(r.reason("dist/x/y.js", None), Some(FoldReason::Pattern));
    assert_eq!(r.reason("top.txt", None), Some(FoldReason::Pattern));
    assert_eq!(r.reason("sub/top.txt", None), None);
    assert_eq!(r.reason("docs/index.html", None), Some(FoldReason::Pattern));
    assert_eq!(r.reason("docs/api/index.html", None), None);
    assert_eq!(r.reason("src/dist/x.js", None), None);
    // Empty patterns (a stray comma) match nothing.
    assert_eq!(Rules::new(true, " , ,").reason("anything", None), None);
}

#[test]
fn globs_keep_stars_within_folders() {
    assert!(glob("*.js", "a.js"));
    assert!(!glob("*.js", "a/b.js"));
    assert!(glob("**/b.js", "b.js"));
    assert!(glob("**/b.js", "a/c/b.js"));
    assert!(!glob("**/b.js", "ab.js"));
    assert!(glob("a/**", "a/b/c"));
    assert!(glob("a?c", "abc"));
    assert!(!glob("a?c", "a/c"));
    assert!(glob("ü*", "über"));
    assert!(glob("", ""));
    assert!(!glob("", "x"));
    // Many stars over a long name end quickly.
    assert!(!glob("*a*a*a*a*a*b", &"a".repeat(200)));
}

#[test]
fn the_folded_line_says_what_changed_and_why() {
    assert_eq!(
        stats_line(Some(1204), Some(880), FoldReason::Generated),
        "+1,204 −880, generated"
    );
    assert_eq!(
        stats_line(Some(0), Some(0), FoldReason::Lockfile),
        "+0 −0, lockfile"
    );
    assert_eq!(
        stats_line(None, None, FoldReason::NoDiff),
        "-diff in .gitattributes"
    );
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1000), "1,000");
    assert_eq!(thousands(1_234_567), "1,234,567");
    assert_eq!(thousands(u32::MAX), "4,294,967,295");
}

#[test]
fn a_group_is_named_after_what_it_holds() {
    use FoldReason::*;
    assert_eq!(group_title(&[Lockfile]), "1 lockfile");
    assert_eq!(group_title(&[Lockfile, Lockfile]), "2 lockfiles");
    assert_eq!(
        group_title(&[Generated, Generated, Generated]),
        "3 generated files"
    );
    assert_eq!(group_title(&[Minified]), "1 minified file");
    assert_eq!(group_title(&[NoDiff, NoDiff]), "2 files marked -diff");
    assert_eq!(group_title(&[Lockfile, Generated]), "2 folded files");
}

#[test]
fn attributes_are_asked_once_and_remembered() {
    let mut folding = Folding::default();
    let paths = ["src/gen.rs", "Cargo.lock", "src/gen.rs"];
    let unknown = folding.unknown(paths);
    assert_eq!(
        unknown,
        vec!["src/gen.rs".to_owned(), "Cargo.lock".to_owned()]
    );
    let epoch = folding.asking(&unknown);
    // Asked: not again while the answer is on its way.
    assert!(folding.unknown(paths).is_empty());
    let generation = folding.generation;
    let answer = HashMap::from([("src/gen.rs".to_owned(), FoldAttr::Generated)]);
    folding.arrived(epoch, unknown, Ok(answer));
    assert!(folding.generation > generation);
    assert!(folding.unknown(paths).is_empty());
    let r = rules();
    assert_eq!(
        folding.reason(&r, "src/gen.rs"),
        Some(FoldReason::Generated)
    );
    assert_eq!(folding.reason(&r, "Cargo.lock"), Some(FoldReason::Lockfile));
}

#[test]
fn a_failed_answer_leaves_names_to_decide() {
    let mut folding = Folding::default();
    let unknown = folding.unknown(["Cargo.lock", "a.rs"]);
    let epoch = folding.asking(&unknown);
    folding.arrived(epoch, unknown, Err("git is gone".to_owned()));
    assert!(folding.unknown(["Cargo.lock", "a.rs"]).is_empty());
    assert_eq!(
        folding.reason(&rules(), "Cargo.lock"),
        Some(FoldReason::Lockfile)
    );
    assert_eq!(folding.reason(&rules(), "a.rs"), None);
}

fn work_with(paths: &[(&str, char)]) -> git::WorkStatus {
    git::WorkStatus {
        unstaged: paths
            .iter()
            .map(|&(path, code)| StatusFile {
                path: path.to_owned(),
                old_path: None,
                code,
                area: Area::Unstaged,
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn a_changed_gitattributes_or_checkout_forgets_what_was_read() {
    let mut folding = Folding::default();
    folding.saw_status(&work_with(&[("a.rs", 'M')]), "head1");
    let unknown = folding.unknown(["gen.rs"]);
    let epoch = folding.asking(&unknown);
    folding.arrived(
        epoch,
        unknown,
        Ok(HashMap::from([("gen.rs".to_owned(), FoldAttr::Generated)])),
    );
    // The same status: still known.
    folding.saw_status(&work_with(&[("a.rs", 'M')]), "head1");
    assert!(folding.unknown(["gen.rs"]).is_empty());
    // Another commit checked out: asked again.
    folding.saw_status(&work_with(&[("a.rs", 'M')]), "head2");
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
    // A `.gitattributes` being edited: forgotten at every status read.
    let unknown = folding.unknown(["gen.rs"]);
    let epoch = folding.asking(&unknown);
    folding.arrived(epoch, unknown, Ok(HashMap::new()));
    folding.saw_status(&work_with(&[("sub/.gitattributes", 'M')]), "head2");
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
}

#[test]
fn an_answer_asked_before_forgetting_is_dropped() {
    let mut folding = Folding::default();
    let unknown = folding.unknown(["gen.rs"]);
    let epoch = folding.asking(&unknown);
    folding.saw_status(&work_with(&[(".gitattributes", 'M')]), "h");
    let stale = HashMap::from([("gen.rs".to_owned(), FoldAttr::Generated)]);
    folding.arrived(epoch, unknown, Ok(stale));
    assert_eq!(folding.reason(&rules(), "gen.rs"), None);
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
}

/// A repository with a `.gitattributes`, in a temporary folder removed when dropped.
struct Repo(std::path::PathBuf);

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-fold-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            git::run(Some(&repo.0), args)?;
        }
        Ok(repo)
    }

    fn write(&self, path: &str, body: &str) -> Result<(), String> {
        let file = self.0.join(path);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(file, body).map_err(|e| e.to_string())
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn gitattributes_are_read_from_a_real_repository() -> Result<(), String> {
    let repo = Repo::new("attrs")?;
    repo.write(
        ".gitattributes",
        "gen/** linguist-generated\n*.dat -diff\nCargo.lock linguist-generated=false\n\
         \"with space.rs\" linguist-generated=true\n",
    )?;
    repo.write("sub/.gitattributes", "ünï.txt linguist-generated\n")?;
    let paths: Vec<String> = [
        "gen/a/b.rs",
        "blob.dat",
        "Cargo.lock",
        "with space.rs",
        "sub/ünï.txt",
        "plain.rs",
        "-starts-with-dash.rs",
    ]
    .iter()
    .map(|p| (*p).to_owned())
    .collect();
    let found = git::fold_attributes(&repo.0, &paths)?;
    assert_eq!(found["gen/a/b.rs"], FoldAttr::Generated);
    assert_eq!(found["blob.dat"], FoldAttr::NoDiff);
    assert_eq!(found["Cargo.lock"], FoldAttr::NotGenerated);
    assert_eq!(found["with space.rs"], FoldAttr::Generated);
    assert_eq!(found["sub/ünï.txt"], FoldAttr::Generated);
    assert_eq!(found["plain.rs"], FoldAttr::Unspecified);
    assert_eq!(found["-starts-with-dash.rs"], FoldAttr::Unspecified);
    // Nothing asked, nothing run.
    assert!(git::fold_attributes(&repo.0, &[])?.is_empty());
    Ok(())
}

#[test]
fn thousands_of_paths_are_asked_in_a_few_commands() -> Result<(), String> {
    let repo = Repo::new("many")?;
    repo.write(".gitattributes", "*.gen linguist-generated\n")?;
    let paths: Vec<String> = (0..5000)
        .map(|i| format!("some/rather/deep/folder/file number {i}.gen"))
        .collect();
    let found = git::fold_attributes(&repo.0, &paths)?;
    assert_eq!(found.len(), paths.len());
    assert!(found.values().all(|a| *a == FoldAttr::Generated));
    Ok(())
}

#[test]
fn a_folder_that_is_no_repository_says_so() {
    let dir = std::env::temp_dir().join(format!("gitcraft-fold-none-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let result = git::fold_attributes(&dir.join("missing"), &["a".to_owned()]);
    let _ = std::fs::remove_dir_all(&dir);
    let Err(e) = result else {
        unreachable!("a missing folder has no attributes")
    };
    assert!(e.starts_with("Couldn't read .gitattributes"), "{e}");
}

/// A lockfile's diff, read as the diff pane reads it: its folded line counts its lines.
#[test]
fn a_folded_diff_counts_the_lines_of_its_change() -> Result<(), String> {
    let repo = Repo::new("counts")?;
    let lock: String = (0..1500).map(|i| format!("pkg-{i} = \"1.0\"\n")).collect();
    repo.write("Cargo.lock", &lock)?;
    git::run(Some(&repo.0), &["add", "-A"])?;
    git::run(Some(&repo.0), &["commit", "-qm", "one"])?;
    let changed = lock.replace("= \"1.0\"\npkg-1", "= \"2.0\"\npkg-1");
    repo.write("Cargo.lock", &changed)?;
    let key = crate::repo::DiffKey::Work(StatusFile {
        path: "Cargo.lock".to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
    let (added, removed) = banner::line_counts(&data);
    let rules = rules();
    let reason = Folding::default()
        .reason(&rules, "Cargo.lock")
        .ok_or("not folded")?;
    let changed_lines = lock
        .lines()
        .zip(changed.lines())
        .filter(|(a, b)| a != b)
        .count();
    let n = u32::try_from(changed_lines).map_err(|e| e.to_string())?;
    assert_eq!((added, removed), (Some(n), Some(n)));
    assert_eq!(
        stats_line(added, removed, reason),
        format!("+{n} −{n}, lockfile")
    );
    Ok(())
}
