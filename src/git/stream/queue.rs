//! The output a streamed command printed, on its way from the process service's event loop
//! to the caller that reads it into lines.
//!
//! It is bounded by the bytes it holds, not by how many chunks: a command printing many
//! short lines hands over a chunk per write, and the caller being descheduled for a moment
//! must not fail it. A consumer that cannot keep up overflows it, and stays overflowed.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

/// The most bytes on their way at once.
pub(super) const LIMIT: usize = 8 << 20;

type Item = (usize, Vec<u8>);

/// Where the event loop puts chunks: never blocks.
#[derive(Clone)]
pub(super) struct Sender {
    sent: mpsc::Sender<Item>,
    queued: Arc<AtomicUsize>,
    full: Arc<AtomicBool>,
    limit: usize,
}

/// Where the caller takes them from.
pub(super) struct Receiver {
    received: mpsc::Receiver<Item>,
    queued: Arc<AtomicUsize>,
    full: Arc<AtomicBool>,
}

pub(super) fn queue(limit: usize) -> (Sender, Receiver) {
    let (sent, received) = mpsc::channel();
    let queued = Arc::new(AtomicUsize::new(0));
    let full = Arc::new(AtomicBool::new(false));
    (
        Sender {
            sent,
            queued: queued.clone(),
            full: full.clone(),
            limit,
        },
        Receiver {
            received,
            queued,
            full,
        },
    )
}

impl Sender {
    /// Queues `bytes` of output `which` (0 out, 1 err), unless that would hold more than
    /// the limit or the receiver is gone: then the queue is full from now on.
    pub(super) fn push(&self, which: usize, bytes: &[u8]) {
        if self.full.load(Ordering::Acquire) {
            return;
        }
        let before = self.queued.fetch_add(bytes.len(), Ordering::AcqRel);
        if before.saturating_add(bytes.len()) > self.limit
            || self.sent.send((which, bytes.to_vec())).is_err()
        {
            self.full.store(true, Ordering::Release);
        }
    }

    pub(super) fn full(&self) -> bool {
        self.full.load(Ordering::Acquire)
    }
}

impl Receiver {
    /// The next chunk, waiting up to `wait`; `None` when there is none (yet, or any more).
    pub(super) fn recv_timeout(&self, wait: Duration) -> Option<Item> {
        self.received
            .recv_timeout(wait)
            .ok()
            .map(|item| self.took(item))
    }

    /// The next chunk already queued.
    pub(super) fn try_recv(&self) -> Option<Item> {
        self.received.try_recv().ok().map(|item| self.took(item))
    }

    pub(super) fn full(&self) -> bool {
        self.full.load(Ordering::Acquire)
    }

    fn took(&self, item: Item) -> Item {
        self.queued.fetch_sub(item.1.len(), Ordering::AcqRel);
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Many more chunks than a chunk-counted queue held all fit, in order, while their
    /// bytes do.
    #[test]
    fn many_small_chunks_fit_while_their_bytes_do() {
        let (sender, receiver) = queue(LIMIT);
        for i in 0..10_000u32 {
            sender.push(usize::from(i % 2 == 1), &i.to_le_bytes());
        }
        assert!(!sender.full());
        for i in 0..10_000u32 {
            assert_eq!(
                receiver.try_recv(),
                Some((usize::from(i % 2 == 1), i.to_le_bytes().to_vec()))
            );
        }
        assert_eq!(receiver.try_recv(), None);
    }

    /// What is taken off makes room again; going over the limit fills it for good.
    #[test]
    fn going_over_the_limit_fills_it_for_good() {
        let (sender, receiver) = queue(10);
        sender.push(0, &[0; 6]);
        assert_eq!(receiver.try_recv().map(|(_, b)| b.len()), Some(6));
        sender.push(0, &[0; 6]);
        assert!(!receiver.full());
        sender.push(1, &[0; 5]);
        assert!(receiver.full());
        assert_eq!(receiver.try_recv().map(|(_, b)| b.len()), Some(6));
        assert_eq!(receiver.try_recv(), None);
        sender.push(0, &[0; 1]);
        assert!(receiver.full());
        assert_eq!(receiver.try_recv(), None);
    }

    /// With nobody reading any more, the queue says it is full.
    #[test]
    fn a_receiver_gone_fills_it() {
        let (sender, receiver) = queue(LIMIT);
        drop(receiver);
        sender.push(0, b"line\n");
        assert!(sender.full());
    }
}
