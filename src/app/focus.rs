//! Distinguish initial window activation from a return after using another application.

pub(super) struct State {
    previous: bool,
    seen: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            previous: true,
            seen: false,
        }
    }
}

impl State {
    /// A newly created child window has not received its initial focus event yet.
    pub(super) fn unfocused() -> Self {
        Self {
            previous: false,
            seen: false,
        }
    }

    /// A focus gain, and whether it is the first activation. Only that activation may
    /// reuse a recent startup read; subsequent returns can follow external Git changes.
    pub(super) fn regained(&mut self, focused: bool) -> Option<bool> {
        let gained = focused && !self.previous;
        let initial = !self.seen;
        self.previous = focused;
        self.seen |= focused;
        gained.then_some(initial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_initial_activation_can_reuse_a_recent_startup_read() {
        let mut state = State::default();
        assert_eq!(state.regained(false), None);
        assert_eq!(state.regained(true), Some(true));
        assert_eq!(state.regained(true), None);
        assert_eq!(state.regained(false), None);
        assert_eq!(state.regained(true), Some(false));
    }

    #[test]
    fn child_windows_have_independent_initial_activation() {
        let mut root = State::default();
        root.regained(true);
        let mut child = State::unfocused();
        std::mem::swap(&mut root, &mut child);
        assert_eq!(root.regained(true), Some(true));
        std::mem::swap(&mut root, &mut child);
        assert_eq!(root.regained(false), None);
        assert_eq!(root.regained(true), Some(false));
        assert_eq!(child.regained(false), None);
        assert_eq!(child.regained(true), Some(false));
    }

    #[test]
    fn starting_focused_still_recognizes_a_real_return() {
        let mut state = State::default();
        assert_eq!(state.regained(true), None);
        assert_eq!(state.regained(false), None);
        assert_eq!(state.regained(true), Some(false));
    }
}
