//! Bisect slots, parallel rounds, cached answers, and final search conclusions.
use super::*;
use crate::parallel_bisect::runner;

/// A commit tested for the search, or one of its ends, tested first to make sure the test
/// tells them apart.
#[derive(Clone, Debug)]
pub(super) struct Job {
    pub(super) hash: String,
    pub(super) tree: String,
    /// For an end: what it was marked.
    pub(super) end: Option<Verdict>,
}

/// The ends to check: the bad commit and the good ones, with their trees.
pub(super) fn ends(setup: &Setup, search: &Search) -> Result<Vec<Job>, String> {
    let mut ends = Vec::new();
    for (hash, marked) in std::iter::once((&setup.bad, Verdict::Bad))
        .chain(setup.good.iter().map(|g| (g, Verdict::Good)))
    {
        let tree = match search.dag().index_of(hash) {
            Some(i) => search.dag().tree(i).to_owned(),
            None => git::run(
                Some(&setup.repo),
                &["rev-parse", &format!("{hash}^{{tree}}")],
            )?
            .trim()
            .to_owned(),
        };
        ends.push(Job {
            hash: hash.clone(),
            tree,
            end: Some(marked),
        });
    }
    Ok(ends)
}

/// A worktree on the machine, made when first needed.
pub(super) struct Slot {
    pub(super) n: usize,
    pub(super) path: PathBuf,
    pub(super) there: String,
    pub(super) made: bool,
}

impl Slot {
    pub(super) fn new(ws: &Workspace, n: usize) -> Self {
        let (path, there) = ws.slot(n);
        Slot {
            n,
            path,
            there,
            made: false,
        }
    }

    /// Puts the worktree on `hash`, as it is in the commit (what an earlier test left is
    /// removed; ignored files, a build's output, stay). A new worktree is added one at a time
    /// (git reads the others' files as it adds one), then checked out side by side.
    pub(super) fn checkout(
        &mut self,
        ws: &Workspace,
        hash: &str,
        limit: Duration,
    ) -> Result<(), String> {
        let failed = |exit: git::Exit| {
            let said = String::from_utf8_lossy(&exit.stderr).trim().to_owned();
            format!(
                "could not check out {} in a worktree on {}: {said}",
                short(hash),
                ws.machine.label()
            )
        };
        if !self.made {
            let add = [
                "worktree",
                "add",
                "-q",
                "--no-checkout",
                "--detach",
                "--force",
                &self.there,
                hash,
            ];
            let exit = ws.adding(|| ws.run_git(&ws.repo, &add, limit))?;
            if exit.code != Some(0) {
                return Err(failed(exit));
            }
            self.made = true;
        }
        for args in [
            &["checkout", "-q", "--detach", "--force", hash][..],
            &["clean", "-q", "-ffd"],
        ] {
            let exit = ws.run_git(&self.path, args, limit)?;
            if exit.code != Some(0) {
                return Err(failed(exit));
            }
        }
        Ok(())
    }

    /// Checks `hash` out and runs the test there.
    pub(super) fn test(
        &mut self,
        ws: &Workspace,
        hash: &str,
        setup: &Setup,
        stop: &(dyn Fn() -> bool + Sync),
    ) -> Result<Ran, String> {
        self.checkout(ws, hash, setup.timeout.max(Duration::from_secs(600)))?;
        if stop() {
            return Ok(Ran {
                ended: Ended::Stopped,
                output: String::new(),
            });
        }
        runner::with_prepared_log(
            || ws.prepare_test_log(self.n),
            |log| {
                let authority =
                    ws.authority(crate::remote::workspace_authority::FenceMode::Dispatch)?;
                let owner_record = ws.owner_record(&authority)?;
                runner::run(
                    &runner::Test {
                        dir: &self.path,
                        control: &ws.repo,
                        owner_record: &owner_record,
                        authority: &authority,
                        log,
                        command: &setup.command,
                        limit: setup.timeout,
                    },
                    stop,
                )
                .inspect_err(|_| ws.invalidate())
            },
        )
    }
}

/// The rounds of a bisect, and what it knows so far.
pub(super) struct Run<'a> {
    pub(super) setup: &'a Setup,
    pub(super) ws: &'a Workspace,
    pub(super) cache: &'a TreeCache,
    pub(super) out: &'a Out,
    pub(super) stop: &'a AtomicBool,
    pub(super) search: Search,
    pub(super) slots: Vec<Slot>,
    /// The machine, as the cache knows it.
    pub(super) machine: String,
}

impl Run<'_> {
    pub(super) fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Round after round until the answer is known: the ends first (with as many commits as
    /// there is room for beside them), then k commits at a time.
    pub(super) fn rounds(&mut self, mut ends: Vec<Job>) -> Result<Outcome, String> {
        // Ends tested before need no test now.
        let mut untested = Vec::new();
        for end in ends.drain(..) {
            match self.cached(&end.tree) {
                Some(verdict) => {
                    if let Some(outcome) = self.take(&end, verdict, String::new(), true) {
                        return Ok(outcome);
                    }
                }
                None => untested.push(end),
            }
        }
        let k = self.slots.len();
        let mut number = 0;
        loop {
            if self.stopped() {
                return Ok(Outcome::Cancelled);
            }
            let take = untested.len().min(k);
            let mut jobs: Vec<Job> = untested.drain(..take).collect();
            let busy: Vec<String> = jobs.iter().map(|j| j.tree.clone()).collect();
            jobs.extend(self.picks(k - jobs.len(), &busy));
            if jobs.is_empty() {
                return self.conclusion();
            }
            number += 1;
            self.out.send(Event::Round {
                number,
                rounds_left: self.search.rounds_left(k).saturating_sub(1),
                testing: jobs.iter().map(|j| j.hash.clone()).collect(),
            });
            if let Some(outcome) = self.round(jobs)? {
                return Ok(outcome);
            }
        }
    }

    /// Up to `n` commits to test, none with a tree of `busy` (the ends tested beside them) or
    /// one tested before: those answers are taken in at once, and the commits picked again.
    pub(super) fn picks(&mut self, n: usize, busy: &[String]) -> Vec<Job> {
        let busy: Vec<&str> = busy.iter().map(String::as_str).collect();
        loop {
            let picks = self.search.picks(n, &busy);
            let mut known = false;
            let mut jobs = Vec::new();
            for i in picks {
                let (hash, tree) = (
                    self.search.dag().hash(i).to_owned(),
                    self.search.dag().tree(i).to_owned(),
                );
                let job = Job {
                    hash,
                    tree,
                    end: None,
                };
                match self.cached(&job.tree) {
                    Some(verdict) => {
                        known = true;
                        self.take(&job, verdict, String::new(), true);
                    }
                    None => jobs.push(job),
                }
            }
            if !known {
                return jobs;
            }
        }
    }

    pub(super) fn cached(&self, tree: &str) -> Option<Verdict> {
        self.cache.get(&self.machine, &self.setup.command, tree)
    }

    /// Runs `jobs` side by side, one per worktree, taking their answers in as they come. A
    /// test whose answer is no longer needed (its commit left the candidates) is stopped.
    pub(super) fn round(&mut self, jobs: Vec<Job>) -> Result<Option<Outcome>, String> {
        let dropped: Vec<AtomicBool> = jobs.iter().map(|_| AtomicBool::new(false)).collect();
        let mut slots = std::mem::take(&mut self.slots);
        let (tx, rx) = channel::<(usize, Result<Ran, String>)>();
        let (ws, setup, stop) = (self.ws, self.setup, self.stop);
        let mut ended = None;
        std::thread::scope(|scope| {
            for (j, (job, slot)) in jobs.iter().zip(slots.iter_mut()).enumerate() {
                let tx = tx.clone();
                let dropped = &dropped[j];
                scope.spawn(move || {
                    let stop = || dropped.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed);
                    let _ = tx.send((j, slot.test(ws, &job.hash, setup, &stop)));
                });
            }
            drop(tx);
            let mut running: HashSet<usize> = (0..jobs.len()).collect();
            while !running.is_empty() {
                let (j, result) = match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(got) => got,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                running.remove(&j);
                let job = &jobs[j];
                if let Some(error) =
                    record_round_result(&mut ended, result, self.stopped(), |ran| {
                        self.answer(job, ran)
                    })
                {
                    self.out
                        .send(Event::Warning(format!("{}: {error}", short(&job.hash))));
                }
                if ended.is_some() {
                    dropped
                        .iter()
                        .for_each(|d| d.store(true, Ordering::Relaxed));
                    continue;
                }
                for &other in &running {
                    let moot = jobs[other].end.is_none()
                        && self
                            .search
                            .dag()
                            .index_of(&jobs[other].hash)
                            .is_some_and(|i| !self.search.is_candidate(i));
                    if moot && !dropped[other].swap(true, Ordering::Relaxed) {
                        self.out.send(Event::Dropped(jobs[other].hash.clone()));
                    }
                }
            }
        });
        self.slots = slots;
        finish_round(ended, self.stopped())
    }

    /// What a test that came back says: `Some` when it ends the bisect. An exit code of 128 or
    /// more stops it on an end, as `git bisect run` would; on a commit in between, which a bisect
    /// one commit at a time might never have tested, it counts as skipped (not kept for its
    /// tree), with a warning.
    pub(super) fn answer(&mut self, job: &Job, ran: Ran) -> Result<Option<Outcome>, String> {
        let verdict = match ran.ended {
            Ended::Exited(code) => match interpret(code) {
                Ok(verdict) => verdict,
                Err(e) if job.end.is_some() => {
                    let output = ran.output.trim();
                    return Err(format!("{} {e}\n{output}", short(&job.hash))
                        .trim_end()
                        .to_owned());
                }
                Err(_) => {
                    self.out.send(Event::Warning(format!(
                        "The test of {} ended with exit code {code} (a crash or a signal): it \
                         counts as skipped.",
                        short(&job.hash)
                    )));
                    return Ok(self.take(job, Verdict::Skip, ran.output, true));
                }
            },
            Ended::TimedOut => Verdict::TimedOut,
            Ended::Stopped => return Ok(None),
        };
        Ok(self.take(job, verdict, ran.output, false))
    }

    /// Takes in `verdict` for `job`, kept for its tree and told to the tab; `Some` when it
    /// ends the bisect (an end the test says the opposite of).
    pub(super) fn take(
        &mut self,
        job: &Job,
        verdict: Verdict,
        output: String,
        cached: bool,
    ) -> Option<Outcome> {
        if !cached {
            self.cache
                .put(&self.machine, &self.setup.command, &job.tree, verdict);
        }
        let contradicts = job
            .end
            .is_some_and(|marked| !verdict.is_skip() && marked != verdict);
        let same_tree: Vec<String> = if contradicts {
            Vec::new()
        } else {
            self.search
                .mark_tree(&job.tree, verdict)
                .into_iter()
                .map(|i| self.search.dag().hash(i).to_owned())
                .collect()
        };
        self.out.send(Event::Tested {
            hash: job.hash.clone(),
            verdict,
            cached,
            output: output.clone(),
            same_tree,
        });
        if contradicts {
            let hash = job.hash.clone();
            return Some(match job.end {
                Some(Verdict::Good) => Outcome::GoodEndFails { hash, output },
                _ => Outcome::BadEndPasses { hash, output },
            });
        }
        self.send_remaining();
        None
    }

    pub(super) fn send_remaining(&self) {
        let dag = self.search.dag();
        let left: HashSet<String> = self
            .search
            .candidates()
            .into_iter()
            .map(|i| dag.hash(i).to_owned())
            .collect();
        self.out.send(Event::Remaining(Arc::new(left)));
    }

    pub(super) fn conclusion(&self) -> Result<Outcome, String> {
        let dag = self.search.dag();
        match self.search.conclusion() {
            Some(Conclusion::FirstBad(i)) => Ok(Outcome::FirstBad(dag.hash(i).to_owned())),
            Some(Conclusion::OneOf(all)) => Ok(Outcome::OneOf(
                all.into_iter().map(|i| dag.hash(i).to_owned()).collect(),
            )),
            None => Err("the bisect ran out of commits to test before it ended".to_owned()),
        }
    }
}
