//! The card's words, the fix each problem gets, and the card's whole way on real repositories:
//! looked at in the background, confirmed, made and undone.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::fix::{self, Fix, Plan, attribute_values, origin_and_value};
use super::text::{self, Place};
use super::{ErrorCard, Stage, cut};
use crate::git::errors::GitProblem;
use crate::git::errors::fix::{ConfigEdit, Scope};
use crate::git::run;

/// The error of `r`, which should have failed (`what`).
fn failure<T: std::fmt::Debug>(r: Result<T, String>, what: &str) -> Result<String, String> {
    match r {
        Ok(v) => Err(format!("{what} worked: {v:?}")),
        Err(e) => Ok(e),
    }
}

const HERE: Place = Place {
    windows: false,
    remote: false,
};
const WINDOWS: Place = Place {
    windows: true,
    remote: false,
};
const WSL: Place = Place {
    windows: true,
    remote: true,
};

fn every_problem() -> Vec<GitProblem> {
    vec![
        GitProblem::DubiousOwnership {
            path: "/srv/r".to_owned(),
        },
        GitProblem::StaleLock {
            lock: "/srv/r/.git/index.lock".to_owned(),
        },
        GitProblem::LineEndings {
            files: vec!["a".to_owned()],
            to_crlf: true,
            fatal: false,
        },
        GitProblem::NameTooLong,
        GitProblem::Certificate {
            detail: "x".to_owned(),
        },
        GitProblem::Transfer { too_large: true },
        GitProblem::Transfer { too_large: false },
        GitProblem::UnrelatedHistories,
        GitProblem::ShallowPush,
        GitProblem::SshKeyRefused { login: None },
    ]
}

#[test]
fn every_problem_is_explained() {
    for problem in every_problem() {
        for place in [HERE, WINDOWS, WSL] {
            assert!(!text::title(&problem).is_empty());
            let said = text::explanation(&problem, place);
            assert!(said.iter().all(|p| !p.trim().is_empty()), "{problem:?}");
            assert!(!said.is_empty(), "{problem:?}");
        }
    }
}

#[test]
fn the_explanation_names_what_it_is_about() {
    let trust = text::explanation(
        &GitProblem::DubiousOwnership {
            path: "/srv/dir with space/ü".to_owned(),
        },
        HERE,
    );
    assert!(trust[0].starts_with("/srv/dir with space/ü"));
    assert!(trust[1].contains("only this one"));
    let lock = text::explanation(
        &GitProblem::StaleLock {
            lock: "C:\\r\\.git\\index.lock".to_owned(),
        },
        WINDOWS,
    );
    assert!(lock[0].contains("git keeps index.lock"), "{lock:?}");
    let files: Vec<String> = (0..7).map(|i| format!("f{i}")).collect();
    let crlf = text::explanation(
        &GitProblem::LineEndings {
            files,
            to_crlf: false,
            fatal: true,
        },
        HERE,
    );
    assert_eq!(crlf[0], "Files: f0, f1, f2 and 4 more.");
    assert!(crlf.iter().any(|p| p.contains("core.safecrlf")));
    // Long paths and schannel are Git for Windows' alone.
    let long = |place| text::explanation(&GitProblem::NameTooLong, place).join(" ");
    assert!(long(WINDOWS).contains("core.longpaths"));
    assert!(!long(HERE).contains("core.longpaths"));
    assert!(!long(WSL).contains("core.longpaths"));
    let cert = |place| {
        text::explanation(
            &GitProblem::Certificate {
                detail: "d".to_owned(),
            },
            place,
        )
        .join(" ")
    };
    assert!(cert(WINDOWS).contains("schannel"));
    assert!(!cert(WSL).contains("schannel"));
    let ssh = |place| {
        text::explanation(
            &GitProblem::SshKeyRefused {
                login: Some("git@example.org".to_owned()),
            },
            place,
        )
    };
    assert!(ssh(HERE)[0].contains("ssh -T git@example.org"));
    assert_eq!(ssh(WSL).len(), 2);
}

#[test]
fn which_problems_get_a_fix() {
    let plan = |problem: &GitProblem, place| fix::plan(problem, place);
    let lock = GitProblem::StaleLock {
        lock: "/r/.git/index.lock".to_owned(),
    };
    assert_eq!(
        plan(&lock, WSL),
        Plan::RemoveLock("/r/.git/index.lock".to_owned())
    );
    assert_eq!(plan(&GitProblem::NameTooLong, WINDOWS), Plan::LongPaths);
    assert_eq!(plan(&GitProblem::NameTooLong, HERE), Plan::Explain);
    assert_eq!(plan(&GitProblem::NameTooLong, WSL), Plan::Explain);
    let cert = GitProblem::Certificate {
        detail: String::new(),
    };
    assert_eq!(plan(&cert, WINDOWS), Plan::WindowsCertificates);
    assert_eq!(plan(&cert, HERE), Plan::Explain);
    for problem in [
        GitProblem::Transfer { too_large: true },
        GitProblem::UnrelatedHistories,
        GitProblem::ShallowPush,
        GitProblem::SshKeyRefused { login: None },
    ] {
        assert_eq!(plan(&problem, WINDOWS), Plan::Explain, "{problem:?}");
    }
}

#[test]
fn the_confirmation_says_which_file_and_what() {
    let add = Fix::Config {
        edit: ConfigEdit::add(Scope::Global, "safe.directory", "/srv/r"),
        file: "/home/me/.gitconfig".to_owned(),
    };
    assert_eq!(add.button(), "Trust this folder");
    assert_eq!(
        add.confirm(),
        "Adds safe.directory = /srv/r to /home/me/.gitconfig. Undo, on this card, takes it out again."
    );
    let set = Fix::Config {
        edit: ConfigEdit {
            scope: Scope::Global,
            key: "http.sslBackend",
            value: "schannel".to_owned(),
            add: false,
            previous: Some("openssl".to_owned()),
        },
        file: "C:/Users/me/.gitconfig".to_owned(),
    };
    assert_eq!(set.button(), "Use Windows' certificates");
    assert!(
        set.confirm()
            .contains("in C:/Users/me/.gitconfig (it is openssl now)")
    );
    let lock = Fix::RemoveLock(crate::git::errors::lock::LockFile {
        shown: "/r/.git/index.lock".to_owned(),
        modified: 0,
        age: 7200,
    });
    assert_eq!(lock.button(), "Remove stale lock");
    assert!(
        lock.confirm()
            .contains("/r/.git/index.lock, last written 2 hours ago")
    );
    assert!(lock.confirm().contains("no undo"));
}

#[test]
fn ages_and_names() {
    assert_eq!(text::age(5), "less than a minute");
    assert_eq!(text::age(120), "2 minutes");
    assert_eq!(text::age(86_400 * 3), "3 days");
    assert_eq!(text::age(u64::MAX), text::age(i64::MAX as u64));
    assert_eq!(text::file_name("C:\\r\\.git\\index.lock"), "index.lock");
    assert_eq!(text::file_name("index.lock"), "index.lock");
}

#[test]
fn reads_what_git_config_and_check_attr_print() {
    assert_eq!(
        origin_and_value("file:/home/me/.gitconfig\0true\0"),
        Some(("/home/me/.gitconfig".to_owned(), "true".to_owned()))
    );
    assert_eq!(
        origin_and_value("file:C:/Program Files/Git/etc/gitconfig\0input\n\0"),
        Some((
            "C:/Program Files/Git/etc/gitconfig".to_owned(),
            "input".to_owned()
        ))
    );
    assert_eq!(origin_and_value(""), None);
    assert_eq!(
        attribute_values("a b.txt\0text\0set\0a b.txt\0eol\0lf\0"),
        ["text", "eol=lf"]
    );
    assert_eq!(
        attribute_values("x\0text\0unset\0x\0eol\0unspecified\0"),
        ["-text"]
    );
    assert!(attribute_values("").is_empty());
}

#[test]
fn git_output_is_cut_on_a_character() {
    let long = "é".repeat(5000);
    let kept = cut(&long, 4001);
    assert!(kept.ends_with('…'));
    assert!(kept.len() <= 4001 + '…'.len_utf8());
    assert_eq!(cut("short", 10), "short");
    assert_eq!(cut("", 10), "");
}

include!("tests/repairs.rs");

/// Constructs inert state directly: no for_error, worker, Git, process list or native API.
#[test]
fn localization_actual_retry_control_keeps_raw_dispatch_identity() {
    use egui_kittest::{Harness, kittest::Queryable};
    crate::i18n::set_language("de");
    let card = ErrorCard {
        problem: GitProblem::NameTooLong,
        dir: PathBuf::from("/nicht-geöffnet/日本語"),
        place: HERE,
        label: "Reload".to_owned(),
        said: "raw git error --literal=unchanged".to_owned(),
        found: Some(Ok(Default::default())),
        stage: Stage::Idle,
        rx: None,
        earlier: None,
        earlier_undone: None,
        earlier_rx: None,
        retried: false,
    };
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, (card, event): &mut (ErrorCard, Option<super::CardEvent>)| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            if let Some(next) = card.ui(ui, true) {
                *event = Some(next);
            }
        },
        (card, None),
    );
    h.run();
    h.get_by_label("Ein Pfad ist zu lang");
    h.get_by_label("Erneut versuchen").click();
    h.run();
    assert_eq!(h.state().1, Some(super::CardEvent::Retry));
    assert_eq!(h.state().0.label(), "Reload");
    assert_eq!(h.state().0.said, "raw git error --literal=unchanged");
    assert!(!h.state().0.can_retry());
    assert!(h.state().0.rx.is_none());
}

#[test]
fn localization_explanations_confirmations_paths_plurals_and_durations() {
    crate::i18n::set_language("de");
    let fix = Fix::Config {
        edit: ConfigEdit::add(
            Scope::Global,
            "safe.directory",
            "/srv/日本語 mit Leerzeichen",
        ),
        file: "C:/Users/ü/.gitconfig".to_owned(),
    };
    assert_eq!(fix.button(), "Diesem Ordner vertrauen");
    assert_eq!(
        fix.confirm(),
        "Fügt safe.directory = /srv/日本語 mit Leerzeichen zu C:/Users/ü/.gitconfig hinzu. Rückgängig auf dieser Karte entfernt den Eintrag wieder."
    );
    let files = (0..1004).map(|i| format!("f{i}")).collect();
    let explanation = text::explanation(
        &GitProblem::LineEndings {
            files,
            to_crlf: true,
            fatal: true,
        },
        HERE,
    );
    assert_eq!(explanation[0], "Dateien: f0, f1, f2 und 1.001 weitere.");
    assert!(explanation.iter().any(|s| s.contains("core.safecrlf")));
    assert_eq!(text::age(60), "1 Minute");
    assert_eq!(text::age(120), "2 Minuten");
    assert_eq!(text::age(7_200), "2 Stunden");
    for problem in every_problem() {
        assert!(!text::title(&problem).starts_with("interaction-"));
        for place in [HERE, WINDOWS, WSL] {
            assert!(
                text::explanation(&problem, place)
                    .iter()
                    .all(|s| !s.starts_with("interaction-"))
            );
        }
    }
    let ssh = text::explanation(
        &GitProblem::SshKeyRefused {
            login: Some("git@literal-host".into()),
        },
        WSL,
    );
    assert!(ssh[0].contains("ssh -T git@literal-host"));
    assert!(ssh[1].contains("anderen Computer"));
    crate::i18n::set_language("en-US");
    assert_eq!(fix.button(), "Trust this folder");
    assert_eq!(text::age(120), "2 minutes");
}
