use super::*;
use eframe::Storage as _;
use std::collections::HashMap;

#[derive(Default)]
struct Memory(HashMap<String, String>);

impl eframe::Storage for Memory {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn settings_of_an_older_version_still_load() {
    let mut storage = Memory::default();
    storage.set_string(
        "s",
        "(ui_scale: 1.25, refs_width: 270.0, show_author: false, show_date: true, \
         show_hash: false, avatars: false, editor: \"subl\")"
            .to_owned(),
    );
    let back: Option<Settings> = eframe::get_value(&storage, "s");
    let back = back.unwrap_or_default();
    assert_eq!(
        (
            back.ui_scale,
            back.refs_width,
            back.show_author,
            back.show_date
        ),
        (1.25, 270.0, false, true)
    );
    assert_eq!(back.editor, "subl");
    let fresh = Settings::default();
    assert_eq!(
        (back.show_when, back.files_as_tree, back.author_width),
        (fresh.show_when, fresh.files_as_tree, fresh.author_width)
    );
}

#[test]
fn resetting_the_columns_keeps_which_show() {
    let mut settings = Settings {
        refs_width: 500.0,
        graph_width: 90.0,
        author_width: 300.0,
        date_width: 60.0,
        last_resized: Some(crate::repo::columns::Bar::Author),
        column_push: 30.0,
        show_hash: true,
        show_author: false,
        compact_graph: true,
        ..Settings::default()
    };
    settings.reset_columns();
    assert_eq!(
        settings,
        Settings {
            show_hash: true,
            show_author: false,
            compact_graph: true,
            ..Settings::default()
        }
    );
}

#[test]
fn a_compact_graph_fits_its_lanes_again() {
    let mut settings = Settings {
        graph_width: 200.0,
        ..Settings::default()
    };
    settings.set_compact_graph(true);
    assert!(settings.compact_graph);
    assert_eq!(settings.graph_width, 0.0);
}

#[test]
fn settings_survive_a_restart() {
    let settings = Settings {
        last_resized: Some(crate::repo::columns::Bar::Graph),
        column_push: 12.5,
        compact_graph: true,
        diff_whole_file: true,
        pull_mode: crate::repo::pull::PullMode::Rebase,
        ..Settings::default()
    };
    let mut storage = Memory::default();
    eframe::set_value(&mut storage, "s", &settings);
    let back: Option<Settings> = eframe::get_value(&storage, "s");
    assert_eq!(back, Some(settings));
}

/// Earlier versions kept "Commit as…" identities under `profiles`; they stay identities, and
/// everything else becomes the default profile's.
#[test]
fn settings_from_before_profiles_get_a_default_profile() {
    let mut storage = Memory::default();
    storage.set_string(
        "s",
        "(editor: \"subl\", profiles: [(label: \"Work\", name: \"Ann\", email: \"ann@corp\")])"
            .to_owned(),
    );
    let back: Option<Settings> = eframe::get_value(&storage, "s");
    let mut back = back.unwrap_or_default();
    back.ensure_profile();
    assert_eq!(
        back.identities,
        [SavedIdentity {
            label: "Work".into(),
            name: "Ann".into(),
            email: "ann@corp".into(),
        }]
    );
    assert_eq!(back.user_profiles, [UserProfile::default()]);
    assert_eq!(back.profile().id, back.active_profile);
    assert!(!back.profile().sets_git());
}

#[test]
fn profiles_are_added_named_and_kept_apart() {
    let mut settings = Settings::default();
    settings.profile_mut().author_email = "me@home".into();
    let work = settings.add_profile(&settings.new_profile_name("Profile"), false);
    let copy = settings.add_profile(&settings.new_profile_name("Default"), true);
    assert_eq!(settings.new_profile_name("Profile"), "Profile 2");
    let names: Vec<&str> = settings
        .user_profiles
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["Default", "Profile", "Default 2"]);
    // A new one is empty, a copy has the settings of the one in use, and none is switched to.
    let by_id = |id| settings.user_profiles.iter().find(|p| p.id == id).cloned();
    assert_eq!(by_id(work).map(|p| p.author_email), Some(String::new()));
    assert_eq!(
        by_id(copy).map(|p| p.author_email),
        Some("me@home".to_owned())
    );
    assert_eq!(settings.active_profile, 1);
    assert_eq!((work, copy), (2, 3));

    // The active one deleted: another takes over.
    settings.user_profiles.retain(|p| p.id != 1);
    settings.ensure_profile();
    assert_eq!(settings.active_profile, work);
    settings.user_profiles.clear();
    settings.ensure_profile();
    assert_eq!(settings.user_profiles.len(), 1);
}
