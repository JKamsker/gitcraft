//! The shells kept open per machine, in three lanes: quick reads (status, log, diffs…), long
//! network operations (fetch, push…), which would otherwise take every shell of a machine for
//! as long as they last, diff and merge tools, and parallel test runs, each in its own lane.

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
    /// A program other than git that may take minutes (an AI tool): apart from both, so that
    /// it never keeps a fetch or push waiting.
    Apart,
    /// A diff or merge tool (`git difftool`, `git mergetool`) the user works in.
    Tool,
    Test,
}

impl Lane {
    /// How many shells at most: the app reads a repository with several commands at once.
    fn max(self) -> usize {
        match self {
            Lane::Quick => 6,
            Lane::Long => 3,
            Lane::Apart => 2,
            Lane::Tool => 4,
            Lane::Test => TEST_SHELLS,
        }
    }

    /// How long a command waits for a shell when all are busy. A tool doesn't: those open
    /// may stay so for hours.
    fn wait_at_most(self) -> Duration {
        match self {
            Lane::Quick | Lane::Long | Lane::Apart | Lane::Test => WAIT_AT_MOST,
            Lane::Tool => Duration::ZERO,
        }
    }

    /// What a command that found every shell busy for too long says.
    pub(super) fn busy(self, host: &Host) -> String {
        match self {
            Lane::Quick | Lane::Long | Lane::Apart | Lane::Test => format!(
                "every connection to {} is busy; try again when what runs there is done",
                host.label()
            ),
            Lane::Tool => format!(
                "{} diff or merge tools are already open on {}: close one of them first",
                self.max(),
                host.label()
            ),
        }
    }
}

/// How many tests a parallel bisect runs at once on one machine, at most.
pub const TEST_SHELLS: usize = 64;

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
    /// Given back with this as the time it was last used (see [`Pool::take_idle`]).
    keep_used: Option<Instant>,
}

impl Taken<'_> {
    pub(super) fn shell(&mut self) -> Option<&mut Shell> {
        self.shell.as_mut()
    }

    pub(super) fn give_back(mut self) {
        if let Some(mut shell) = self.shell.take() {
            if let Some(used) = self.keep_used {
                shell.used = used;
            }
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
        let cancel = crate::tasks::cancellation();
        let mut state = lock(&self.state);
        loop {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
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
            // Tests open their connections one at a time: ssh servers drop new ones at random
            // past a few being made at once (`MaxStartups`), and a parallel bisect asks for many.
            let one_at_a_time = connected == 0 || lane == Lane::Test;
            let wait_for_first = one_at_a_time && state.connecting > 0;
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
            let left = lane.wait_at_most().saturating_sub(since.elapsed());
            if left.is_zero() {
                return Err(lane.busy(host));
            }
            state = self
                .freed
                .wait_timeout(state, left.min(Duration::from_millis(50)))
                .map(|(state, _)| state)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    }

    /// An idle shell that still answers, if there is one: never a new connection (which could
    /// ask for a password or start WSL).
    /// Only one used within `fresh`, so that reads nobody asked for don't keep alive a
    /// connection nothing else uses; and those reads don't count as use (the shell keeps the
    /// time it was last used, and still closes once idle long enough).
    pub(super) fn take_idle(&self, fresh: Duration) -> Option<Taken<'_>> {
        loop {
            let mut shell = {
                let mut state = lock(&self.state);
                let i = state.idle.iter().rposition(|s| s.used.elapsed() < fresh)?;
                state.idle.remove(i)
            };
            let used = shell.used;
            if shell.usable() {
                shell.used = used;
                let mut taken = self.taken(shell, true);
                taken.keep_used = Some(used);
                return Some(taken);
            }
            drop(shell);
            self.close_one();
        }
    }

    fn taken(&self, shell: Shell, reused: bool) -> Taken<'_> {
        Taken {
            pool: self,
            shell: Some(shell),
            reused,
            keep_used: None,
        }
    }

    /// For tests: `shell` as if it had been opened by the pool and left idle.
    #[cfg(test)]
    pub(super) fn add_idle(&self, shell: Shell) {
        let mut state = lock(&self.state);
        state.open += 1;
        state.idle.push(shell);
    }

    /// For tests: how many shells are open, whether a connection was tried and failed, and
    /// when the idle ones were last used.
    #[cfg(test)]
    pub(super) fn looks(&self) -> (usize, bool, Vec<Instant>) {
        let state = lock(&self.state);
        let used = state.idle.iter().map(|s| s.used).collect();
        (state.open, state.failed.is_some(), used)
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

/// Close idle persistent shells at app exit; active jobs own and close their shells once
/// their cancelled protocol read returns. The transport reuse policy is unchanged.
pub(crate) fn shutdown() {
    if let Some(pools) = POOLS.get() {
        let all: Vec<_> = lock(pools).values().cloned().collect();
        for pool in all {
            let idle = std::mem::take(&mut lock(&pool.state).idle);
            for shell in idle {
                drop(shell);
                pool.close_one();
            }
        }
    }
}
