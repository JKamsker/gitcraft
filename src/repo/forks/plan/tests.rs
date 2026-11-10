use super::*;

#[test]
fn the_config_is_read_into_branches_and_fork_remotes() {
    let out = "branch.feat/x.remote\nann\0branch.feat/x.merge\nrefs/heads/fix\0\
               branch.feat/x.pushremote\nann\0branch.main.remote\norigin\0\
               remote.ann.gitcraftfork\ntrue\0remote.bob.gitcraftfork\ntrue\0";
    let setup = parse_setup(out);
    assert_eq!(setup.branches.len(), 2);
    assert_eq!(setup.branches[0].name, "feat/x");
    assert_eq!(setup.branches[0].push_remote, "ann");
    assert_eq!(setup.fork_remotes, ["ann", "bob"]);
    assert_eq!(setup.unused_forks(), ["bob"]);
    assert_eq!(
        setup.pushing_to("ann", "fix").map(|b| b.name.as_str()),
        Some("feat/x")
    );
}

#[test]
fn remote_names_are_made_of_the_owner() {
    assert_eq!(remote_name("ann"), "ann");
    assert_eq!(remote_name("Zoë Ångström"), "Zo---ngstr-m");
    assert_eq!(remote_name("--"), "fork");
    assert_eq!(remote_name("group/sub"), "group-sub");
}

#[test]
fn urls_of_the_same_repository_are_the_same() {
    let h = Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".into(),
        path: "ann/app".into(),
        web: "https://github.com".into(),
    };
    assert!(h.matches_git_remote("git@github.com:Ann/App.git"));
    assert!(!h.matches_git_remote("https://github.com/ann/other"));
}

#[test]
fn review13_fork_remote_reuse_keeps_web_authority_and_gitlab_case() {
    use crate::forge::Forge;
    for (forge, path) in [
        (Forge::Forgejo, "Team/Repo"),
        (Forge::GitLab, "Group/Project"),
    ] {
        let h = Hosting {
            forge,
            host: "forge.example".into(),
            path: "upstream/repo".into(),
            web: "https://forge.example:4000/reviews".into(),
        };
        let mut fork = info();
        fork.repo_path = path.into();
        fork.clone_https = format!("{}/{path}.git", h.web);
        fork.clone_ssh = format!("git@forge.example:{path}.git");
        let remotes = [
            remote(
                "wrong-port",
                &format!("https://forge.example:3000/reviews/{path}.git"),
            ),
            remote(
                "wrong-prefix",
                &format!("https://forge.example:4000/{path}.git"),
            ),
            remote(
                "wrong-prefix-case",
                &format!("https://forge.example:4000/REVIEWS/{path}.git"),
            ),
            remote("selected", &fork.clone_https),
        ];
        assert_eq!(
            fork_remote(&fork, &remotes, Some(&h)).as_deref(),
            Some("selected")
        );
        let remotes = [
            remote(
                "wrong-case",
                "https://forge.example:4000/reviews/group/project.git",
            ),
            remote(
                "selected",
                &format!("ssh://git@forge.example:2222/{path}.git"),
            ),
        ];
        assert_eq!(
            fork_remote(&fork, &remotes, Some(&h)).as_deref(),
            Some("selected")
        );
    }
}

fn selected_hosting() -> Hosting {
    Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "example.org".into(),
        path: "acme/app".into(),
        web: "https://example.org".into(),
    }
}

fn info() -> ForkInfo {
    ForkInfo {
        number: 7,
        cross: true,
        owner: "ann".into(),
        repo_path: "ann/app".into(),
        clone_https: "https://example.org/ann/app.git".into(),
        clone_ssh: "git@example.org:ann/app.git".into(),
        branch: "fix".into(),
        base: "main".into(),
        maintainers_can_edit: true,
        ..ForkInfo::default()
    }
}

fn remote(name: &str, url: &str) -> Remote {
    Remote {
        name: name.into(),
        url: url.into(),
    }
}

#[test]
fn a_checkout_is_refused_where_the_author_doesnt_allow_it() {
    let mut no = info();
    no.maintainers_can_edit = false;
    assert!(checkout(&no, &[], &Setup::default(), &[], "main", None).is_err());
    let mut own = info();
    own.cross = false;
    assert!(checkout(&own, &[], &Setup::default(), &[], "main", None).is_err());
    let mut odd = info();
    odd.branch = "-x".into();
    assert!(checkout(&odd, &[], &Setup::default(), &[], "main", None).is_err());
}

#[test]
fn the_fork_is_reached_like_origin_and_names_taken_are_avoided() {
    let remotes = [
        remote("origin", "git@example.org:acme/app.git"),
        remote("ann", "https://example.org/someone-else/app.git"),
    ];
    let plan = checkout(
        &info(),
        &remotes,
        &Setup::default(),
        &["fix".into()],
        "main",
        Some(&selected_hosting()),
    );
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    assert_eq!(plan.url, "git@example.org:ann/app.git");
    assert_eq!(plan.remote, "ann-fork");
    assert_eq!(plan.local, "ann/fix");
    assert!(
        !plan
            .steps
            .iter()
            .any(|s| s.iter().any(|a| a.ends_with(".push")))
    );
    // The fork already a remote: used as it is.
    let remotes = [
        remote("origin", "https://example.org/acme/app.git"),
        remote("annie", "https://example.org/ann/app"),
    ];
    let plan = checkout(
        &info(),
        &remotes,
        &Setup::default(),
        &[],
        "main",
        Some(&selected_hosting()),
    );
    assert!(plan.is_ok_and(|p| p.remote == "annie" && !p.new_remote && p.local == "fix"));
}
