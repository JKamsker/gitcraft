//! AppKit updates run on the application thread; the model owns all state reduction.
use super::{MenuItem, TaskbarState};
fn link(item: &MenuItem) -> gitcraft_os::Link {
    gitcraft_os::Link {
        title: item.title.clone(),
        args: item.args.clone(),
        tooltip: item.tooltip.clone(),
    }
}
#[cfg(test)]
thread_local! { static RECORDED: std::cell::RefCell<(Vec<gitcraft_os::Link>, Vec<gitcraft_os::Progress>)> = const { std::cell::RefCell::new((Vec::new(), Vec::new())) }; }

pub(super) fn set_menu(items: &[MenuItem]) {
    let recent: Vec<_> = items.iter().filter(|i| i.recent).map(link).collect();
    let tasks: Vec<_> = items.iter().filter(|i| !i.recent).map(link).collect();
    #[cfg(not(test))]
    let _ = gitcraft_os::set_dock_menu(&recent, &tasks);
    #[cfg(test)]
    RECORDED.with(|record| record.borrow_mut().0 = recent.into_iter().chain(tasks).collect());
}
pub fn set_progress(state: TaskbarState) -> bool {
    let progress = match state {
        TaskbarState::Idle => gitcraft_os::Progress::None,
        TaskbarState::Failed => gitcraft_os::Progress::Error,
        TaskbarState::Running(None) => gitcraft_os::Progress::Indeterminate,
        TaskbarState::Running(Some(value)) => gitcraft_os::Progress::Value(value),
    };
    #[cfg(not(test))]
    {
        gitcraft_os::set_dock_progress(progress).is_ok()
    }
    #[cfg(test)]
    {
        RECORDED.with(|record| record.borrow_mut().1.push(progress));
        true
    }
}
pub fn refresh() {
    #[cfg(not(test))]
    gitcraft_os::refresh_dock();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_shell_adapter_records_menu_and_all_progress_transitions() {
        let paths = vec![std::path::PathBuf::from("/Users/a/repo with space")];
        set_menu(&crate::os_shell::menu_items(&paths));
        for state in [
            TaskbarState::Running(Some(0.4)),
            TaskbarState::Running(None),
            TaskbarState::Failed,
            TaskbarState::Idle,
        ] {
            assert!(set_progress(state));
        }
        RECORDED.with(|record| {
            let record = record.borrow();
            assert_eq!(
                record.0.iter().map(|i| i.args.clone()).collect::<Vec<_>>(),
                [
                    vec!["/Users/a/repo with space".to_owned()],
                    vec!["--clone".to_owned()],
                    vec!["--open".to_owned()]
                ]
            );
            assert_eq!(
                record.1,
                [
                    gitcraft_os::Progress::Value(0.4),
                    gitcraft_os::Progress::Indeterminate,
                    gitcraft_os::Progress::Error,
                    gitcraft_os::Progress::None
                ]
            );
        });
    }
}
