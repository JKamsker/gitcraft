//! Reading links into gitcraft, and refusing the rest.

use super::*;

fn web(url: &str) -> Result<(String, String, Target), String> {
    parse_web_url(url).map(|l| (l.host, l.repo, l.target))
}

fn ok(host: &str, repo: &str, target: Target) -> Result<(String, String, Target), String> {
    Ok((host.to_owned(), repo.to_owned(), target))
}

fn file(rev: &str, path: &str, line: Option<u32>) -> Target {
    Target::File {
        rev: rev.to_owned(),
        path: path.to_owned(),
        line,
    }
}

#[test]
fn github() {
    assert_eq!(
        web("https://github.com/JKamsker/gitcraft"),
        ok("github.com", "JKamsker/gitcraft", Target::Repo)
    );
    assert_eq!(
        web("https://github.com/o/r/pull/12/files?diff=split"),
        ok("github.com", "o/r", Target::PullRequest(12))
    );
    assert_eq!(
        web("https://GitHub.com/o/r/commit/ABCDEF1234"),
        ok("github.com", "o/r", Target::Commit("abcdef1234".into()))
    );
    assert_eq!(
        web("https://github.com/o/r/blob/main/src/my%20file%20%C3%BC.rs#L40-L45"),
        ok(
            "github.com",
            "o/r",
            file("main", "src/my file ü.rs", Some(40))
        )
    );
    // A folder named `-` in a GitHub path is a folder.
    assert_eq!(
        web("https://github.com/o/r/blob/main/-/x.rs"),
        ok("github.com", "o/r", file("main", "-/x.rs", None))
    );
    assert_eq!(
        web("https://github.com/o/r.git"),
        ok("github.com", "o/r", Target::Repo)
    );
}

#[test]
fn gitlab_with_groups() {
    assert_eq!(
        web("https://gitlab.com/g/sub/r/-/merge_requests/7/diffs"),
        ok("gitlab.com", "g/sub/r", Target::PullRequest(7))
    );
    assert_eq!(
        web("https://gitlab.example.com:8443/g/r/-/commit/0123abcd"),
        ok(
            "gitlab.example.com:8443",
            "g/r",
            Target::Commit("0123abcd".into())
        )
    );
    assert_eq!(
        web("https://gitlab.com/g/r/-/blob/v1.0/README.md#L3"),
        ok("gitlab.com", "g/r", file("v1.0", "README.md", Some(3)))
    );
}

#[test]
fn gitea_forgejo_and_bitbucket() {
    assert_eq!(
        web("https://codeberg.org/o/r/pulls/12"),
        ok("codeberg.org", "o/r", Target::PullRequest(12))
    );
    assert_eq!(
        web("https://codeberg.org/o/r/src/branch/feature/x/docs/a.md#L40"),
        // The branch is one name here: the rest is the path, as far as a link can tell.
        ok(
            "codeberg.org",
            "o/r",
            file("feature", "x/docs/a.md", Some(40))
        )
    );
    assert_eq!(
        web("https://bitbucket.org/o/r/pull-requests/3/overview"),
        ok("bitbucket.org", "o/r", Target::PullRequest(3))
    );
    assert_eq!(
        web("https://bitbucket.org/o/r/src/abc123/lib/x.py#lines-9"),
        ok("bitbucket.org", "o/r", file("abc123", "lib/x.py", Some(9)))
    );
}

#[test]
fn what_is_refused() {
    for url in [
        "ftp://github.com/o/r",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "https://github.com/o",
        "https://github.com/",
        "https://user:pass@github.com/o/r",
        "https://github.com/o/r/pull/abc",
        "https://github.com/o/r/pull/0",
        "https://github.com/o/r/commit/xyz",
        "https://github.com/o/r/blob/--output=x/a.rs",
        "https://github.com/o/../r",
        "https://github.com/o/r%2F..%2Fx",
        "https://github.com/o/r/issues/3",
        "https://localhost/o/r",
        "https://-bad.com/o/r",
        "https://github.com:99999/o/r",
        "https://github.com/o\n/r",
        "https://github.com/o/r with space",
        "https://github.com/o/%ff",
        "https://github.com/o/%zz",
    ] {
        assert!(parse_web_url(url).is_err(), "{url}");
    }
    assert!(parse_web_url(&format!("https://github.com/o/{}", "r".repeat(5000))).is_err());
    assert!(parse_web_url("").is_err());
}

#[test]
fn gitcraft_links() {
    let link =
        parse_scheme_link("gitcraft://open?url=https%3A%2F%2Fcodeberg.org%2Fo%2Fr%2Fpulls%2F12");
    assert_eq!(
        link.map(|l| (l.host, l.repo, l.target)),
        ok("codeberg.org", "o/r", Target::PullRequest(12))
    );
    // Not encoded at all works too, and a trailing slash after `open`.
    assert!(parse_scheme_link("GITCRAFT://open/?url=https://github.com/o/r").is_ok());
    for link in [
        "gitcraft://run?url=https%3A%2F%2Fgithub.com%2Fo%2Fr",
        "gitcraft://open",
        "gitcraft://open?url=",
        "gitcraft://open?path=/etc",
        "gitcraft://open?url=https://github.com/o/r&url=https://evil.example/o/r",
        "gitcraft://open?url=https://github.com/o/r&cmd=rm",
        "gitcraft://open?url=%E0%A4%A",
        "gitcraft://open?url=file%3A%2F%2F%2Fetc",
        "https://github.com/o/r",
    ] {
        assert!(parse_scheme_link(link).is_err(), "{link}");
    }
}

#[test]
fn matching_a_clone_by_its_remote() -> Result<(), String> {
    let link = parse_web_url("https://github.com/JKamsker/GitCraft/pull/1")?;
    for remote in [
        "https://github.com/JKamsker/gitcraft.git",
        "git@github.com:jkamsker/gitcraft.git",
        "ssh://git@ssh.github.com:443/JKamsker/gitcraft",
    ] {
        assert!(link.is_remote(remote), "{remote}");
    }
    for remote in [
        "https://github.com/JKamsker/gitcraft-fork.git",
        "https://gitlab.com/JKamsker/gitcraft.git",
        "/home/me/gitcraft",
    ] {
        assert!(!link.is_remote(remote), "{remote}");
    }
    let gitlab = parse_web_url("https://gitlab.example.com:8443/g/sub/r/-/merge_requests/2")?;
    assert!(gitlab.is_remote("git@gitlab.example.com:g/sub/r.git"));
    assert_eq!(
        gitlab.clone_url(),
        "https://gitlab.example.com:8443/g/sub/r.git"
    );
    Ok(())
}

#[test]
fn folders_and_gitlab_paths_that_look_like_others() {
    // A Gitea / Forgejo branch or folder page, and a GitLab folder: the repository.
    assert_eq!(
        web("https://codeberg.org/o/r/src/branch/main"),
        ok("codeberg.org", "o/r", Target::Repo)
    );
    assert_eq!(
        web("https://gitlab.com/g/r/-/tree/main/src"),
        ok("gitlab.com", "g/r", Target::Repo)
    );
    // A GitLab group or project named like another forge's keyword.
    assert_eq!(
        web("https://gitlab.com/org/team/src/-/merge_requests/3"),
        ok("gitlab.com", "org/team/src", Target::PullRequest(3))
    );
    assert_eq!(
        web("https://gitlab.com/org/commit/-/commit/abcd1234"),
        ok(
            "gitlab.com",
            "org/commit",
            Target::Commit("abcd1234".into())
        )
    );
    // A forge served over plain http is cloned so too.
    let http = parse_web_url("http://git.example.lan/o/r/pulls/1");
    assert_eq!(
        http.map(|l| l.clone_url()).ok().as_deref(),
        Some("http://git.example.lan/o/r.git")
    );
}
