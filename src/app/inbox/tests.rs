use super::*;

fn hosting(forge: Forge, host: &str, path: &str) -> Hosting {
    Hosting {
        forge,
        host: host.into(),
        path: path.into(),
        web: format!("https://{host}"),
    }
}

fn item(url: &str, updated: i64, mine: bool, requested: bool) -> Item {
    Item {
        repo: Some(PathBuf::from("/r")),
        forge: Forge::GitHub,
        pr: InboxPr {
            url: url.into(),
            updated,
            mine,
            review_requested: requested,
            said: vec![100],
            ..InboxPr::default()
        },
    }
}

#[test]
fn each_forge_repository_is_asked_once_and_a_searchable_host_once() {
    let found = vec![
        (
            PathBuf::from("/a"),
            hosting(Forge::GitHub, "github.com", "o/r"),
        ),
        (
            PathBuf::from("/a-clone"),
            hosting(Forge::GitHub, "github.com", "o/r"),
        ),
        (
            PathBuf::from("/b"),
            hosting(Forge::GitHub, "github.com", "o/other"),
        ),
        (
            PathBuf::from("/fork"),
            hosting(Forge::Forgejo, "codeberg.org", "me/app"),
        ),
        (
            PathBuf::from("/up"),
            hosting(Forge::Forgejo, "codeberg.org", "up/app"),
        ),
    ];
    let todo = asks(found);
    let repos: Vec<_> = todo.iter().map(|a| a.repo.clone()).collect();
    assert_eq!(
        repos,
        [
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/fork")
        ]
    );
    assert_eq!(todo[2].where_is(), "Forgejo · codeberg.org");
    assert_eq!(todo[2].covers(), "https://codeberg.org/");
    assert_eq!(todo[0].covers(), "https://github.com/o/r/");
    // A pull request found on the host opens in the clone of its repository, if any.
    let upstream = InboxPr {
        project: "Up/App".into(),
        ..InboxPr::default()
    };
    assert_eq!(todo[2].clone_for(&upstream), Some(PathBuf::from("/up")));
    let elsewhere = InboxPr {
        project: "x/y".into(),
        ..InboxPr::default()
    };
    assert_eq!(todo[2].clone_for(&elsewhere), None);
    assert_eq!(todo[0].clone_for(&elsewhere), Some(PathBuf::from("/a")));
}

#[test]
fn the_list_is_each_once_newest_first_and_only_what_needs_you() {
    let items = dedupe(vec![
        item("u1", 10, false, true),
        item("u2", 30, true, false),
        item("u1", 10, false, true),
        item("u3", 20, false, false),
    ]);
    let urls: Vec<&str> = items.iter().map(|i| i.pr.url.as_str()).collect();
    assert_eq!(urls, ["u2", "u3", "u1"]);
    let mut seen = BTreeMap::new();
    let list = waiting(&items, &seen);
    let urls: Vec<&str> = list.iter().map(|(i, _)| i.pr.url.as_str()).collect();
    assert_eq!(urls, ["u2", "u1"], "u3 is neither yours nor asks you");
    // Opened from the inbox: its comment is seen.
    seen.insert("u2".to_owned(), Seen { at: 100 });
    let list = waiting(&items, &seen);
    assert_eq!(list.len(), 1);
}

/// The first time one of yours is met, what it already had isn't news; what is no longer
/// listed is let go where its forge answered, kept where it didn't.
#[test]
fn a_round_remembers_yours_and_lets_go_of_the_gone() {
    let url = |n: u32| format!("https://github.com/o/r/pull/{n}");
    let mut seen = BTreeMap::new();
    seen.insert(url(9), Seen { at: 5 });
    seen.insert(
        "https://codeberg.org/x/y/pulls/1".to_owned(),
        Seen { at: 5 },
    );
    let mut first = item(&url(1), 50, true, false);
    first.pr.said = vec![40, 45];
    let asked = item(&url(2), 50, false, true);
    let items = vec![first, asked];
    remember_round(&mut seen, &items, &["https://github.com/o/r/".to_owned()]);
    assert_eq!(seen.get(&url(1)), Some(&Seen { at: 50 }));
    assert!(
        waiting(&items, &seen)
            .iter()
            .all(|(i, _)| i.pr.url == url(2))
    );
    assert!(!seen.contains_key(&url(2)), "only yours");
    assert!(!seen.contains_key(&url(9)), "merged or closed since");
    assert!(seen.contains_key("https://codeberg.org/x/y/pulls/1"));
    // A comment after that is new.
    let mut items = items;
    items[0].pr.said.push(60);
    let list = waiting(&items, &seen);
    assert_eq!(list[0].1, vec![Reason::NewComments(1)]);
}

#[test]
fn problems_are_said_once_each() {
    let problem = |at: &str, why: &str| Problem {
        forge: Forge::GitHub,
        host: "github.com".into(),
        at: at.into(),
        why: why.into(),
    };
    let a = problem("GitHub · o/r", "gh isn't installed");
    let b = problem("GitHub · o/s", "gh isn't installed");
    let c = problem("GitHub · o/s", "HTTP 502");
    assert_eq!(
        grouped(&[&a, &b, &c, &a]),
        [
            "GitHub · o/r, GitHub · o/s: gh isn't installed",
            "GitHub · o/s: HTTP 502"
        ]
    );
}

/// A round over repositories on no forge (or not repositories) ends, with nothing to ask.
#[test]
fn a_round_over_repositories_on_no_forge_ends_empty() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-inbox-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["init", "-q"])?;
    git::run(Some(&dir), &["remote", "add", "origin", "/srv/plain.git"])?;
    let ctx = egui::Context::default();
    let mut inbox = Inbox::default();
    inbox.ask(vec![dir.clone(), dir.join("missing")], false, &ctx);
    assert!(inbox.busy());
    // A second round isn't started over a running one.
    inbox.ask(vec![dir.clone()], false, &ctx);
    let until = Instant::now() + Duration::from_secs(30);
    let mut ended = false;
    while inbox.busy() {
        if Instant::now() > until {
            return Err("the round never ended".to_owned());
        }
        ended |= inbox.poll(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(inbox.items.is_empty() && inbox.problems.is_empty());
    // Nothing asked of a forge: it ended in step one.
    assert!(!ended);
    Ok(())
}

/// A forge given up on (no tool, no login) isn't asked again until ↻, and still counts as
/// not asked.
#[test]
fn a_forge_given_up_on_is_left_until_asked_again() {
    let ctx = egui::Context::default();
    let mut inbox = Inbox::default();
    let gone = Problem {
        forge: Forge::GitHub,
        host: "github.com".into(),
        at: "GitHub · o/r".into(),
        why: "gh isn't installed: gitcraft uses it for pull requests".into(),
    };
    inbox
        .given_up
        .insert((Forge::GitHub, "github.com".into()), gone.clone());
    inbox.found = vec![(
        PathBuf::from("/a"),
        hosting(Forge::GitHub, "github.com", "o/r"),
    )];
    inbox.start_asking(&ctx);
    assert!(!inbox.busy(), "not asked");
    assert_eq!(inbox.problems, [gone]);
    inbox.ask(Vec::new(), true, &ctx);
    assert!(inbox.given_up.is_empty());
}
