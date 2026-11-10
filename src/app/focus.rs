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
    fn starting_focused_still_recognizes_a_real_return() {
        let mut state = State::default();
        assert_eq!(state.regained(true), None);
        assert_eq!(state.regained(false), None);
        assert_eq!(state.regained(true), Some(false));
    }
}
