//! Opt-in, quiet synthesized earcons. The device is opened only for an explicit sound request.
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cue {
    Lane,
    Merge,
    Celebration,
}

static UNAVAILABLE: AtomicBool = AtomicBool::new(false);

pub(crate) fn unavailable() -> bool {
    UNAVAILABLE.load(Ordering::Relaxed)
}
pub(crate) fn retry() {
    UNAVAILABLE.store(false, Ordering::Relaxed);
}

// All test fixtures record dispatches; they never open an audio device.
#[cfg(test)]
thread_local! { static RECORDED: std::cell::RefCell<Vec<Cue>> = const { std::cell::RefCell::new(Vec::new()) }; }

#[cfg(test)]
pub(crate) fn take_recorded() -> Vec<Cue> {
    RECORDED.with(|sounds| std::mem::take(&mut *sounds.borrow_mut()))
}

pub(crate) fn play(cue: Cue) {
    #[cfg(test)]
    RECORDED.with(|sounds| sounds.borrow_mut().push(cue));
    #[cfg(not(test))]
    {
        use std::sync::{Mutex, OnceLock, mpsc};
        static QUEUE: OnceLock<Mutex<Option<mpsc::SyncSender<Cue>>>> = OnceLock::new();
        if unavailable() {
            return;
        }
        let Ok(mut sender) = QUEUE.get_or_init(|| Mutex::new(None)).lock() else {
            return;
        };
        if sender.is_none() {
            let (tx, rx) = mpsc::sync_channel(2);
            match std::thread::Builder::new()
                .name("gitcraft-earcons".into())
                .spawn(move || worker(rx))
            {
                Ok(_) => *sender = Some(tx),
                Err(_) => {
                    UNAVAILABLE.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
        if let Some(queue) = sender.as_ref()
            && matches!(
                queue.try_send(cue),
                Err(mpsc::TrySendError::Disconnected(_))
            )
        {
            *sender = None;
            UNAVAILABLE.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(not(test))]
fn worker(rx: std::sync::mpsc::Receiver<Cue>) {
    // Keep stream and sink on their owning thread. No subprocess, terminal bell or unsafe FFI.
    let mut output = None;
    loop {
        let cue = if output.is_some() {
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(cue) => cue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    output = None;
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(cue) => cue,
                Err(_) => break,
            }
        };
        if unavailable() {
            continue;
        }
        if output.is_none() {
            match rodio::OutputStreamBuilder::open_default_stream() {
                Ok(mut stream) => {
                    stream.log_on_drop(false);
                    let sink = rodio::Sink::connect_new(stream.mixer());
                    output = Some((stream, sink));
                }
                Err(_) => {
                    UNAVAILABLE.store(true, Ordering::Relaxed);
                    continue;
                }
            }
        }
        if let Some((_, sink)) = &output {
            // Drop queued cues rather than building an audible backlog during rapid navigation.
            if sink.len() < 2 {
                sink.append(rodio::buffer::SamplesBuffer::new(1, 24_000, samples(cue)));
            }
        }
    }
}

fn samples(cue: Cue) -> Vec<f32> {
    let frequencies: &[f32] = match cue {
        Cue::Lane => &[440.0, 554.37],
        Cue::Merge => &[392.0, 493.88, 587.33],
        Cue::Celebration => &[523.25, 659.25, 783.99],
    };
    let count = match cue {
        Cue::Lane => 1920,
        Cue::Merge => 3120,
        Cue::Celebration => 4800,
    };
    (0..count)
        .map(|i| {
            let t = i as f32 / 24_000.0;
            let phase = i as f32 / count as f32;
            // Raised-sine envelope avoids clicks; a quiet harmonic chord is distinct from a bell.
            let envelope = (std::f32::consts::PI * phase).sin().powi(2) * (-8.0 * phase).exp();
            let chord: f32 = frequencies
                .iter()
                .map(|frequency| (std::f32::consts::TAU * frequency * t).sin())
                .sum();
            chord / frequencies.len() as f32 * envelope * 0.12
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn earcons_are_short_soft_and_recorded_without_devices() {
        for cue in [Cue::Lane, Cue::Merge, Cue::Celebration] {
            let wave = samples(cue);
            assert!(wave.len() <= 4800);
            assert!(
                wave.iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 0.12)
            );
            assert_eq!(wave[0], 0.0);
            play(cue);
        }
        RECORDED.with(|sounds| assert_eq!(sounds.borrow().len(), 3));
    }
}
