//! Work done the first time a view asks for it, on a thread of its own: the view gets
//! nothing until it is done, then what it made.

use std::sync::{Arc, Mutex};

enum State<T> {
    NotAsked,
    Working,
    Done(T),
}

/// A value made in the background when first asked for.
pub struct Lazy<T> {
    state: Arc<Mutex<State<T>>>,
}

impl<T> Default for Lazy<T> {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::NotAsked)),
        }
    }
}

impl<T: Clone + Send + 'static> Lazy<T> {
    /// The value if it is made; else starts making it with `make` (the first time), and asks
    /// `ctx` to draw again once it is. A `make` that panics leaves `fallback`.
    pub fn get(
        &self,
        ctx: &egui::Context,
        make: impl FnOnce() -> T + Send + 'static,
        fallback: T,
    ) -> Option<T> {
        let mut s = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &*s {
            State::Done(v) => return Some(v.clone()),
            State::Working => return None,
            State::NotAsked => *s = State::Working,
        }
        let (state, ctx) = (self.state.clone(), ctx.clone());
        crate::connections::spawn(move || {
            let made =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(make)).unwrap_or(fallback);
            *state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = State::Done(made);
            ctx.request_repaint();
        });
        None
    }
}
