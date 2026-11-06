//! The shells kept open per machine, in two lanes: quick reads (status, log, diffs…) and long
//! network operations (fetch, push…), which would otherwise take every shell of a machine for
//! as long as they last and leave the tab waiting for its reads.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::Host;
use super::shell::{Shell, lock};

/// Which of a machine's shells a command runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Lane {
    Quick,
    Long,
}

impl Lane {
    /// How many shells at most: the app reads a repository with several commands at once.
    fn max(self) -> usize {
        match self {
            Lane::Quick => 6,
            Lane::Long => 3,
        }
    }
}

/// After a connection couldn't be made, commands fail at once with its error for this long
/// (while none is open) rather than each trying, and waiting for, one of their own.
const FAILED_FOR: Duration = Duration::from_secs(5);

/// How long a command waits for a shell when all are busy.
const WAIT_AT_MOST: Duration = Duration::from_secs(120);

/// A shell idle this long is closed (and a connection with it).
const IDLE_FOR: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) struct Pool {
    state: Mutex<State>,
    freed: Condvar,
}

#[derive(Default)]
struct State {
    idle: Vec<Shell>,
    /// Shells open or being opened, idle or not.
    open: usize,
    /// Of those, being opened.
    connecting: usize,
    /// The last time one couldn't be opened, and why.
    failed: Option<(Instant, String)>,
}

/// A shell taken from a pool: back to it with [`Taken::give_back`]; dropped (whatever happens
/// to the caller), it is closed and its place freed.
pub(super) struct Taken<'a> {
    pool: &'a Pool,
    shell: Option<Shell>,
    /// It ran commands before (it may have gone away since).
    pub(super) reused: bool,
}

impl Taken<'_> {
    pub(super) fn shell(&mut self) -> Option<&mut Shell> {
        self.shell.as_mut()
    }

    pub(super) fn give_back(mut self) {
        if let Some(shell) = self.shell.take() {
            lock(&self.pool.state).idle.push(shell);
            self.pool.freed.notify_one();
        }
    }
}

impl Drop for Taken<'_> {
    fn drop(&mut self) {
        if let Some(shell) = self.shell.take() {
            drop(shell);
            self.pool.close_one();
        }
    }
}

impl Pool {
    /// A shell to run a command in: an idle one that still answers, else a new one while
    /// there are fewer than the lane allows, else the next one freed. While none is
    /// connected, one connection is made at a time and serves all who wait for it.
    pub(super) fn take(&self, host: &Host, lane: Lane) -> Result<Taken<'_>, String> {
        let since = Instant::now();
        let mut state = lock(&self.state);
        loop {
            if let Some(mut shell) = state.idle.pop() {
                drop(state);
                if shell.usable() {
                    return Ok(self.taken(shell, true));
                }
                drop(shell);
                self.close_one();
                state = lock(&self.state);
                continue;
            }
            let connected = state.open - state.connecting;
            if connected == 0
                && let Some((at, why)) = &state.failed
                && at.elapsed() < FAILED_FOR
            {
                return Err(why.clone());
            }
            let wait_for_first = connected == 0 && state.connecting > 0;
            if !wait_for_first && state.open < lane.max() {
                state.open += 1;
                state.connecting += 1;
                drop(state);
                let started = super::session::start_shell(host);
                let mut state = lock(&self.state);
                state.connecting -= 1;
                self.freed.notify_all();
                return match started {
                    Ok(shell) => {
                        state.failed = None;
                        drop(state);
                        Ok(self.taken(shell, false))
                    }
                    Err(why) => {
                        state.open -= 1;
                        state.failed = Some((Instant::now(), why.clone()));
                        Err(why)
                    }
                };
            }
            let left = WAIT_AT_MOST.saturating_sub(since.elapsed());
            if left.is_zero() {
                return Err(format!(
                    "every connection to {} is busy; try again when what runs there is done",
                    host.label()
                ));
            }
            state = self
                .freed
                .wait_timeout(state, left)
                .map(|(state, _)| state)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    }

    fn taken(&self, shell: Shell, reused: bool) -> Taken<'_> {
        Taken {
            pool: self,
            shell: Some(shell),
            reused,
        }
    }

    /// One shell less (already closed by the caller, outside the lock).
    fn close_one(&self) {
        let mut state = lock(&self.state);
        state.open = state.open.saturating_sub(1);
        self.freed.notify_all();
    }

    /// Closes the shells idle longer than [`IDLE_FOR`].
    fn close_idle(&self) {
        let old: Vec<Shell> = {
            let mut state = lock(&self.state);
            let (old, kept) = std::mem::take(&mut state.idle)
                .into_iter()
                .partition(|s| s.used.elapsed() > IDLE_FOR);
            state.idle = kept;
            state.open = state.open.saturating_sub(old.len());
            old
        };
        // Outside the lock: closing waits for the processes to end.
        drop(old);
    }
}

type Pools = Mutex<HashMap<(Host, Lane), Arc<Pool>>>;

static POOLS: OnceLock<Pools> = OnceLock::new();

/// Whether a shell to `host` is connected, in either lane: a command for it now won't make
/// a new connection (which may ask for a password).
pub(super) fn connected(host: &Host) -> bool {
    let Some(pools) = POOLS.get() else {
        return false;
    };
    let pools: Vec<Arc<Pool>> = [Lane::Quick, Lane::Long]
        .iter()
        .filter_map(|lane| lock(pools).get(&(host.clone(), *lane)).cloned())
        .collect();
    pools.iter().any(|pool| {
        let state = lock(&pool.state);
        state.open > state.connecting
    })
}

/// The pool of `host`'s shells of `lane`.
pub(super) fn pool(host: &Host, lane: Lane) -> Arc<Pool> {
    let pools = POOLS.get_or_init(|| {
        // Closes idle shells now and then, also of machines no tab shows any more.
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(30));
                let all: Vec<Arc<Pool>> = POOLS
                    .get()
                    .map(|p| lock(p).values().cloned().collect())
                    .unwrap_or_default();
                for pool in all {
                    pool.close_idle();
                }
            }
        });
        Mutex::default()
    });
    Arc::clone(lock(pools).entry((host.clone(), lane)).or_default())
}
