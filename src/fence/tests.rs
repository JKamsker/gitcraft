use std::path::{Path, PathBuf};

use super::configs::{agents_md, claude_settings, cursorignore};
use super::{How, Pattern, check, parse_list, read, write, write_all};

fn list(items: &[&str]) -> Vec<Pattern> {
    parse_list(&items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
}

#[test]
fn patterns_match_like_gitignore() {
    let cases: [(&str, &[&str], &[&str]); 8] = [
        (
            ".env",
            &[".env", "app/.env", ".env/x"],
            &[".envrc", "x.env", "env"],
        ),
        (
            ".env.*",
            &[".env.local", "a/.env.prod"],
            &[".env", "env.local"],
        ),
        (
            "secrets/",
            &["secrets/a", "x/secrets/b/c"],
            &["secretsx/a", "my-secrets"],
        ),
        (
            ".github/workflows/",
            &[".github/workflows/ci.yml"],
            &["x/.github/workflows/ci.yml", ".github/ci.yml"],
        ),
        ("/Cargo.lock", &["Cargo.lock"], &["crates/a/Cargo.lock"]),
        (
            "Cargo.lock",
            &["Cargo.lock", "crates/a/Cargo.lock"],
            &["Cargo.toml"],
        ),
        (
            "**/migrations/*.sql",
            &["db/migrations/1.sql", "migrations/2.sql"],
            &["db/migrations/a/1.sql", "db/migrations/1.rb"],
        ),
        (
            "dö cs/ünï?.md",
            &["dö cs/ünï1.md"],
            &["dö cs/ünï12.md", "ünï1.md"],
        ),
    ];
    for (pattern, yes, no) in cases {
        let p = Pattern::parse(pattern);
        assert!(p.is_some(), "{pattern} parses");
        let Some(p) = p else { continue };
        for path in yes {
            assert!(p.matches(path), "{pattern} should fence {path}");
        }
        for path in no {
            assert!(!p.matches(path), "{pattern} should not fence {path}");
        }
    }
    assert!(Pattern::parse("").is_none());
    assert!(Pattern::parse("  # a comment").is_none());
    assert!(Pattern::parse("/").is_none());
    assert!(Pattern::parse("a\\b").is_some_and(|p| p.matches("a/b")));
}

#[test]
fn claude_rules_replace_their_old_ones_and_keep_the_rest() -> Result<(), String> {
    let mine =
        r#"{"model": "x", "permissions": {"deny": ["Bash(rm:*)"], "allow": ["Read(./src/**)"]}}"#;
    let first = claude_settings(mine, &[], &list(&[".env", "/db/"]))?;
    let v: serde_json::Value = serde_json::from_str(&first).map_err(|e| e.to_string())?;
    let deny: Vec<&str> = v["permissions"]["deny"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        deny,
        vec![
            "Bash(rm:*)",
            "Read(./**/.env)",
            "Read(./**/.env/**)",
            "Edit(./**/.env)",
            "Edit(./**/.env/**)",
            "Read(./db)",
            "Read(./db/**)",
            "Edit(./db)",
            "Edit(./db/**)",
        ]
    );
    assert_eq!(v["model"], "x");
    assert_eq!(v["permissions"]["allow"][0], "Read(./src/**)");
    // The list changed: the old rules go, the user's stay.
    let second = claude_settings(&first, &list(&[".env", "/db/"]), &list(&["/db/"]))?;
    assert!(!second.contains(".env"));
    assert!(second.contains("Bash(rm:*)") && second.contains("Edit(./db/**)"));
    assert!(claude_settings("[1]", &[], &[]).is_err());
    assert!(claude_settings("{not json", &[], &[]).is_err());
    assert!(claude_settings(r#"{"permissions": {"deny": "x"}}"#, &[], &[]).is_err());
    Ok(())
}

#[test]
fn blocks_are_replaced_in_place_and_removed_when_empty() {
    let start = "node_modules/\n";
    let one = cursorignore(start, &list(&[".env"]));
    assert_eq!(
        one,
        "node_modules/\n\n# gitcraft agent fence: start\n.env\n# gitcraft agent fence: end\n"
    );
    let edited = format!("{one}dist/\n");
    let two = cursorignore(&edited, &list(&["secrets/", ".env"]));
    assert_eq!(
        two,
        "node_modules/\n\n# gitcraft agent fence: start\nsecrets/\n.env\n# gitcraft agent fence: end\ndist/\n"
    );
    let gone = cursorignore(&two, &[]);
    assert_eq!(gone, "node_modules/\n\ndist/\n");
    let md = agents_md("# Repo\n\nBuild with make.\n", &list(&[".env"]));
    assert!(md.starts_with("# Repo\n\nBuild with make.\n\n<!-- gitcraft agent fence: start -->\n## Paths agents must not touch"));
    assert!(md.contains("- `.env`\n<!-- gitcraft agent fence: end -->\n"));
    assert_eq!(agents_md(&md, &[]), "# Repo\n\nBuild with make.\n");
    assert_eq!(agents_md("", &[]), "");
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-fence-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(&dir, args)?;
    }
    std::fs::write(dir.join(".gitignore"), ".env\nbuild/\n").map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join(".github/workflows")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(".github/workflows/ci.yml"), "on: push\n")
        .map_err(|e| e.to_string())?;
    std::fs::write(dir.join("src main.rs"), "fn main() {}\n").map_err(|e| e.to_string())?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "base"])?;
    Ok(dir)
}

#[test]
fn the_list_lives_in_the_repositorys_config() -> Result<(), String> {
    let dir = repo("store")?;
    assert!(read(&dir)?.is_empty());
    let entries: Vec<String> = [".env", " secrets/ ", "", "dö cs/ünï.md"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    write(&dir, &entries)?;
    assert_eq!(read(&dir)?, vec![".env", "secrets/", "dö cs/ünï.md"]);
    write(&dir, &[])?;
    assert!(read(&dir)?.is_empty());
    assert!(write(&dir, &["a\nb".to_owned()]).is_err());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_worktree_that_touches_the_fence_in_every_way() -> Result<(), String> {
    let dir = repo("check")?;
    let base = git(&dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    let wt = dir.with_extension("wt").join("agent one");
    let wt_s = wt.to_string_lossy().into_owned();
    git(
        &dir,
        &["worktree", "add", "-q", "-b", "agent/one", &wt_s, "main"],
    )?;
    let fence = list(&[".env", ".github/workflows/", "secrets/", "*.pem"]);
    // A fresh worktree touches nothing; one with only allowed changes neither.
    assert!(check(&wt, &base, &fence)?.is_empty());
    std::fs::write(wt.join("src main.rs"), "fn main() { x() }\n").map_err(|e| e.to_string())?;
    assert!(check(&wt, &base, &fence)?.is_empty());

    // Committed: the workflow changed. Uncommitted: it changed again. Untracked: a key.
    // Ignored: an .env that `git worktree add` never makes, written by the agent.
    std::fs::write(wt.join(".github/workflows/ci.yml"), "on: [push]\n")
        .map_err(|e| e.to_string())?;
    git(&wt, &["commit", "-qam", "agent work"])?;
    std::fs::write(wt.join(".github/workflows/ci.yml"), "on: pull_request\n")
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(wt.join("secrets")).map_err(|e| e.to_string())?;
    std::fs::write(wt.join("secrets/ä key.pem"), "k").map_err(|e| e.to_string())?;
    std::fs::write(wt.join(".env"), "TOKEN=1").map_err(|e| e.to_string())?;
    let found = check(&wt, &base, &fence)?;
    let seen: Vec<(How, &str)> = found.iter().map(|v| (v.how, v.path.as_str())).collect();
    assert_eq!(
        seen,
        vec![
            (How::Committed, ".github/workflows/ci.yml"),
            (How::Uncommitted, ".github/workflows/ci.yml"),
            (How::Untracked, "secrets/ä key.pem"),
            (How::Ignored, ".env"),
        ]
    );
    assert_eq!(found[2].pattern, "secrets/");
    // Nothing fenced: nothing found, nothing run.
    assert!(check(&wt, &base, &[])?.is_empty());
    // A worktree gone: an error saying which.
    let gone = check(&dir.join("nowhere"), &base, &fence);
    assert!(gone.is_err_and(|e| e.starts_with("Could not check")));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    Ok(())
}

#[test]
fn configs_are_written_into_a_worktree_and_rewritten() -> Result<(), String> {
    let dir = repo("configs")?;
    std::fs::write(dir.join("AGENTS.md"), "# Notes\n").map_err(|e| e.to_string())?;
    let changed = write_all(&dir, &[], &list(&[".env", "migrations/"]))?;
    assert_eq!(changed, vec![super::CLAUDE, super::CURSOR, super::AGENTS]);
    let read = |rel: &str| std::fs::read_to_string(dir.join(rel)).unwrap_or_default();
    assert!(read(super::CLAUDE).contains("Edit(./**/migrations/**)"));
    assert!(read(super::CURSOR).contains("migrations/"));
    assert!(read(super::AGENTS).starts_with("# Notes\n"));
    // The same again: nothing to write.
    assert!(
        write_all(
            &dir,
            &list(&[".env", "migrations/"]),
            &list(&[".env", "migrations/"])
        )?
        .is_empty()
    );
    // An empty list where nothing was: no files made for it.
    let empty = repo("configs-empty")?;
    assert!(write_all(&empty, &[], &[])?.is_empty());
    assert!(!empty.join(".claude").exists());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&empty);
    Ok(())
}
