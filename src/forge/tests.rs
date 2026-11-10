use super::*;

/// One refused reviewer doesn't keep the others from being asked, and the URL is said.
#[test]
fn reviewers_are_asked_one_at_a_time() {
    let asked = std::cell::RefCell::new(Vec::new());
    let reviewers = ["me".to_owned(), "ann".to_owned(), "gone".to_owned()];
    let result = ask_reviewers("https://x/pr/1".to_owned(), &reviewers, |who| {
        asked.borrow_mut().push(who.to_owned());
        match who {
            "me" => Err("Review cannot be requested from pull request author".to_owned()),
            "gone" => Err("not found".to_owned()),
            _ => Ok(()),
        }
    });
    assert_eq!(*asked.borrow(), ["me", "ann", "gone"]);
    assert!(
        result.is_err_and(|e| e.starts_with("Opened https://x/pr/1") && e.contains("me, gone"))
    );
    assert_eq!(
        ask_reviewers("u".to_owned(), &[], |_| Err(String::new())),
        Ok("u".to_owned())
    );
}

#[test]
fn remote_locations() {
    let gh = Some(("github.com".to_owned(), "owner/repo".to_owned()));
    assert_eq!(location("https://github.com/owner/repo.git"), gh);
    assert_eq!(location("https://user@github.com/owner/repo/"), gh);
    assert_eq!(location("git@github.com:owner/repo.git"), gh);
    assert_eq!(location("ssh://git@github.com:22/owner/repo"), gh);
    assert_eq!(
        location("https://gitlab.example.org/group/sub/proj.git"),
        Some(("gitlab.example.org".to_owned(), "group/sub/proj".to_owned()))
    );
    assert_eq!(location("/srv/git/repo.git"), None);
    // GitHub's and GitLab's SSH over port 443.
    assert_eq!(location("ssh://git@ssh.github.com:443/owner/repo.git"), gh);
    assert_eq!(
        location("ssh://git@altssh.gitlab.com:443/g/p.git"),
        Some(("gitlab.com".to_owned(), "g/p".to_owned()))
    );
}

#[test]
fn web_pages_of_remotes() {
    let page = Some("https://github.com/JKamsker/gitcraft".to_owned());
    assert_eq!(web_url("https://github.com/JKamsker/gitcraft.git"), page);
    assert_eq!(web_url("git@github.com:JKamsker/gitcraft.git"), page);
    assert_eq!(web_url("ssh://git@github.com:22/JKamsker/gitcraft"), page);
    assert_eq!(
        web_url("http://git.example.org/team/repo.git"),
        Some("http://git.example.org/team/repo".to_owned())
    );
    assert_eq!(
        web_url("https://git.example.org:8443/team/repo.git"),
        Some("https://git.example.org:8443/team/repo".to_owned())
    );
    assert_eq!(
        web_url("ssh://git@git.example.org:2222/team/repo.git"),
        Some("https://git.example.org/team/repo".to_owned())
    );
    assert_eq!(
        web_url("https://me@git.example.org:3000/o/r.git"),
        Some("https://git.example.org:3000/o/r".to_owned())
    );
    assert_eq!(web_url("C:/repos/gitcraft"), None);
    assert_eq!(web_url("/srv/git/repo.git"), None);
}

#[test]
fn avatars_by_email() {
    let json = r#"[
            {"commit": {"author": {"email": "Ann@x.org"}, "committer": {"email": "noreply@github.com"}},
             "author": {"avatar_url": "https://a/1?v=4"}, "committer": {"avatar_url": "https://a/web?v=4"}},
            {"commit": {"author": {"email": "nobody@x.org"}, "committer": {"email": "nobody@x.org"}},
             "author": null, "committer": null}
        ]"#;
    let m = parse_commit_avatars(json);
    assert_eq!(
        m.get("ann@x.org").map(String::as_str),
        Some("https://a/1?v=4")
    );
    assert!(m.contains_key("noreply@github.com"));
    assert!(!m.contains_key("nobody@x.org"));
}

#[test]
fn detects_forge_from_the_host() {
    let none = HashMap::new();
    assert_eq!(choose("github.com", &none), Some(Forge::GitHub));
    assert_eq!(choose("github.example.com", &none), Some(Forge::GitHub));
    assert_eq!(choose("gitlab.com", &none), Some(Forge::GitLab));
    assert_eq!(choose("codeberg.org", &none), Some(Forge::Forgejo));
    assert_eq!(choose("forgejo.example.org", &none), Some(Forge::Forgejo));
    assert_eq!(choose("gitea.com", &none), Some(Forge::Gitea));
    assert_eq!(choose("git.example.org", &none), None);
    assert_eq!(choose("c", &none), None);
}

#[test]
fn detects_forge_from_origin() {
    let r = |name: &str, url: &str| Remote {
        name: name.into(),
        url: url.into(),
    };
    // Neither the machine's git config nor its `tea` logins: what the remotes say alone.
    let forge = |remotes: &[Remote]| detect_with(remotes, &HashMap::new(), &[]).map(|h| h.forge);
    assert_eq!(
        forge(&[r("origin", "git@github.com:a/b.git")]),
        Some(Forge::GitHub)
    );
    assert_eq!(
        forge(&[
            r("up", "https://github.com/x/y"),
            r("origin", "https://gitlab.com/a/b")
        ]),
        Some(Forge::GitLab)
    );
    assert_eq!(
        forge(&[r("origin", "ssh://git@codeberg.org/a/b.git")]),
        Some(Forge::Forgejo)
    );
    assert_eq!(forge(&[r("origin", "C:/repos/bare.git")]), None);
    // A folder's name is no forge.
    assert_eq!(forge(&[r("origin", "/srv/gitlab-backup/x.git")]), None);
    assert_eq!(forge(&[r("origin", "C:/gitlab/x.git")]), None);
    assert_eq!(
        forge(&[r("origin", "https://github.example.com/a/b")]),
        Some(Forge::GitHub)
    );
}

#[test]
fn unusable_origin_falls_back_to_first_usable_forge_remote() {
    let remotes = [
        Remote {
            name: "disk".into(),
            url: "/srv/repo".into(),
        },
        Remote {
            name: "origin".into(),
            url: "https://unknown.example/owner/repo".into(),
        },
        Remote {
            name: "fork".into(),
            url: "https://github.com/JKamsker/gitcraft".into(),
        },
    ];
    let hosting = detect_with(&remotes, &HashMap::new(), &[]);
    assert_eq!(hosting.map(|h| h.path), Some("JKamsker/gitcraft".into()));
}

#[test]
fn only_the_host_tells_the_forge() {
    let r = |url: &str| Remote {
        name: "origin".into(),
        url: url.into(),
    };
    let forge = |remotes: &[Remote]| detect_with(remotes, &HashMap::new(), &[]).map(|h| h.forge);
    assert_eq!(
        forge(&[r("git@gitlab.com:me/me.github.io.git")]),
        Some(Forge::GitLab)
    );
    assert_eq!(forge(&[r("https://git.example.org/github.com/x")]), None);
    assert_eq!(forge(&[r("https://git.example.org/a@github.com/x")]), None);
    assert_eq!(
        forge(&[r("https://github-mirror.gitlab.example.org/g/p.git")]),
        Some(Forge::GitLab)
    );
    assert_eq!(
        location("https://me:tok@github.com/a/b.git"),
        Some(("github.com".into(), "a/b".into()))
    );
    assert_eq!(
        forge(&[r("https://example.org/gitlab/x/y")]),
        Some(Forge::GitLab)
    );
}

#[test]
fn git_config_names_the_forge_of_a_host_its_name_doesnt_tell() {
    let config = parse_config(
        "gitcraft.git.example.org.forge forgejo\ngitcraft.other.example.org.forge Gitea\n",
    );
    assert_eq!(choose("git.example.org", &config), Some(Forge::Forgejo));
    assert_eq!(choose("other.example.org", &config), Some(Forge::Gitea));
    assert_eq!(choose("third.example.org", &config), None);
}

#[test]
fn a_repositorys_own_setting_wins() {
    let config = parse_config("gitcraft.forge gitea\ngitcraft.github.com.forge github\n");
    assert_eq!(choose("github.com", &config), Some(Forge::Gitea));
    let bitbucket = parse_config("gitcraft.forge bitbucket\n");
    assert_eq!(choose("github.com", &bitbucket), Some(Forge::Bitbucket));
    let nonsense = parse_config("gitcraft.forge unknown-forge\n");
    assert_eq!(choose("github.com", &nonsense), Some(Forge::GitHub));
}

#[test]
fn a_remote_whose_host_doesnt_tell_goes_by_its_whole_url() {
    let r = |url: &str| Remote {
        name: "origin".into(),
        url: url.into(),
    };
    let forge = |url: &str| detect_with(&[r(url)], &HashMap::new(), &[]).map(|h| h.forge);
    assert_eq!(
        forge("gitlab@code.acme.com:group/proj.git"),
        Some(Forge::GitLab)
    );
    assert_eq!(
        forge("https://acme.com/gitlab/group/proj.git"),
        Some(Forge::GitLab)
    );
    // A local path has no host: whatever its folders are called, it's on no forge.
    assert_eq!(forge("/home/me/src/github.com/o/r"), None);
    // A host that says what it is wins over the rest of the URL.
    assert_eq!(
        forge("https://gitlab.com/ana/ana.github.io"),
        Some(Forge::GitLab)
    );
}

#[test]
fn an_ssh_remote_gets_its_web_address_from_teas_login() {
    let r = Remote {
        name: "origin".into(),
        url: "git@gitea.lan:o/r.git".into(),
    };
    let tea = [("gitea.lan".to_owned(), "http://gitea.lan:3000".to_owned())];
    let hosting = detect_with(&[r], &HashMap::new(), &tea);
    assert_eq!(
        hosting.map(|h| (h.forge, h.web)),
        Some((Forge::Gitea, "http://gitea.lan:3000".to_owned()))
    );
}

#[test]
fn gerrit_override_excludes_only_its_remote_from_primary_forge_candidates() {
    let remotes = [
        Remote {
            name: "origin".into(),
            url: "https://github.com/team/review.git".into(),
        },
        Remote {
            name: "fork".into(),
            url: "https://gitlab.com/team/review.git".into(),
        },
    ];
    let config = parse_config("gitcraft.github.com.forge Gerrit\n");
    assert!(detect_remote(&remotes[0], &config, &[]).is_none());
    assert_eq!(
        detect_with(&remotes, &config, &[]).map(|h| h.forge),
        Some(Forge::GitLab)
    );
    let all_gerrit = parse_config("gitcraft.forge gerrit\n");
    assert!(detect_with(&remotes, &all_gerrit, &[]).is_none());
}

#[test]
fn the_web_server_keeps_its_port_and_an_ssh_one_is_dropped() {
    assert_eq!(
        web_base("https://me:tok@Git.Example.org:3000/o/r.git").as_deref(),
        Some("https://git.example.org:3000")
    );
    assert_eq!(
        web_base("http://git.local/o/r.git").as_deref(),
        Some("http://git.local")
    );
    assert_eq!(web_base("ssh://git@git.example.org:2222/o/r.git"), None);
    assert_eq!(web_base("git@codeberg.org:o/r.git"), None);
    let r = Remote {
        name: "origin".into(),
        url: "ssh://git@git.example.org:2222/o/r.git".into(),
    };
    let config = parse_config("gitcraft.forge forgejo\n");
    let hosting = detect_with(&[r], &config, &[]);
    assert_eq!(
        hosting.map(|h| h.web).as_deref(),
        Some("https://git.example.org")
    );
}

#[test]
fn an_ipv6_host_keeps_its_brackets() {
    assert_eq!(
        location("https://[fd00::5]:3000/o/r.git"),
        Some(("[fd00::5]".to_owned(), "o/r".to_owned()))
    );
    assert_eq!(
        web_base("https://[fd00::5]:3000/o/r.git").as_deref(),
        Some("https://[fd00::5]:3000")
    );
}
