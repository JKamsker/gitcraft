//! Tests for the diff algorithm: passed to git for each kind of diff (never written to the
//! repository's config), kept per file type, and switched without losing one's place.

use super::algorithm::keep_algorithm;
use super::place::{offset_of_place, place_at};
use super::*;
use crate::git::{DiffAlgorithm, StatusFile};
use crate::settings::Settings;

const BEFORE: &str = "#include <stdio.h>\n\n// Frobs foo heartily\nint frobnitz(int foo)\n{\n    \
int i;\n    for(i = 0; i < 10; i++)\n    {\n        printf(\"Your answer is: \");\n        \
printf(\"%d\\n\", foo);\n    }\n}\n\nint fact(int n)\n{\n    if(n > 1)\n    {\n        return \
fact(n-1) * n;\n    }\n    return 1;\n}\n\nint main(int argc, char **argv)\n{\n    \
frobnitz(fact(10));\n}\n";

const AFTER: &str = "#include <stdio.h>\n\nint fib(int n)\n{\n    if(n > 2)\n    {\n        \
return fib(n-1) + fib(n-2);\n    }\n    return 1;\n}\n\n// Frobs foo heartily\nint frobnitz(int \
foo)\n{\n    int i;\n    for(i = 0; i < 10; i++)\n    {\n        printf(\"%d\\n\", foo);\n    }\n}\
\n\nint main(int argc, char **argv)\n{\n    frobnitz(fib(10));\n}\n";

/// A repository where `name` went from [`BEFORE`] (commit `from`) to [`AFTER`] (commit
/// `to`), and `w-name`, committed as [`BEFORE`], is [`AFTER`] in the working tree.
struct Repo {
    dir: std::path::PathBuf,
    from: String,
    to: String,
}

impl Repo {
    fn new(tag: &str, name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-alg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            git(args)?;
        }
        let work = format!("w-{name}");
        let write = |file: &str, body: &str| {
            std::fs::write(dir.join(file), body).map_err(|e| e.to_string())
        };
        write(name, BEFORE)?;
        write(&work, BEFORE)?;
        git(&["add", "-A"])?;
        git(&["commit", "-qm", "before"])?;
        let from = git(&["rev-parse", "HEAD"])?.trim().to_owned();
        write(name, AFTER)?;
        git(&["commit", "-qam", "after"])?;
        let to = git(&["rev-parse", "HEAD"])?.trim().to_owned();
        write(&work, AFTER)?;
        Ok(Repo { dir, from, to })
    }

    fn keys(&self, name: &str) -> [DiffKey; 3] {
        [
            DiffKey::Commit {
                hash: self.to.clone(),
                path: name.to_owned(),
            },
            DiffKey::Range {
                from: self.from.clone(),
                to: Some(self.to.clone()),
                path: name.to_owned(),
            },
            DiffKey::Work(StatusFile {
                path: format!("w-{name}"),
                old_path: None,
                code: 'M',
                area: Area::Unstaged,
            }),
        ]
    }

    fn load(&self, key: &DiffKey, algorithm: Option<DiffAlgorithm>) -> Result<DiffData, String> {
        let opts = DiffOptions {
            algorithm,
            ..DiffOptions::default()
        };
        load(&self.dir, key, opts, true)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Whether the comment line stays as context (patience pairs it) or is removed and added
/// again (Myers pairs the braces instead).
fn comment_kept(data: &DiffData) -> bool {
    data.lines
        .iter()
        .any(|l| l.kind == Kind::Context && l.text == "// Frobs foo heartily")
}

#[test]
fn patience_and_myers_pair_lines_differently_in_every_kind_of_diff() -> Result<(), String> {
    let name = "frob nitz ü.c";
    let repo = Repo::new("pair", name)?;
    for key in repo.keys(name) {
        let myers = repo.load(&key, Some(DiffAlgorithm::Myers))?;
        let minimal = repo.load(&key, Some(DiffAlgorithm::Minimal))?;
        let patience = repo.load(&key, Some(DiffAlgorithm::Patience))?;
        let histogram = repo.load(&key, Some(DiffAlgorithm::Histogram))?;
        let default = repo.load(&key, None)?;
        assert_eq!(default.lines, myers.lines, "{key:?}");
        assert_ne!(myers.lines, patience.lines, "{key:?}");
        assert_eq!(patience.lines, histogram.lines, "{key:?}");
        assert!(!minimal.lines.is_empty(), "{key:?}");
        // From BEFORE to AFTER, Myers pairs the braces and patience the comment.
        assert!(!comment_kept(&myers), "{key:?}");
        assert!(!comment_kept(&minimal), "{key:?}");
        assert!(comment_kept(&patience), "{key:?}");
        assert!(comment_kept(&histogram), "{key:?}");
    }
    Ok(())
}

/// The choice goes on the command line: it wins over `diff.algorithm` in the repository's
/// config, and the config is left as it was.
#[test]
fn the_choice_wins_over_the_config_and_never_changes_it() -> Result<(), String> {
    let name = "f.c";
    let repo = Repo::new("config", name)?;
    git::run(Some(&repo.dir), &["config", "diff.algorithm", "patience"])?;
    let [commit, ..] = repo.keys(name);
    assert!(comment_kept(&repo.load(&commit, None)?));
    assert!(!comment_kept(
        &repo.load(&commit, Some(DiffAlgorithm::Myers))?
    ));
    let config = git::run(Some(&repo.dir), &["config", "--local", "--list"])?;
    assert!(config.contains("diff.algorithm=patience"), "{config}");
    assert!(!config.contains("myers"), "{config}");
    Ok(())
}

/// Lines picked from a patience diff still apply: staging works whatever the pairing.
#[test]
fn lines_picked_from_a_patience_diff_stage() -> Result<(), String> {
    let name = "f.c";
    let repo = Repo::new("stage", name)?;
    let [.., work] = repo.keys(name);
    let data = repo.load(&work, Some(DiffAlgorithm::Patience))?;
    assert!(data.partial);
    let chosen = hunk_changes(&data.lines, 0);
    let patch = partial_patch(&data.header, &data.lines, &chosen, Pick::Stage).ok_or("none")?;
    let saved = git::save_patch(&patch)?;
    let applied = git::run(
        Some(&repo.dir),
        &["apply", "--cached", "--check", &saved.to_string_lossy()],
    );
    let _ = std::fs::remove_file(saved);
    applied.map(|_| ())
}

#[test]
fn the_choice_is_kept_per_file_type() {
    let mut settings = Settings::default();
    assert_eq!(algorithm::algorithm_for(&settings, "src/a.rs"), None);
    assert!(keep_algorithm(
        &mut settings,
        "src/a.rs",
        Some(DiffAlgorithm::Patience)
    ));
    assert!(!keep_algorithm(
        &mut settings,
        "b.RS",
        Some(DiffAlgorithm::Patience)
    ));
    assert_eq!(
        algorithm::algorithm_for(&settings, "other/Main.rs"),
        Some(DiffAlgorithm::Patience)
    );
    assert_eq!(algorithm::algorithm_for(&settings, "a.py"), None);
    assert!(keep_algorithm(
        &mut settings,
        "Makefile",
        Some(DiffAlgorithm::Histogram)
    ));
    assert_eq!(
        algorithm::algorithm_for(&settings, "sub/makefile"),
        Some(DiffAlgorithm::Histogram)
    );
    // Back to git's default: forgotten.
    assert!(keep_algorithm(&mut settings, "a.rs", None));
    assert_eq!(algorithm::algorithm_for(&settings, "a.rs"), None);
    assert_eq!(settings.diff_algorithms.len(), 1);
    // And it survives a restart.
    let mut storage = TestStorage::default();
    eframe::set_value(&mut storage, "s", &settings);
    let back: Option<Settings> = eframe::get_value(&storage, "s");
    assert_eq!(
        back.map(|s| s.diff_algorithms),
        Some(settings.diff_algorithms)
    );
}

#[derive(Default)]
struct TestStorage(std::collections::HashMap<String, String>);

impl eframe::Storage for TestStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

/// Switching algorithm keeps the line at the top of the view at the top, though its row moves.
#[test]
fn the_line_at_the_top_stays_there_when_the_pairing_changes() -> Result<(), String> {
    let name = "f.c";
    let repo = Repo::new("place", name)?;
    let [commit, ..] = repo.keys(name);
    let myers = repo.load(&commit, Some(DiffAlgorithm::Myers))?;
    let patience = repo.load(&commit, Some(DiffAlgorithm::Patience))?;
    for split in [false, true] {
        // The row showing `int main` in the new file.
        let rows = if split {
            myers.split.len()
        } else {
            myers.lines.len()
        };
        let main_row = (0..rows)
            .find(|&r| {
                let i = if split { myers.split[r].1 } else { Some(r) };
                i.is_some_and(|i| myers.lines[i].text.starts_with("int main"))
            })
            .ok_or("no main")?;
        let offset = main_row as f32 * LINE_H;
        let place = place_at(&myers, split, offset).ok_or("no place")?;
        let back = offset_of_place(&patience, split, place);
        let row = (back / LINE_H) as usize;
        let i = if split {
            patience.split[row].1
        } else {
            Some(row)
        };
        let text = i.map(|i| patience.lines[i].text.as_str());
        assert_eq!(
            text,
            Some("int main(int argc, char **argv)"),
            "split: {split}"
        );
        // Past the end, or of an empty diff: the top.
        assert_eq!(place_at(&myers, split, 1.0e9), None);
    }
    Ok(())
}
