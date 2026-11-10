use super::*;

#[test]
fn a_listing_reads_folders_and_repositories() {
    let out = b"/home/me\0D\0Dsrc\0Rapp\0D.cache\0Dodd\nname\0";
    let listing = parse(out);
    assert_eq!(
        listing,
        Some(Listing {
            dir: "/home/me".to_owned(),
            repo: false,
            folders: vec![
                Folder {
                    name: "app".to_owned(),
                    repo: true
                },
                Folder {
                    name: "odd\nname".to_owned(),
                    repo: false
                },
                Folder {
                    name: "src".to_owned(),
                    repo: false
                },
                // Hidden ones last.
                Folder {
                    name: ".cache".to_owned(),
                    repo: false
                },
            ],
            cut: false,
        })
    );
    let cut = parse(b"/\0R\0Dx\0M\0");
    assert_eq!(cut.as_ref().map(|l| (l.repo, l.cut)), Some((true, true)));
    // Not what the script prints.
    assert_eq!(parse(b"oops"), None);
    assert_eq!(parse(b""), None);
}

#[test]
fn paths_and_their_parts() {
    assert_eq!(child("/", "home"), "/home");
    assert_eq!(child("/home/me", "a b"), "/home/me/a b");
    assert_eq!(
        crumbs("/home/me"),
        vec![
            ("/".to_owned(), "/".to_owned()),
            ("home".to_owned(), "/home".to_owned()),
            ("me".to_owned(), "/home/me".to_owned()),
        ]
    );
    assert_eq!(crumbs("/"), vec![("/".to_owned(), "/".to_owned())]);
}

/// The machines `GITCRAFT_TEST_WSL` and `GITCRAFT_TEST_SSH` name (see `remote::live_tests`),
/// with the repository folder named for SSH.
fn test_hosts() -> Vec<(Host, Option<String>)> {
    let mut hosts = Vec::new();
    if let Ok(distro) = std::env::var("GITCRAFT_TEST_WSL") {
        hosts.push((Host::Wsl(distro), None));
    }
    if let Ok(ssh) = std::env::var("GITCRAFT_TEST_SSH")
        && let Some((host, repo)) = ssh.split_once(':')
    {
        hosts.push((Host::Ssh(host.to_owned()), Some(repo.to_owned())));
    }
    hosts
}

/// Against a real machine (skipped without one): odd names, hidden folders, a repository
/// among them, a folder that isn't there.
#[test]
fn folders_there_are_listed() -> Result<(), String> {
    for (host, repo) in test_hosts() {
        let scratch = format!("/tmp/gitcraft-browse-{}", std::process::id());
        let made = remote::exec(
            &host,
            &remote::Request {
                script: format!(
                    concat!(
                        "rm -rf {s} && mkdir -p {s}/'a b' {s}/.hidden {s}/repo/.git ",
                        "{s}/\"$(printf 'new\\nline')\" && : > {s}/file"
                    ),
                    s = scratch
                ),
                ..remote::Request::default()
            },
        )?;
        assert_eq!(made.code, 0, "{}", String::from_utf8_lossy(&made.stderr));
        let listed = list(&host, &scratch);
        let _ = remote::exec(
            &host,
            &remote::Request {
                script: format!("rm -rf {scratch}"),
                ..remote::Request::default()
            },
        );
        let listed = listed?;
        let names: Vec<(&str, bool)> = listed
            .folders
            .iter()
            .map(|f| (f.name.as_str(), f.repo))
            .collect();
        assert_eq!(
            names,
            vec![
                ("a b", false),
                ("new\nline", false),
                ("repo", true),
                (".hidden", false)
            ],
            "{host:?}"
        );
        assert_eq!(listed.dir, scratch);
        assert!(!listed.repo);

        let home = list(&host, "~")?;
        assert!(home.dir.starts_with('/') && home.dir != "/", "{home:?}");
        assert!(list(&host, "/no/such/folder/here").is_err());
        if let Some(repo) = repo {
            let here = list(&host, &repo)?;
            assert!(here.repo, "{here:?}");
            let name = here.dir.rsplit('/').next().unwrap_or_default();
            let parent = list(&host, remote::parent_there(&here.dir))?;
            assert!(
                parent.folders.iter().any(|f| f.name == name && f.repo),
                "{parent:?}"
            );
        }
    }
    Ok(())
}

/// The folder is quoted for the shell, `~` left to be the home folder.
#[test]
fn the_script_names_the_folder_safely() {
    assert!(script("/x/it's $(id)").starts_with(r"cd -- '/x/it'\''s $(id)' || exit 3"));
    assert!(script("~/repos").starts_with("cd -- \"$HOME\"'/repos' || exit 3"));
}
