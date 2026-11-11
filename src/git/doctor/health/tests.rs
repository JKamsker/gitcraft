use super::*;

#[test]
fn count_objects_is_read() {
    let o = parse_count_objects(
        "count: 5\nsize: 20\nin-pack: 1200\npacks: 2\nsize-pack: 830\nprune-packable: 1\n\
         garbage: 0\nsize-garbage: 0\nfuture-key: 9\nbroken line\n",
    );
    assert_eq!(
        (o.loose, o.loose_kib, o.packed, o.packs, o.pack_kib),
        (5, 20, 1200, 2, 830)
    );
    assert_eq!(o.prune_packable, 1);
    assert_eq!(parse_count_objects(""), Objects::default());
}

#[test]
fn fsmonitor_states() {
    assert_eq!(fsmonitor_state("true", true), State::Unsupported);
    assert_eq!(fsmonitor_state("", true), State::Unsupported);
    assert_eq!(fsmonitor_state("true", false), State::On);
    assert_eq!(fsmonitor_state("false", false), State::Off);
    // A hook (Watchman) works anywhere.
    assert_eq!(
        fsmonitor_state(".git/hooks/fsmonitor-watchman", true),
        State::On
    );
}

#[test]
fn shallow_and_partial_clones() {
    let shallow = suggestions(&Health {
        shallow: true,
        ..Default::default()
    });
    assert_eq!(shallow[0].state, State::Unsupported);
    let partial = suggestions(&Health {
        partial: true,
        ..Default::default()
    });
    assert!(!partial[0].ops[0].iter().any(|a| a == "--changed-paths"));
    assert!(
        suggestions(&Health::default())[0].ops[0]
            .iter()
            .any(|a| a == "--changed-paths")
    );
}

#[test]
fn maintenance_registration_is_matched() {
    assert!(registered("/home/me/a\n/home/me/repo\n", "/home/me/repo/"));
    assert!(!registered("/home/me/repo2\n", "/home/me/repo"));
    assert!(!registered("", "/x"));
    if cfg!(windows) {
        assert!(registered("C:/Code/Repo\n", "c:\\code\\repo"));
    }
}

#[test]
fn suggestions_follow_the_health() {
    let h = Health {
        objects: Objects {
            loose: 5000,
            ..Default::default()
        },
        status_took: Some(Duration::from_millis(900)),
        fsmonitor: Some(State::Unsupported),
        ..Default::default()
    };
    let s = suggestions(&h);
    assert_eq!(s.len(), 5);
    assert!(s.iter().any(|x| x.title == "Pack the objects"));
    let fs = s.iter().find(|x| x.title == "File system monitor");
    assert!(fs.is_some_and(|x| x.state == State::Unsupported && x.why.contains("900 ms")));
    let quiet = suggestions(&Health {
        commit_graph: true,
        ..Default::default()
    });
    assert_eq!(quiet.len(), 4);
    assert_eq!(quiet[0].state, State::On);
}
