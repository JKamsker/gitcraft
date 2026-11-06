//! Doing one thing for many repositories (look at each, fetch each…): a few at a time, so a
//! workspace of fifty doesn't start fifty gits at once, and each given so long, so one that
//! hangs (a machine that went to sleep) doesn't hold up the rest.
//!
//! A job that takes too long is reported as such and left to end on its own: a git here can't
//! be stopped from outside without leaving a lock behind, and one elsewhere is stopped there
//! by its own time limit (see [`crate::remote::Request::timeout`]).

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// One job's result, by the index of its item.
pub type Done<R> = (usize, Result<R, String>);

/// How many jobs run at once: in all, and of one group (the repositories of one machine, which
/// share the few shells kept open to it).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub total: usize,
    pub per_group: usize,
}

/// The items waiting, and how many of each group run.
struct Queue<T> {
    waiting: VecDeque<(usize, String, T)>,
    running: HashMap<String, usize>,
}

/// Runs `work` on every item, at most `limits` at a time (`group` tells an item's group), each
/// given `timeout`. Results come on the receiver as they finish, in any order, each followed by
/// a call of `wake` (the window's repaint). Dropping the receiver stops the jobs not started yet.
pub fn run_bounded<T, R>(
    items: Vec<T>,
    limits: Limits,
    timeout: Duration,
    group: impl Fn(&T) -> String,
    work: impl Fn(T) -> Result<R, String> + Send + Sync + 'static,
    wake: impl Fn() + Send + Sync + 'static,
) -> Receiver<Done<R>>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = channel();
    let workers = limits.total.max(1).min(items.len());
    let waiting = items
        .into_iter()
        .enumerate()
        .map(|(i, item)| (i, group(&item), item))
        .collect();
    let queue = Arc::new((
        Mutex::new(Queue {
            waiting,
            running: HashMap::new(),
        }),
        Condvar::new(),
    ));
    let per_group = limits.per_group.max(1);
    let work = Arc::new(work);
    let wake = Arc::new(wake);
    for _ in 0..workers {
        let queue = Arc::clone(&queue);
        let work = Arc::clone(&work);
        let wake = Arc::clone(&wake);
        let tx = tx.clone();
        std::thread::spawn(move || worker(&queue, per_group, &work, &*wake, &tx, timeout));
    }
    rx
}

/// The next item whose group has room, waiting while every waiting item's group is full;
/// `None` once nothing waits.
fn next<T>(queue: &(Mutex<Queue<T>>, Condvar), per_group: usize) -> Option<(usize, String, T)> {
    let (lock, freed) = queue;
    let mut q = lock.lock().ok()?;
    loop {
        if q.waiting.is_empty() {
            return None;
        }
        let room = q
            .waiting
            .iter()
            .position(|(_, g, _)| q.running.get(g).copied().unwrap_or(0) < per_group);
        if let Some(at) = room
            && let Some(item) = q.waiting.remove(at)
        {
            *q.running.entry(item.1.clone()).or_default() += 1;
            return Some(item);
        }
        q = freed.wait(q).ok()?;
    }
}

/// One less running of `group`.
fn finished<T>(queue: &(Mutex<Queue<T>>, Condvar), group: &str) {
    let (lock, freed) = queue;
    if let Ok(mut q) = lock.lock()
        && let Some(n) = q.running.get_mut(group)
    {
        *n = n.saturating_sub(1);
    }
    freed.notify_all();
}

fn worker<T, R, W>(
    queue: &(Mutex<Queue<T>>, Condvar),
    per_group: usize,
    work: &Arc<W>,
    wake: &(dyn Fn() + Send + Sync),
    tx: &Sender<Done<R>>,
    timeout: Duration,
) where
    T: Send + 'static,
    R: Send + 'static,
    W: Fn(T) -> Result<R, String> + Send + Sync + 'static,
{
    while let Some((index, group, item)) = next(queue, per_group) {
        let result = with_timeout(Arc::clone(work), item, timeout);
        finished(queue, &group);
        if tx.send((index, result)).is_err() {
            // Nobody waits for the rest any more.
            if let Ok(mut q) = queue.0.lock() {
                q.waiting.clear();
            }
            queue.1.notify_all();
            return;
        }
        wake();
    }
}

/// `work(item)`, or an error once `timeout` has passed without an answer.
fn with_timeout<T, R, W>(work: Arc<W>, item: T, timeout: Duration) -> Result<R, String>
where
    T: Send + 'static,
    R: Send + 'static,
    W: Fn(T) -> Result<R, String> + Send + Sync + 'static,
{
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(work(item));
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(format!(
            "no answer within {}; it may still be running",
            seconds(timeout)
        )),
        Err(RecvTimeoutError::Disconnected) => Err("stopped unexpectedly".to_owned()),
    }
}

/// "20 seconds", "2 minutes".
fn seconds(d: Duration) -> String {
    let s = d.as_secs();
    if s == 0 {
        "a second".to_owned()
    } else if s >= 120 && s.is_multiple_of(60) {
        format!("{} minutes", s / 60)
    } else if s == 1 {
        "1 second".to_owned()
    } else {
        format!("{s} seconds")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn collect<R>(rx: &Receiver<Done<R>>) -> Vec<Done<R>> {
        let mut all: Vec<Done<R>> = rx.iter().collect();
        all.sort_by_key(|(i, _)| *i);
        all
    }

    #[test]
    fn never_more_than_the_limit_at_once() {
        let running = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let (r, m) = (Arc::clone(&running), Arc::clone(&most));
        let rx = run_bounded(
            (0..20).collect(),
            Limits {
                total: 3,
                per_group: 3,
            },
            Duration::from_secs(10),
            |_| String::new(),
            move |n: u32| {
                let now = r.fetch_add(1, Ordering::SeqCst) + 1;
                m.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(15));
                r.fetch_sub(1, Ordering::SeqCst);
                Ok(n * 2)
            },
            || {},
        );
        let all = collect(&rx);
        assert_eq!(all.len(), 20);
        assert!(all.iter().all(|(i, r)| r == &Ok(*i as u32 * 2)));
        assert!(most.load(Ordering::SeqCst) <= 3);
        assert!(most.load(Ordering::SeqCst) >= 2, "ran side by side");
    }

    #[test]
    fn one_that_hangs_times_out_and_the_rest_go_on() {
        let woken = Arc::new(AtomicUsize::new(0));
        let w = Arc::clone(&woken);
        let rx = run_bounded(
            vec![0u64, 2000, 0, 0],
            Limits {
                total: 1,
                per_group: 1,
            },
            Duration::from_millis(200),
            |_| String::new(),
            |ms| {
                std::thread::sleep(Duration::from_millis(ms));
                Ok(ms)
            },
            move || {
                w.fetch_add(1, Ordering::SeqCst);
            },
        );
        let all = collect(&rx);
        assert_eq!(all.len(), 4);
        let err = all[1].1.clone().err().unwrap_or_default();
        assert!(err.contains("no answer within"), "{err}");
        assert_eq!(all[3].1, Ok(0));
        assert_eq!(woken.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn errors_and_panics_are_results_too() {
        let rx = run_bounded(
            vec![1, 2, 3],
            Limits {
                total: 2,
                per_group: 2,
            },
            Duration::from_secs(5),
            |_| String::new(),
            |n: i32| match n {
                1 => Err("refused".to_owned()),
                2 => std::panic::resume_unwind(Box::new("a bug")),
                _ => Ok(n),
            },
            || {},
        );
        let all = collect(&rx);
        assert_eq!(all[0].1, Err("refused".to_owned()));
        assert_eq!(all[1].1, Err("stopped unexpectedly".to_owned()));
        assert_eq!(all[2].1, Ok(3));
    }

    #[test]
    fn nothing_to_do_ends_at_once() {
        let limits = Limits {
            total: 4,
            per_group: 4,
        };
        let rx = run_bounded(
            Vec::<u8>::new(),
            limits,
            Duration::from_secs(1),
            |_| String::new(),
            Ok,
            || {},
        );
        assert!(rx.recv().is_err());
    }

    #[test]
    fn a_dropped_receiver_stops_what_hasnt_started() {
        let started = Arc::new(AtomicUsize::new(0));
        let s = Arc::clone(&started);
        let rx = run_bounded(
            (0..50).collect::<Vec<u32>>(),
            Limits {
                total: 1,
                per_group: 1,
            },
            Duration::from_secs(5),
            |_| String::new(),
            move |n| {
                s.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                Ok(n)
            },
            || {},
        );
        let _ = rx.recv();
        drop(rx);
        std::thread::sleep(Duration::from_millis(150));
        assert!(started.load(Ordering::SeqCst) < 10);
    }

    /// Six jobs in all at once, but only two of one machine's: the rest of its jobs wait while
    /// another machine's run.
    #[test]
    fn a_group_never_runs_more_than_its_share() {
        let running: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
        let most: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
        let (r, m) = (Arc::clone(&running), Arc::clone(&most));
        let items: Vec<(String, u32)> = (0..12)
            .map(|i| (if i < 8 { "build" } else { "here" }.to_owned(), i))
            .collect();
        let rx = run_bounded(
            items,
            Limits {
                total: 6,
                per_group: 2,
            },
            Duration::from_secs(10),
            |(g, _)| g.clone(),
            move |(g, i)| {
                let now = {
                    let mut map = r.lock().map_err(|e| e.to_string())?;
                    let n = map.entry(g.clone()).or_default();
                    *n += 1;
                    *n
                };
                if let Ok(mut m) = m.lock() {
                    let best = m.entry(g.clone()).or_default();
                    *best = (*best).max(now);
                }
                std::thread::sleep(Duration::from_millis(20));
                if let Ok(mut map) = r.lock() {
                    *map.entry(g).or_default() -= 1;
                }
                Ok(i)
            },
            || {},
        );
        assert_eq!(collect(&rx).len(), 12);
        let most = most.lock().map(|m| m.clone()).unwrap_or_default();
        assert_eq!(most.get("build"), Some(&2));
        assert_eq!(most.get("here"), Some(&2));
    }

    #[test]
    fn durations_read_as_words() {
        assert_eq!(seconds(Duration::from_secs(20)), "20 seconds");
        assert_eq!(seconds(Duration::from_secs(300)), "5 minutes");
        assert_eq!(seconds(Duration::from_secs(1)), "1 second");
        assert_eq!(seconds(Duration::from_millis(200)), "a second");
    }
}
