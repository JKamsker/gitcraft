//! The window's end of the channel to `gitcraft mcp`: requests come in on the channel's own
//! threads and wait here until the window's next frame takes them, without the window ever
//! waiting for them.

use std::sync::mpsc::{Receiver, channel};

use super::link::{self, Incoming, Listener};

/// Requests from agents, while the window listens for them.
pub struct Inbox {
    rx: Receiver<Incoming>,
    /// Listening, while it is kept.
    _listener: Option<Listener>,
    /// Why agents can't reach this window, if they can't.
    pub error: Option<String>,
}

impl Inbox {
    /// Starts listening for agents; asks for a frame whenever a request comes in.
    pub fn start(ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        // The tests' windows stay out of the user's own list of windows.
        if cfg!(test) {
            return Self {
                rx,
                _listener: None,
                error: None,
            };
        }
        let repaint = ctx.clone();
        let deliver = move |incoming: Incoming| {
            if tx.send(incoming).is_ok() {
                repaint.request_repaint();
            }
        };
        let listened = link::dir()
            .ok_or_else(|| "there is no folder for gitcraft's settings on this system".to_owned())
            .and_then(|dir| link::listen(&dir, deliver));
        match listened {
            Ok(listener) => Self {
                rx,
                _listener: Some(listener),
                error: None,
            },
            Err(e) => Self {
                rx,
                _listener: None,
                error: Some(e),
            },
        }
    }

    /// The requests that came in since the last frame.
    pub fn take(&self) -> Vec<Incoming> {
        self.rx.try_iter().collect()
    }
}
