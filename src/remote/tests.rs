use std::path::{Path, PathBuf};

use super::*;

fn ssh(host: &str, path: &str) -> Option<Target> {
    Some(Target {
        host: Host::Ssh(host.to_owned()),
        path: path.to_owned(),
    })
}

#[test]
fn ssh_paths_name_a_host_and_a_path_there() {
    assert_eq!(
        parse(Path::new("ssh://build/home/me/repo")),
        ssh("build", "/home/me/repo")
    );
    assert_eq!(
        parse(Path::new("ssh://me@build:2222/srv/x/")),
        ssh("me@build:2222", "/srv/x")
    );
    assert_eq!(
        parse(Path::new("ssh://build/~/priv/repos/4SaaS")),
        ssh("build", "~/priv/repos/4SaaS")
    );
    assert_eq!(parse(Path::new("SSH://build")), ssh("build", "/"));
}

/// What `Path::join` and a folded double slash make of one is still the same place.
#[test]
fn joined_ssh_paths_keep_their_host() {
    let repo = PathBuf::from("ssh://build/home/me/repo");
    assert_eq!(
        parse(&repo.join("src").join("a.rs")),
        ssh("build", "/home/me/repo/src/a.rs")
    );
    assert_eq!(parse(Path::new(r"ssh:\build\x")), ssh("build", "/x"));
}

#[test]
fn hosts_are_names_users_and_ports_only() {
    for ok in [
        "build",
        "work-build-server-be",
        "me@build.example.org",
        "me@10.0.0.1:2222",
        "[::1]",
        "me@[fe80::1]:22",
        "host_1",
    ] {
        assert!(valid_ssh_host(ok), "{ok}");
    }
    for bad in [
        "",
        "-oProxyCommand=calc",
        "me@-oProxyCommand=calc",
        "x;calc",
        "a&calc",
        "$(id)",
        "`id`",
        "a|b",
        "host:",
        "host:22x",
        "[::1",
        "[::1]x",
        "[]",
        "a b",
        "@host",
    ] {
        assert!(!valid_ssh_host(bad), "{bad}");
    }
}

/// Nothing starting `ssh:` goes to the system's link handler, not even one not parsed as a
/// repository elsewhere.
#[test]
fn ssh_paths_are_never_opened_here() {
    assert!(openable_here(Path::new("ssh://build/home/me/repo")).is_err());
    assert!(openable_here(Path::new("ssh://x;calc/home")).is_err());
    assert!(openable_here(Path::new("SSH:/x")).is_err());
    assert!(openable_here(Path::new("/home/me/repo")).is_ok());
}

#[test]
fn hosts_that_read_as_options_are_refused() {
    assert_eq!(parse(Path::new("ssh://-oProxyCommand=calc/x")), None);
    assert_eq!(parse(Path::new("ssh://a b/x")), None);
    assert_eq!(parse(Path::new("ssh:///x")), None);
}

#[test]
fn local_paths_are_not_remote() {
    for p in [
        "/home/me/repo",
        r"C:\Users\me\repo",
        "relative/dir",
        r"\\server\share\repo",
    ] {
        assert_eq!(parse(Path::new(p)), None, "{p}");
    }
}

#[cfg(windows)]
#[test]
fn wsl_paths_are_their_unc_form() {
    let want = Some(Target {
        host: Host::Wsl("Ubuntu".to_owned()),
        path: "/home/me/repo".to_owned(),
    });
    for p in [
        r"\\wsl.localhost\Ubuntu\home\me\repo",
        r"\\wsl$\Ubuntu\home\me\repo\",
        r"\\?\UNC\wsl.localhost\Ubuntu\home\me\repo",
        "//wsl.localhost/Ubuntu/home/me/repo",
    ] {
        assert_eq!(parse(Path::new(p)), want, "{p}");
    }
    assert_eq!(
        encode(&Host::Wsl("Ubuntu".to_owned()), "/home/me/repo"),
        PathBuf::from(r"\\wsl.localhost\Ubuntu\home\me\repo")
    );
    assert_eq!(parse(Path::new(r"\\wsl.localhost\-x\home")), None);
}

#[test]
fn encoding_and_parsing_go_back_and_forth() {
    let host = Host::Ssh("build".to_owned());
    for path in ["/", "/home/me/repo", "~/repo", "~"] {
        assert_eq!(parse(&encode(&host, path)), ssh("build", path), "{path}");
    }
}

#[test]
fn printed_paths_resolve_on_the_repository_machine() {
    let repo = PathBuf::from("ssh://build/home/me/repo");
    assert_eq!(
        resolve(&repo, "/home/me/wt/.git"),
        PathBuf::from("ssh://build/home/me/wt/.git")
    );
    assert_eq!(
        parse(&resolve(&repo, ".git/rebase-merge")),
        ssh("build", "/home/me/repo/.git/rebase-merge")
    );
    let here = PathBuf::from("/srv/repo");
    assert_eq!(resolve(&here, ".git/x"), here.join(".git/x"));
}

#[test]
fn quoting_keeps_one_word_and_the_home() {
    assert_eq!(sh_quote("it's"), r"'it'\''s'");
    assert_eq!(sh_path("~"), "\"$HOME\"");
    assert_eq!(sh_path("~/a b"), "\"$HOME\"'/a b'");
    assert_eq!(sh_path("/x/~y"), "'/x/~y'");
    assert_eq!(sh_path("~other/x"), "'~other/x'");
}
