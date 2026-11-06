//! Links made on real repositories, as the menus ask for them: the remote picked, the revision
//! made a commit's hash, and the warning when it isn't pushed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{LinkMenu, LinkRequest};
use crate::forge::links::{self, Lines, Link, Target};
use crate::git;
use crate::repo::{Action, RepoTab};

/// A fresh folder named after `name`, with a repository in it.
fn empty_repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-link-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        git(&dir, args)?;
    }
    Ok(dir)
}

/// A repository with one commit on `main` and these remotes. None is reached: their branches
/// are made as a fetch would leave them (see [`fetched`]).
fn repo(name: &str, remotes: &[(&str, &str)]) -> Result<PathBuf, String> {
    let dir = empty_repo(name)?;
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "base"])?;
    for (remote, url) in remotes {
        git(&dir, &["remote", "add", remote, url])?;
    }
    Ok(dir)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map_err(|e| format!("git {args:?}: {e}"))
}

/// What a fetch of `remote` would leave: its `branch` at `rev`.
fn fetched(dir: &Path, remote: &str, branch: &str, rev: &str) -> Result<(), String> {
    let name = format!("refs/remotes/{remote}/{branch}");
    git(dir, &["update-ref", &name, rev]).map(drop)
}

/// Commits `text` as the file `path`; its hash.
fn commit(dir: &Path, path: &str, text: &str) -> Result<String, String> {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    git(dir, &["add", "--", path])?;
    git(dir, &["commit", "-q", "-m", "change"])?;
    Ok(git::head(dir))
}

fn resolve(dir: &Path, remote: &str, target: &Target) -> Result<Result<Link, String>, String> {
    let remotes = git::remotes(dir)?;
    Ok(links::resolve(dir, &remotes, remote, target))
}

fn at(rev: &str) -> Target {
    Target::Commit { rev: rev.into() }
}

/// Runs `check` on the repository `dir`, then removes it, whatever `check` found.
fn with(dir: PathBuf, check: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let result = check(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_pushed_commit_links_by_its_hash() -> Result<(), String> {
    with(
        repo("pushed", &[("origin", "git@github.com:o/r.git")])?,
        |dir| {
            let sha = git::head(dir);
            fetched(dir, "origin", "main", &sha)?;
            assert_eq!(
                resolve(dir, "origin", &at("HEAD"))?,
                Ok(Link {
                    url: format!("https://github.com/o/r/commit/{sha}"),
                    warning: None,
                })
            );
            Ok(())
        },
    )
}

#[test]
fn an_unpushed_commit_still_links_but_warns() -> Result<(), String> {
    let remotes = [
        ("origin", "https://gitlab.com/g/r.git"),
        ("fork", "git@gitlab.com:me/r.git"),
    ];
    with(repo("unpushed", &remotes)?, |dir| {
        fetched(dir, "origin", "main", &git::head(dir))?;
        let sha = commit(dir, "a.txt", "a\n")?;
        let nowhere = resolve(dir, "origin", &at(&sha))??;
        assert_eq!(
            nowhere.url,
            format!("https://gitlab.com/g/r/-/commit/{sha}")
        );
        let warning = nowhere.warning.ok_or("no warning")?;
        assert!(warning.contains(&sha[..8]), "{warning}");
        assert!(warning.contains("isn't pushed to any remote"), "{warning}");

        // Pushed to the fork only.
        fetched(dir, "fork", "topic", &sha)?;
        let elsewhere = resolve(dir, "origin", &at(&sha))??;
        let warning = elsewhere.warning.ok_or("no warning")?;
        assert!(warning.contains("isn't on origin"), "{warning}");
        assert!(warning.contains("only on fork"), "{warning}");
        let on_fork = resolve(dir, "fork", &at(&sha))??;
        assert_eq!(
            on_fork.url,
            format!("https://gitlab.com/me/r/-/commit/{sha}")
        );
        assert_eq!(on_fork.warning, None);
        Ok(())
    })
}

#[test]
fn a_file_with_spaces_and_unicode_links_to_its_lines() -> Result<(), String> {
    with(
        repo("file", &[("origin", "git@codeberg.org:o/r.git")])?,
        |dir| {
            let path = "dir with space/ünï #1.txt";
            let sha = commit(dir, path, "one\ntwo\nthree\n")?;
            fetched(dir, "origin", "main", &sha)?;
            let target = |path: &str, lines| Target::File {
                rev: "HEAD".into(),
                path: path.into(),
                lines,
            };
            let encoded = "dir%20with%20space/%C3%BCn%C3%AF%20%231.txt";
            let page = format!("https://codeberg.org/o/r/src/commit/{sha}/{encoded}");
            assert_eq!(
                resolve(dir, "origin", &target(path, Lines::new(3, 2)))?.map(|l| l.url),
                Ok(format!("{page}#L2-L3"))
            );
            assert_eq!(
                resolve(dir, "origin", &target(path, None))?.map(|l| l.url),
                Ok(page)
            );
            let missing = resolve(dir, "origin", &target("nope.txt", None))?;
            let missing = missing.err().ok_or("a link to a missing file")?;
            assert!(missing.contains("nope.txt isn't in commit"), "{missing}");
            Ok(())
        },
    )
}

#[test]
fn removed_lines_link_to_the_parent() -> Result<(), String> {
    with(
        repo("parent", &[("origin", "git@github.com:o/r.git")])?,
        |dir| {
            let before = commit(dir, "f.txt", "a\nb\n")?;
            let after = commit(dir, "f.txt", "a\n")?;
            fetched(dir, "origin", "main", &after)?;
            let target = Target::File {
                rev: format!("{after}^"),
                path: "f.txt".into(),
                lines: Lines::new(2, 2),
            };
            assert_eq!(
                resolve(dir, "origin", &target)?.map(|l| l.url),
                Ok(format!("https://github.com/o/r/blob/{before}/f.txt#L2"))
            );
            Ok(())
        },
    )
}

#[test]
fn a_branch_links_by_its_name_and_warns_when_the_remote_has_none() -> Result<(), String> {
    let remotes = [("my/fork", "https://gitea.com/me/r.git")];
    with(repo("branch", &remotes)?, |dir| {
        fetched(dir, "my/fork", "feature/x", "HEAD")?;
        let branch = |name: &str| Target::Branch { name: name.into() };
        assert_eq!(
            resolve(dir, "my/fork", &branch("feature/x"))?,
            Ok(Link {
                url: "https://gitea.com/me/r/src/branch/feature/x".into(),
                warning: None,
            })
        );
        let missing = resolve(dir, "my/fork", &branch("local-only"))??;
        assert_eq!(missing.url, "https://gitea.com/me/r/src/branch/local-only");
        let warning = missing.warning.ok_or("no warning")?;
        assert!(warning.contains("local-only isn't on my/fork"), "{warning}");
        Ok(())
    })
}

#[test]
fn a_self_hosted_server_is_told_by_git_config() -> Result<(), String> {
    let remotes = [("origin", "https://me@git.example.org:8443/team/app.git")];
    with(repo("self-hosted", &remotes)?, |dir| {
        let sha = git::head(dir);
        fetched(dir, "origin", "main", &sha)?;
        let unknown = resolve(dir, "origin", &at("HEAD"))?;
        let unknown = unknown.err().ok_or("a link to an unknown forge")?;
        assert!(
            unknown.contains("git config gitcraft.git.example.org.forge"),
            "{unknown}"
        );
        git(
            dir,
            &["config", "gitcraft.git.example.org.forge", "forgejo"],
        )?;
        assert_eq!(
            resolve(dir, "origin", &at("HEAD"))?.map(|l| l.url),
            Ok(format!(
                "https://git.example.org:8443/team/app/commit/{sha}"
            ))
        );
        Ok(())
    })
}

#[test]
fn what_cant_be_linked_says_why() -> Result<(), String> {
    with(
        repo("errors", &[("origin", "git@github.com:o/r.git")])?,
        |dir| {
            assert_eq!(
                resolve(dir, "upstream", &at("HEAD"))?,
                Err("There is no remote called upstream any more".into())
            );
            let bad = resolve(dir, "origin", &at("no-such-branch"))?;
            let bad = bad.err().ok_or("a link to no commit")?;
            assert!(bad.contains("names no commit"), "{bad}");
            let root = git::head(dir);
            let parent = resolve(dir, "origin", &at(&format!("{root}^")))?;
            assert_eq!(parent, Err(format!("{} has no parent commit", &root[..8])));
            Ok(())
        },
    )?;
    // A repository with no commit yet.
    with(empty_repo("unborn")?, |dir| {
        git(dir, &["remote", "add", "origin", "git@github.com:o/r.git"])?;
        assert!(resolve(dir, "origin", &at("HEAD"))?.is_err());
        Ok(())
    })
}

/// A tab on `dir` that has read its refs and remotes, as its first read does.
fn tab_on(dir: &Path, ctx: &egui::Context) -> Result<RepoTab, String> {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.refs = git::refs(dir)?;
    tab.remotes = git::remotes(dir)?;
    Ok(tab)
}

/// Applies `action` and waits for what the tab says of it.
fn status_after(
    tab: &mut RepoTab,
    action: Action,
    ctx: &egui::Context,
) -> Result<(String, bool), String> {
    tab.status = None;
    tab.apply(action, ctx);
    let start = Instant::now();
    while tab.status.is_none() && start.elapsed() < Duration::from_secs(20) {
        tab.poll(ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    tab.status.clone().ok_or_else(|| "no status".to_owned())
}

#[test]
fn the_tab_copies_the_link_in_the_background() -> Result<(), String> {
    with(
        repo("tab", &[("origin", "git@github.com:o/r.git")])?,
        |dir| {
            let sha = git::head(dir);
            let ctx = egui::Context::default();
            let mut tab = tab_on(dir, &ctx)?;
            let copy = |rev: &str| {
                Action::ForgeLink(LinkRequest {
                    target: at(rev),
                    remote: "origin".into(),
                    open: false,
                })
            };
            let url = format!("https://github.com/o/r/commit/{sha}");
            let (said, warns) = status_after(&mut tab, copy(&sha), &ctx)?;
            assert!(said.starts_with(&format!("Copied {url}. ")), "{said}");
            assert!(warns, "an unpushed commit is a warning");

            fetched(dir, "origin", "main", &sha)?;
            let pushed = status_after(&mut tab, copy(&sha), &ctx)?;
            assert_eq!(pushed, (format!("Copied {url}"), false));

            let (said, failed) = status_after(&mut tab, copy("nothing-here"), &ctx)?;
            assert!(said.starts_with("No link: "), "{said}");
            assert!(failed);
            Ok(())
        },
    )
}

fn remotes_of(menu: Option<LinkMenu>) -> Vec<String> {
    menu.map(|m| m.choices.into_iter().map(|c| c.remote).collect())
        .unwrap_or_default()
}

#[test]
fn menus_link_to_the_tracked_remote_else_origin_else_ask() -> Result<(), String> {
    let remotes = [
        ("upstream", "https://github.com/up/r.git"),
        ("fork", "git@github.com:me/r.git"),
        ("disk", "/srv/git/r.git"),
    ];
    with(repo("menus", &remotes)?, |dir| {
        fetched(dir, "fork", "main-on-fork", "HEAD")?;
        git(dir, &["branch", "topic"])?;
        let upstream = "--set-upstream-to=refs/remotes/fork/main-on-fork";
        git(dir, &["branch", upstream, "main"])?;
        let ctx = egui::Context::default();
        let tab = tab_on(dir, &ctx)?;
        let branch = |name: &str| {
            tab.branch_link_menu(name)
                .map(|(menu, target)| (remotes_of(Some(menu)), target))
        };
        let on_fork = Target::Branch {
            name: "main-on-fork".into(),
        };

        // `main` (checked out) tracks the fork: its links go there.
        assert_eq!(remotes_of(tab.link_menu(None)), ["fork"]);
        assert_eq!(branch("main"), Some((vec!["fork".into()], on_fork.clone())));
        assert_eq!(
            branch("fork/main-on-fork"),
            Some((vec!["fork".into()], on_fork))
        );
        // No upstream and no origin: a choice among the remotes on a forge, as git lists them.
        assert_eq!(
            remotes_of(tab.link_menu(Some("topic"))),
            ["fork", "upstream"]
        );
        let topic = Target::Branch {
            name: "topic".into(),
        };
        assert_eq!(
            branch("topic"),
            Some((vec!["fork".into(), "upstream".into()], topic))
        );
        // A remote on disk has no pages.
        fetched(dir, "disk", "main", "HEAD")?;
        let tab = tab_on(dir, &ctx)?;
        assert!(tab.branch_link_menu("disk/main").is_none());
        Ok(())
    })
}

#[test]
fn no_remote_on_a_forge_means_no_links() -> Result<(), String> {
    with(repo("no-forge", &[("origin", "/srv/git/r.git")])?, |dir| {
        let ctx = egui::Context::default();
        let tab = tab_on(dir, &ctx)?;
        assert!(tab.link_menu(None).is_none());
        assert!(tab.branch_link_menu("main").is_none());
        Ok(())
    })
}
