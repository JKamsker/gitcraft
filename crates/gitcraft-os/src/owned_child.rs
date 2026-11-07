//! An execution owner, never constructible from a persisted PID. On Unix the direct
//! child remains unreaped until the final group signal; its identity cannot be recycled.
use std::io;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus};

pub struct OwnedChild {
    process: Process,
    terminal: Option<Result<ExitStatus, String>>,
}

struct Process {
    child: Child,
    #[cfg(windows)]
    job: super::process_job::ProcessJob,
}

impl OwnedChild {
    pub fn spawn(mut command: Command) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0004);
        }
        #[allow(unused_mut)]
        let mut child = command.spawn()?;
        #[cfg(windows)]
        let job = match super::process_job::supervise_suspended_process(child.id()) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::other(error));
            }
        };
        Ok(Self {
            process: Process {
                child,
                #[cfg(windows)]
                job,
            },
            terminal: None,
        })
    }

    /// Diagnostic only; no public operation accepts a process/group number.
    pub fn id(&self) -> u32 {
        self.process.child.id()
    }
    pub fn stdin(&mut self) -> Option<ChildStdin> {
        self.process.child.stdin.take()
    }
    pub fn stdout(&mut self) -> Option<ChildStdout> {
        self.process.child.stdout.take()
    }
    pub fn stderr(&mut self) -> Option<ChildStderr> {
        self.process.child.stderr.take()
    }

    /// Observe exit without releasing the Unix identity anchor. No SIGCHLD handler or
    /// other waiter may reap this private child. Windows retains the Job HANDLE instead.
    pub fn observe(&mut self) -> io::Result<Option<ExitStatus>> {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            // SAFETY: initialized output, exact owned direct child, and WNOWAIT prevents
            // reaping. All group signals occur before the single private wait below.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.process.child.id(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: waitid initializes the SIGCHLD variant, including si_pid/status.
            if unsafe { info.si_pid() } == 0 {
                return Ok(None);
            }
            let code = unsafe { info.si_status() };
            let raw = match info.si_code {
                libc::CLD_EXITED => code << 8,
                libc::CLD_KILLED => code,
                libc::CLD_DUMPED => code | 0x80,
                _ => return Err(io::Error::other("unexpected child wait state")),
            };
            Ok(Some(ExitStatus::from_raw(raw)))
        }
        #[cfg(windows)]
        {
            self.process.child.try_wait()
        }
    }

    /// Finish is idempotent. All group/Job signals precede reaping, including failures.
    pub fn finish(&mut self) -> io::Result<ExitStatus> {
        finish(&mut self.terminal, &mut self.process)
    }
}

trait Control {
    fn stop(&mut self) -> io::Result<()>;
    fn reap(&mut self) -> io::Result<ExitStatus>;
}

fn finish(
    terminal: &mut Option<Result<ExitStatus, String>>,
    process: &mut impl Control,
) -> io::Result<ExitStatus> {
    if let Some(result) = terminal {
        return result.clone().map_err(io::Error::other);
    }
    let stopped = process.stop();
    let reaped = process.reap();
    let result = stopped.and(reaped);
    // A wait error may mean ownership was lost. Never send another numeric signal after
    // a reaping attempt, and never turn a previous cleanup error into an acknowledgement.
    *terminal = Some(result.as_ref().copied().map_err(ToString::to_string));
    result
}

impl Control for Process {
    /// Stop remaining inherited descendants while ownership is still anchored. Even an
    /// exited leader is not reaped first. EPERM is an error (including Darwin's zombie-only
    /// case): callers must report uncertain cleanup rather than silently claim success.
    fn stop(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let pid = i32::try_from(self.child.id()).map_err(io::Error::other)?;
            if pid <= 1 {
                return Err(io::Error::other("invalid owned child"));
            }
            // SAFETY: spawn established this exact child's group. The private Child has
            // not been reaped, so the group number cannot refer to a different lifetime.
            if unsafe { libc::killpg(pid, libc::SIGKILL) } != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
        }
        #[cfg(windows)]
        self.job.terminate().map_err(io::Error::other)?;
        Ok(())
    }

    fn reap(&mut self) -> io::Result<ExitStatus> {
        // Handle-based/direct-child fallback also bounds a refused group signal.
        let _ = self.child.kill();
        self.child.wait()
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[derive(Default)]
    struct Recording {
        events: Vec<&'static str>,
        refuse: bool,
    }
    impl Control for Recording {
        fn stop(&mut self) -> io::Result<()> {
            self.events.push("signal owned group/job while anchor held");
            if self.refuse {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        }
        fn reap(&mut self) -> io::Result<ExitStatus> {
            self.events.push("reap anchor");
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                Ok(ExitStatus::from_raw(7 << 8))
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::ExitStatusExt;
                Ok(ExitStatus::from_raw(7))
            }
        }
    }
    #[test]
    fn final_signal_precedes_reap_and_never_runs_after_retirement() -> io::Result<()> {
        for refuse in [false, true] {
            let mut backend = Recording {
                refuse,
                ..Default::default()
            };
            let mut terminal = None;
            let result = finish(&mut terminal, &mut backend);
            assert_eq!(result.is_err(), refuse);
            let repeated = finish(&mut terminal, &mut backend);
            assert_eq!(repeated.is_err(), refuse);
            if !refuse {
                assert_eq!(repeated?.code(), Some(7));
            }
            assert_eq!(
                backend.events,
                ["signal owned group/job while anchor held", "reap anchor"]
            );
        }
        Ok(())
    }
}
