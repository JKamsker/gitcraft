//! Test overrides follow the source repository across background workers. A panic removes
//! only that fixture's registration; unrelated tabs cannot inherit a fake agent command.
use super::*;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

static COMMANDS: LazyLock<Mutex<BTreeMap<PathBuf, CommandLine>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub(super) struct Registration(PathBuf);
pub(super) fn register(repo: &Path, command: CommandLine) -> Registration {
    let mut commands = COMMANDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(
        !commands.contains_key(repo),
        "fixture already owns an override"
    );
    commands.insert(repo.to_owned(), command);
    Registration(repo.to_owned())
}
pub(in crate::repo::resolver) fn command_for(repo: &Path) -> Option<CommandLine> {
    COMMANDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(repo)
        .cloned()
}
impl Drop for Registration {
    fn drop(&mut self) {
        COMMANDS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.0);
    }
}

#[test]
#[allow(clippy::panic)] // The fixture must unwind to verify owner cleanup.
fn synthetic_fixture_overrides_are_scoped_and_unwind_removes_only_the_owner() {
    let a = Path::new("synthetic/resolver/a");
    let b = Path::new("synthetic/resolver/b");
    let _a = register(a, stand_in());
    let unwound = std::panic::catch_unwind(|| {
        let _b = register(b, slow_stand_in(9));
        assert!(command_for(a).is_some());
        assert!(command_for(b).is_some());
        assert!(command_for(Path::new("synthetic/unrelated")).is_none());
        panic!("synthetic fixture failure");
    });
    assert!(unwound.is_err());
    assert!(command_for(b).is_none());
    assert!(command_for(a).is_some());
}
