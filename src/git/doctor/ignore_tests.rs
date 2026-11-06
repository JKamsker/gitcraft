//! Ignore rules and attributes against real repositories: nested `.gitignore` files, negation,
//! tracked files, attributes and line endings, and the preview of an edit.

use super::ignore::{self, explain};
use super::rules;
use super::tests::{git, repo, write};

#[test]
fn the_rule_responsible_is_found_through_nested_files_and_negation() -> Result<(), String> {
    let dir = repo(
        "ignore-nested",
        &[
            (".gitignore", b"*.log\nbuild/\n"),
            ("sub dir/.gitignore", b"!keep.log\n/local.txt\n"),
            ("tracked.log", b"t\n"),
        ],
    )?;
    let result = (|| {
        // Tracked before the rule existed: it stays tracked.
        git(&dir, &["add", "-f", "tracked.log"])?;
        git(&dir, &["commit", "-q", "-m", "tracked log"])?;
        for f in [
            "a.log",
            "sub dir/keep.log",
            "sub dir/local.txt",
            "build/out/é x.bin",
            "free.txt",
        ] {
            write(&dir.join(f), b"x\n")?;
        }
        let a = explain(&dir, "a.log")?;
        assert!(a.ignored());
        let rule = a.rule.clone().ok_or("no rule")?;
        assert_eq!(
            (rule.source.as_str(), rule.line, rule.pattern.as_str()),
            (".gitignore", 1, "*.log")
        );

        let keep = explain(&dir, "sub dir/keep.log")?;
        assert!(!keep.ignored());
        let rule = keep.rule.ok_or("no rule")?;
        assert!(rule.negated());
        assert_eq!(rule.source, "sub dir/.gitignore");

        let local = explain(&dir, "./sub dir/local.txt")?;
        assert!(local.ignored());
        assert_eq!(local.path, "sub dir/local.txt");

        // Inside an ignored folder: the folder's rule.
        let deep = explain(&dir, "build/out/é x.bin")?;
        assert_eq!(deep.rule.map(|r| r.pattern), Some("build/".to_owned()));

        let tracked = explain(&dir, "tracked.log")?;
        assert!(tracked.tracked && !tracked.ignored() && tracked.rule.is_some());

        let free = explain(&dir, "free.txt")?;
        assert!(free.rule.is_none() && !free.ignored());

        let (list, total) = ignore::ignored_files(&dir, 100)?;
        assert!(list.iter().any(|p| p == "a.log"), "{list:?}");
        assert!(list.iter().any(|p| p.starts_with("build/")), "{list:?}");
        assert!(!list.iter().any(|p| p == "sub dir/keep.log"));
        let (cut, all) = ignore::ignored_files(&dir, 1)?;
        assert_eq!((cut.len(), all), (1, total));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn attributes_line_endings_and_missing_drivers() -> Result<(), String> {
    let dir = repo(
        "ignore-attrs",
        &[
            (
                ".gitattributes",
                b"*.txt text eol=lf\n*.png binary\n*.psd filter=lfs diff=lfs merge=lfs -text\n\
                  *.lock merge=ours-driver\n*.csv merge=union\n",
            ),
            ("a b.txt", b"one\r\ntwo\r\n"),
            ("p.png", b"\x89PNG\x00"),
            ("x.lock", b"l\n"),
        ],
    )?;
    let result = (|| {
        let txt = explain(&dir, "a b.txt")?;
        let get = |e: &ignore::Explained, n: &str| {
            e.attrs
                .iter()
                .find(|a| a.name == n)
                .map(|a| a.value.clone())
        };
        assert_eq!(get(&txt, "eol").as_deref(), Some("lf"));
        assert_eq!(get(&txt, "text").as_deref(), Some("set"));
        let eol = txt.eol.clone().ok_or("no line endings")?;
        // Stored as LF, written out as it was: CRLF in the working tree.
        assert_eq!(eol.index, "lf");
        assert_eq!(eol.worktree, "crlf");
        assert!(eol.attr.contains("eol=lf"), "{eol:?}");

        let png = explain(&dir, "p.png")?;
        assert_eq!(get(&png, "binary").as_deref(), Some("set"));
        assert!(
            ignore::meaning(&png.attrs)
                .iter()
                .any(|m| m.starts_with("binary"))
        );

        // A merge driver nobody defined: git quietly merges as usual.
        let lock = explain(&dir, "x.lock")?;
        assert_eq!(
            lock.missing_drivers,
            [("merge".to_owned(), "ours-driver".to_owned())]
        );
        git(&dir, &["config", "merge.ours-driver.driver", "true"])?;
        assert!(explain(&dir, "x.lock")?.missing_drivers.is_empty());
        // Built in: never missing.
        assert!(explain(&dir, "data.csv")?.missing_drivers.is_empty());

        // A file that doesn't exist still has the attributes its name matches.
        let gone = explain(&dir, "nowhere/new.psd")?;
        assert_eq!(get(&gone, "filter").as_deref(), Some("lfs"));
        assert!(gone.eol.is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn editing_rules_previews_and_saves() -> Result<(), String> {
    let dir = repo(
        "ignore-edit",
        &[
            ("sub/.gitignore", b"*.tmp\n"),
            ("sub/kept.cfg", b"tracked\n"),
        ],
    )?;
    let result = (|| {
        for f in [
            "sub/a.tmp",
            "sub/deep/b.tmp",
            "sub/c.cfg",
            "top.cfg",
            "top.tmp",
        ] {
            write(&dir.join(f), b"x\n")?;
        }
        let files = rules::files(&dir)?;
        let labels: Vec<&str> = files.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels[0], ".gitignore");
        assert!(labels.contains(&"sub/.gitignore"), "{labels:?}");
        assert!(
            labels.iter().any(|l| l.ends_with("info/exclude")),
            "{labels:?}"
        );
        let sub = files
            .iter()
            .find(|f| f.label == "sub/.gitignore")
            .ok_or("no sub/.gitignore")?
            .clone();
        assert_eq!(sub.base, "sub");
        let old = rules::read(&sub)?;
        assert_eq!(old, "*.tmp\n");

        // Swap *.tmp for *.cfg: within sub only, the top's files are no business of this file.
        let new = "*.cfg\n";
        let preview = rules::preview(&dir, &sub, &old, new)?;
        assert_eq!(preview.newly_ignored, ["sub/c.cfg"]);
        assert_eq!(preview.no_longer_ignored, ["sub/a.tmp", "sub/deep/b.tmp"]);
        assert_eq!(preview.tracked_matching, ["sub/kept.cfg"]);

        // Changed elsewhere since it was read: not written over.
        write(&dir.join("sub/.gitignore"), b"*.tmp\n*.bak\n")?;
        assert!(rules::save(&sub, &old, new).is_err());
        let now = rules::read(&sub)?;
        rules::save(&sub, &now, new)?;
        assert_eq!(rules::read(&sub)?, new);
        assert!(explain(&dir, "sub/c.cfg")?.ignored());
        assert!(!explain(&dir, "sub/a.tmp")?.ignored());

        // The top .gitignore, made by saving it.
        let top = files[0].clone();
        assert_eq!(rules::read(&top)?, "");
        rules::save(&top, "", "/top.tmp\n")?;
        assert!(explain(&dir, "top.tmp")?.ignored());
        // An empty edit ignores nothing.
        let empty = rules::preview(&dir, &top, "", "")?;
        assert_eq!(empty, rules::Preview::default());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(unix)]
#[test]
fn a_rules_file_that_is_a_link_is_not_written_through() -> Result<(), String> {
    let dir = repo("ignore-link", &[])?;
    let result = (|| {
        let target = dir.join("elsewhere");
        write(&target, b"x\n")?;
        std::os::unix::fs::symlink(&target, dir.join(".gitignore")).map_err(|e| e.to_string())?;
        let files = rules::files(&dir)?;
        let top = files[0].clone();
        let err = rules::save(&top, "x\n", "*\n");
        assert!(err.is_err_and(|e| e.contains("symbolic link")));
        assert_eq!(std::fs::read(&target).map_err(|e| e.to_string())?, b"x\n");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
