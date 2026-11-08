//! Dock presentation and a recording-friendly action queue. No application process is launched.
#[cfg(any(target_os = "macos", test))]
use crate::Progress;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type Wake = Arc<dyn Fn() + Send + Sync>;
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Presentation {
    pub badge: Option<&'static str>,
    pub fraction: Option<f32>,
    pub indeterminate: bool,
}
#[cfg(any(target_os = "macos", test))]
impl Presentation {
    pub fn from(progress: Progress) -> Self {
        match progress {
            Progress::None => Self {
                badge: None,
                fraction: None,
                indeterminate: false,
            },
            Progress::Error => Self {
                badge: Some("!"),
                fraction: None,
                indeterminate: false,
            },
            Progress::Value(value) if value.is_finite() => Self {
                badge: None,
                fraction: Some(value.clamp(0.0, 1.0)),
                indeterminate: false,
            },
            _ => Self {
                badge: Some("…"),
                fraction: None,
                indeterminate: true,
            },
        }
    }
}

#[derive(Default)]
struct Commands {
    pending: VecDeque<Vec<String>>,
    dropped: usize,
    wake: Option<Wake>,
}
impl Commands {
    #[cfg(any(target_os = "macos", test))]
    fn push(&mut self, args: Vec<String>) -> Option<Wake> {
        if !self.pending.contains(&args) {
            if self.pending.len() < 64 {
                self.pending.push_back(args);
            } else {
                self.dropped = self.dropped.saturating_add(1);
            }
        }
        self.wake.clone()
    }
    fn take(&mut self) -> Vec<Result<Vec<String>, String>> {
        let mut out: Vec<_> = self.pending.drain(..).map(Ok).collect();
        if self.dropped != 0 {
            out.push(Err(format!(
                "{} Dock actions exceeded the pending-action limit; select them again",
                std::mem::take(&mut self.dropped)
            )));
        }
        out
    }
}
static COMMANDS: Mutex<Commands> = Mutex::new(Commands {
    pending: VecDeque::new(),
    dropped: 0,
    wake: None,
});

pub fn set_native_dock_wake(wake: impl Fn() + Send + Sync + 'static) {
    let wake: Wake = Arc::new(wake);
    let pending = COMMANDS
        .lock()
        .map(|mut state| {
            state.wake = Some(wake.clone());
            !state.pending.is_empty() || state.dropped != 0
        })
        .unwrap_or(false);
    if pending {
        wake();
    }
}
pub fn take_native_dock_requests() -> Vec<Result<Vec<String>, String>> {
    COMMANDS
        .lock()
        .map(|mut state| state.take())
        .unwrap_or_default()
}
#[cfg(target_os = "macos")]
pub(crate) fn activate(args: Vec<String>) {
    let wake = COMMANDS.lock().ok().and_then(|mut state| state.push(args));
    if let Some(wake) = wake {
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presentation_clears_idle_and_distinguishes_failure_fraction_and_unknown() {
        assert_eq!(
            Presentation::from(Progress::None),
            Presentation {
                badge: None,
                fraction: None,
                indeterminate: false
            }
        );
        assert_eq!(Presentation::from(Progress::Error).badge, Some("!"));
        assert_eq!(Presentation::from(Progress::Value(0.4)).fraction, Some(0.4));
        assert_eq!(Presentation::from(Progress::Value(9.0)).fraction, Some(1.0));
        assert!(Presentation::from(Progress::Indeterminate).indeterminate);
        assert!(Presentation::from(Progress::Value(f32::NAN)).indeterminate);
    }
    #[test]
    fn callbacks_record_exact_argv_and_coalesce_only_pending_actions() {
        let mut queue = Commands::default();
        let args = vec!["/Users/a/repo with space;$(literal)".into()];
        queue.push(args.clone());
        queue.push(args.clone());
        queue.push(vec!["--clone".into()]);
        queue.push(vec!["--open".into()]);
        assert_eq!(
            queue.take(),
            [
                Ok(args.clone()),
                Ok(vec!["--clone".into()]),
                Ok(vec!["--open".into()])
            ]
        );
        queue.push(args.clone());
        assert_eq!(queue.take(), [Ok(args)]);
    }
}
