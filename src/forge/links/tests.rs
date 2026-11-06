use std::collections::HashMap;

use super::*;
use crate::forge::detect_with;
use crate::git::Remote;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn remote(name: &str, url: &str) -> Remote {
    Remote {
        name: name.into(),
        url: url.into(),
    }
}

/// Where `url` is hosted, as detection finds it with `forge` set in git's config when given.
fn hosting(url: &str, forge: Option<&str>) -> Result<Hosting, String> {
    let config: HashMap<String, String> = forge
        .map(|f| HashMap::from([("gitcraft.forge".to_owned(), f.to_owned())]))
        .unwrap_or_default();
    detect_with(&[remote("origin", url)], &config, &[]).ok_or_else(|| format!("no forge for {url}"))
}

fn file(path: &str, lines: Option<Lines>) -> Place<'_> {
    Place::File {
        sha: SHA,
        path,
        lines,
    }
}

#[test]
fn github_links_from_ssh_and_https_remotes() -> Result<(), String> {
    for remote in [
        "git@github.com:o/r.git",
        "https://github.com/o/r.git",
        "https://github.com/o/r",
        "ssh://git@github.com:22/o/r.git",
        "ssh://git@ssh.github.com:443/o/r.git",
        "https://me:token@github.com/o/r.git/",
    ] {
        let h = hosting(remote, None)?;
        assert_eq!(
            url(&h, &Place::Commit(SHA)),
            format!("https://github.com/o/r/commit/{SHA}"),
            "{remote}"
        );
    }
    let h = hosting("git@github.com:o/r.git", None)?;
    assert_eq!(
        url(&h, &Place::Branch("feature/x")),
        "https://github.com/o/r/tree/feature/x"
    );
    assert_eq!(
        url(&h, &file("src/main.rs", None)),
        format!("https://github.com/o/r/blob/{SHA}/src/main.rs")
    );
    assert_eq!(
        url(&h, &file("src/main.rs", Lines::new(7, 7))),
        format!("https://github.com/o/r/blob/{SHA}/src/main.rs#L7")
    );
    assert_eq!(
        url(&h, &file("src/main.rs", Lines::new(12, 3))),
        format!("https://github.com/o/r/blob/{SHA}/src/main.rs#L3-L12")
    );
    Ok(())
}

#[test]
fn gitlab_links_have_their_dash_and_ranges_without_a_second_l() -> Result<(), String> {
    let h = hosting("git@gitlab.com:group/sub/proj.git", None)?;
    assert_eq!(h.forge, Forge::GitLab);
    assert_eq!(
        url(&h, &Place::Commit(SHA)),
        format!("https://gitlab.com/group/sub/proj/-/commit/{SHA}")
    );
    assert_eq!(
        url(&h, &Place::Branch("main")),
        "https://gitlab.com/group/sub/proj/-/tree/main"
    );
    assert_eq!(
        url(&h, &file("a.txt", Lines::new(4, 9))),
        format!("https://gitlab.com/group/sub/proj/-/blob/{SHA}/a.txt#L4-9")
    );
    assert_eq!(
        url(&h, &file("a.txt", Lines::new(4, 4))),
        format!("https://gitlab.com/group/sub/proj/-/blob/{SHA}/a.txt#L4")
    );
    Ok(())
}

#[test]
fn forgejo_and_gitea_links() -> Result<(), String> {
    for (remote, forge) in [
        ("git@codeberg.org:o/r.git", Forge::Forgejo),
        ("https://gitea.com/o/r.git", Forge::Gitea),
    ] {
        let h = hosting(remote, None)?;
        assert_eq!(h.forge, forge);
        let web = h.web.clone();
        assert_eq!(
            url(&h, &Place::Commit(SHA)),
            format!("{web}/o/r/commit/{SHA}")
        );
        assert_eq!(
            url(&h, &Place::Branch("rel/1.0")),
            format!("{web}/o/r/src/branch/rel/1.0")
        );
        assert_eq!(
            url(&h, &file("docs/a.md", Lines::new(1, 2))),
            format!("{web}/o/r/src/commit/{SHA}/docs/a.md#L1-L2")
        );
    }
    Ok(())
}

#[test]
fn self_hosted_servers_keep_their_port_and_sub_path() -> Result<(), String> {
    // A web remote names the server's port; an SSH one's port is ssh's.
    let h = hosting("https://git.example.org:8443/team/app.git", Some("gitea"))?;
    assert_eq!(
        url(&h, &Place::Commit(SHA)),
        format!("https://git.example.org:8443/team/app/commit/{SHA}")
    );
    let h = hosting(
        "ssh://git@git.example.org:2222/team/app.git",
        Some("forgejo"),
    )?;
    assert_eq!(
        url(&h, &Place::Commit(SHA)),
        format!("https://git.example.org/team/app/commit/{SHA}")
    );
    // GitLab installed under a folder of the server: the folder is part of the remote's path.
    let h = hosting("https://example.org/gitlab/group/app.git", None)?;
    assert_eq!(h.forge, Forge::GitLab);
    assert_eq!(
        url(&h, &Place::Commit(SHA)),
        format!("https://example.org/gitlab/group/app/-/commit/{SHA}")
    );
    // A Gitea under a folder, reached over SSH: `tea`'s login says where its pages are.
    let tea = [(
        "gitea.lan".to_owned(),
        "http://gitea.lan:3000/git/".to_owned(),
    )];
    let h = detect_with(
        &[remote("origin", "git@gitea.lan:o/r.git")],
        &HashMap::new(),
        &tea,
    )
    .ok_or("no forge for gitea.lan")?;
    assert_eq!(
        url(&h, &Place::Branch("main")),
        "http://gitea.lan:3000/git/o/r/src/branch/main"
    );
    // An IPv6 server.
    let h = hosting("http://[fd00::5]:3000/o/r.git", Some("gitea"))?;
    assert_eq!(
        url(&h, &Place::Commit(SHA)),
        format!("http://[fd00::5]:3000/o/r/commit/{SHA}")
    );
    Ok(())
}

#[test]
fn paths_and_branches_are_percent_encoded() -> Result<(), String> {
    let h = hosting("git@github.com:o/r.git", None)?;
    assert_eq!(
        url(&h, &file("docs/My Notes #1?.md", None)),
        format!("https://github.com/o/r/blob/{SHA}/docs/My%20Notes%20%231%3F.md")
    );
    assert_eq!(
        url(&h, &file("ünï/日本.txt", Lines::new(2, 3))),
        format!("https://github.com/o/r/blob/{SHA}/%C3%BCn%C3%AF/%E6%97%A5%E6%9C%AC.txt#L2-L3")
    );
    assert_eq!(
        url(&h, &file("100%/a+b(c)@d.rs", None)),
        format!("https://github.com/o/r/blob/{SHA}/100%25/a+b(c)@d.rs")
    );
    assert_eq!(
        url(&h, &Place::Branch("fix/#12 ä")),
        "https://github.com/o/r/tree/fix/%2312%20%C3%A4"
    );
    Ok(())
}

#[test]
fn a_long_path_and_huge_line_numbers_stay_whole() -> Result<(), String> {
    let h = hosting("git@github.com:o/r.git", None)?;
    let deep = vec!["d"; 500].join("/");
    let link = url(&h, &file(&deep, Lines::new(1, u32::MAX)));
    assert!(link.ends_with(&format!("/{deep}#L1-L{}", u32::MAX)));
    Ok(())
}

#[test]
fn lines_are_ordered_and_named() {
    assert_eq!(Lines::new(9, 2), Some(Lines { first: 2, last: 9 }));
    assert_eq!(Lines::new(0, 2), None);
    assert_eq!(
        Lines::new(5, 5).map(Lines::label).as_deref(),
        Some("line 5")
    );
    assert_eq!(
        Lines::new(5, 8).map(Lines::label).as_deref(),
        Some("lines 5–8")
    );
}

#[test]
fn the_tracked_remote_wins_then_origin_then_a_choice() {
    let remotes = [
        remote("upstream", "https://github.com/up/r.git"),
        remote("origin", "git@github.com:me/r.git"),
        remote("fork", "git@gitlab.com:me/r.git"),
        remote("disk", "/srv/git/r.git"),
    ];
    assert_eq!(candidates(&remotes, Some("fork")), ["fork"]);
    assert_eq!(candidates(&remotes, None), ["origin"]);
    // A tracked remote on disk has no pages: origin's are next best.
    assert_eq!(candidates(&remotes, Some("disk")), ["origin"]);
    let no_origin = [
        remote("upstream", "https://github.com/up/r.git"),
        remote("fork", "git@gitlab.com:me/r.git"),
        remote("disk", "C:/repos/r.git"),
    ];
    assert_eq!(candidates(&no_origin, None), ["upstream", "fork"]);
    assert_eq!(candidates(&no_origin, Some("fork")), ["fork"]);
    assert_eq!(candidates(&no_origin[2..], None), Vec::<String>::new());
    assert_eq!(candidates(&[], None), Vec::<String>::new());
    // An origin on disk isn't taken over a remote on a forge.
    let disk_origin = [
        remote("origin", "/srv/r.git"),
        remote("gh", "git@github.com:o/r.git"),
    ];
    assert_eq!(candidates(&disk_origin, None), ["gh"]);
}

#[test]
fn an_upstream_belongs_to_the_longest_remote_that_fits() {
    let remotes = [
        remote("my", "git@github.com:a/r.git"),
        remote("my/fork", "git@github.com:b/r.git"),
        remote("origin", "git@github.com:c/r.git"),
    ];
    assert_eq!(
        upstream_remote("my/fork/topic/x", &remotes),
        Some(("my/fork".to_owned(), "topic/x".to_owned()))
    );
    assert_eq!(
        upstream_remote("my/main", &remotes),
        Some(("my".to_owned(), "main".to_owned()))
    );
    assert_eq!(upstream_remote("origin/", &remotes), None);
    assert_eq!(upstream_remote("elsewhere/main", &remotes), None);
    assert_eq!(upstream_remote("", &remotes), None);
}

#[test]
fn the_menu_names_the_forge() {
    let gh = remote("origin", "git@github.com:o/r.git");
    assert_eq!(forge_name(&gh, None), "GitHub");
    let lab = remote("origin", "https://example.org/x/r.git");
    let found = Hosting {
        forge: Forge::Forgejo,
        host: "example.org".into(),
        path: "x/r".into(),
        web: "https://example.org".into(),
    };
    assert_eq!(forge_name(&lab, Some(&found)), "Forgejo");
    assert_eq!(forge_name(&lab, None), "the forge");
}
