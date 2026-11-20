use super::*;
use crate::ci::Check;

fn check(name: &str, state: CheckState) -> Check {
    Check {
        name: name.to_owned(),
        state,
        detail: String::new(),
        url: format!("https://ci/{name}"),
    }
}

#[test]
fn the_tooltip_tallies_and_names_what_failed_first() {
    let status = Status {
        state: CheckState::Failed,
        checks: vec![
            check("test", CheckState::Running),
            check("lint", CheckState::Failed),
            check("docs", CheckState::Passed),
        ],
    };
    assert_eq!(
        tooltip(&status),
        "CI: 1 failed, 1 running, 1 passed\nfailed  lint\nrunning  test\nClick for details"
    );
}

fn project(path: &str) -> Project {
    Project {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".to_owned(),
        path: path.to_owned(),
    }
}

fn passed() -> Status {
    Status {
        state: CheckState::Passed,
        checks: Vec::new(),
    }
}

fn only(path: &str) -> HashSet<Project> {
    HashSet::from([project(path)])
}

fn answer(path: &str, sha: &str) -> Answers {
    vec![(
        project(path),
        Ok(HashMap::from([(sha.to_owned(), passed())])),
    )]
}

#[test]
fn statuses_asked_of_the_project_before_are_dropped() {
    let mut ci = CiStatuses {
        projects: only("me/fork"),
        loading: true,
        ..CiStatuses::default()
    };
    // The remote is pointed elsewhere while the fork's statuses are being read.
    ci.moved_to(only("org/upstream"));
    assert!(ci.loading, "no second read while the first is on its way");
    ci.arrived(false, answer("me/fork", "abc"));
    assert!(
        ci.by_sha.is_empty(),
        "the fork's statuses aren't upstream's"
    );
    assert!(!ci.loading);
    assert!(ci.next.is_some(), "read again from upstream");
    // Upstream's own answer is taken.
    ci.arrived(false, answer("org/upstream", "abc"));
    assert_eq!(ci.by_sha.len(), 1);
    assert_eq!(ci.source["abc"].path, "org/upstream");
}

#[test]
fn a_project_change_with_nothing_on_its_way_drops_what_the_old_one_said() {
    let mut ci = CiStatuses {
        projects: only("me/fork"),
        by_sha: HashMap::from([("abc".to_owned(), passed())]),
        source: HashMap::from([("abc".to_owned(), project("me/fork"))]),
        cached: vec![(
            project("me/fork"),
            HashMap::from([("abc".to_owned(), passed())]),
        )],
        ..CiStatuses::default()
    };
    ci.moved_to(only("org/upstream"));
    assert!(ci.by_sha.is_empty());
    assert!(!ci.loading);
    ci.arrived(false, answer("org/upstream", "abc"));
    assert_eq!(ci.by_sha.len(), 1);
}

/// A remote added (a fork checked out to push to) doesn't blank the dots meanwhile.
#[test]
fn a_project_added_keeps_what_the_others_said() {
    let mut ci = CiStatuses {
        projects: only("org/upstream"),
        by_sha: HashMap::from([("abc".to_owned(), passed())]),
        source: HashMap::from([("abc".to_owned(), project("org/upstream"))]),
        cached: vec![(
            project("org/upstream"),
            HashMap::from([("abc".to_owned(), passed())]),
        )],
        ..CiStatuses::default()
    };
    ci.moved_to(HashSet::from([project("org/upstream"), project("me/fork")]));
    assert_eq!(ci.by_sha.len(), 1);
    assert_eq!(ci.source.len(), 1);
}

/// Removing a secondary source removes only its checks, even if the next read fails.
#[test]
fn removing_a_project_rebuilds_shared_statuses_from_retained_contributions() {
    let mut ci = CiStatuses::default();
    let mut failed = passed();
    failed.state = CheckState::Failed;
    ci.arrived(
        false,
        vec![
            (
                project("me/fork"),
                Ok(HashMap::from([("abc".to_owned(), passed())])),
            ),
            (
                project("org/upstream"),
                Ok(HashMap::from([("abc".to_owned(), failed)])),
            ),
        ],
    );
    assert_eq!(ci.by_sha["abc"].state, CheckState::Failed);
    ci.moved_to(only("me/fork"));
    assert_eq!(ci.by_sha["abc"].state, CheckState::Passed);
    assert_eq!(ci.cached.len(), 1);
    ci.arrived(false, vec![(project("me/fork"), Err("offline".to_owned()))]);
    assert_eq!(ci.by_sha["abc"].state, CheckState::Passed);
    assert_eq!(ci.source["abc"], project("me/fork"));
}

#[test]
fn a_status_without_checks_says_its_state() {
    let status = Status {
        state: CheckState::Passed,
        checks: Vec::new(),
    };
    assert_eq!(tooltip(&status), "CI: passed\nClick for details");
}
