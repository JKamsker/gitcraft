//! An execution owner, never constructible from a persisted PID. On Unix the direct
//! child is a packaged live anchor until final signaling, then reaped exactly once.
use std::io;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus};

#[cfg(all(test, target_os = "macos"))]
#[path = "owned_child_runtime_tests.rs"]
mod owned_runtime_tests;

pub struct OwnedChild {
    process: Option<Process>,
    terminal: Option<Result<ExitStatus, String>>,
}

struct Process {
    child: Child,
    controller: bool,
    controller_status: Option<ExitStatus>,
    controller_permit: Option<super::controller_reaper::Permit>,
    #[cfg(unix)]
    status: Option<super::unix_anchor::Status>,
    #[cfg(windows)]
    job: Option<super::process_job::ProcessJob>,
}

#[cfg(windows)]
struct WindowsLaunch(Command);
#[cfg(windows)]
impl super::suspended::Backend for WindowsLaunch {
    type Child = Child;
    type Job = super::process_job::ProcessJob;
    fn spawn_suspended(&mut self) -> io::Result<Child> {
        self.0.spawn()
    }
    fn assign(&mut self, child: &Child) -> io::Result<Self::Job> {
        super::process_job::assign(child).map_err(io::Error::other)
    }
    fn resume(&mut self, child: &Child) -> io::Result<()> {
        super::process_job::resume(child).map_err(io::Error::other)
    }
    fn abort_and_reap(&mut self, child: &mut Child) {
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl OwnedChild {
    pub fn spawn(command: Command) -> io::Result<Self> {
        Self::start(command, false)
    }
    /// Own only a transport/controller's direct child. Its authenticated endpoint owns the
    /// actual command tree. Never use this for user commands or ordinary local execution.
    pub fn spawn_controller(command: Command) -> io::Result<Self> {
        Self::start(command, true)
    }
    fn start(mut command: Command, controller: bool) -> io::Result<Self> {
        let controller_permit = if controller {
            Some(super::controller_reaper::reserve()?)
        } else {
            None
        };
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            if !controller {
                command.process_group(0);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(if controller { 0x0800_0000 } else { 0x0800_0004 });
        }
        #[cfg(unix)]
        let status = if controller {
            None
        } else {
            Some(super::unix_anchor::prepare(&mut command)?)
        };
        #[cfg(unix)]
        let child = command.spawn()?;
        #[cfg(windows)]
        let (child, job) = if controller {
            (command.spawn()?, None)
        } else {
            let (child, job) = super::suspended::launch(&mut WindowsLaunch(command))?;
            (child, Some(job))
        };
        Ok(Self {
            process: Some(Process {
                child,
                controller,
                controller_status: None,
                controller_permit,
                #[cfg(unix)]
                status,
                #[cfg(windows)]
                job,
            }),
            terminal: None,
        })
    }

    /// Diagnostic only; no public operation accepts a process/group number.
    pub fn id(&self) -> u32 {
        self.process.as_ref().map_or(0, |p| p.child.id())
    }
    pub fn stdin(&mut self) -> Option<ChildStdin> {
        self.process.as_mut()?.child.stdin.take()
    }
    pub fn stdout(&mut self) -> Option<ChildStdout> {
        self.process.as_mut()?.child.stdout.take()
    }
    pub fn stderr(&mut self) -> Option<ChildStderr> {
        self.process.as_mut()?.child.stderr.take()
    }

    /// Observe exit without releasing the Unix identity anchor. No SIGCHLD handler or
    /// other waiter may reap this private child. Windows retains the Job HANDLE instead.
    pub fn observe(&mut self) -> io::Result<Option<ExitStatus>> {
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| io::Error::other("owner retired"))?;
        if process.controller {
            if process.controller_status.is_none() {
                process.controller_status = process.child.try_wait()?;
            }
            return Ok(process.controller_status);
        }
        #[cfg(unix)]
        {
            process
                .status
                .as_mut()
                .ok_or_else(|| io::Error::other("missing group anchor"))?
                .observe()
        }
        #[cfg(windows)]
        {
            process.child.try_wait()
        }
    }

    /// Finish is idempotent. All group/Job signals precede reaping, including failures.
    pub fn finish(&mut self) -> io::Result<ExitStatus> {
        finish(
            &mut self.terminal,
            self.process
                .as_mut()
                .ok_or_else(|| io::Error::other("owner retired"))?,
        )
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
        if self.controller {
            if self.controller_status.is_none() {
                self.controller_status = self.child.try_wait()?;
            }
            return controller_retirement(self.controller_status).map(|_| ());
        }
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
        if let Some(job) = &self.job {
            job.terminate().map_err(io::Error::other)?;
        }
        Ok(())
    }

    fn reap(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.controller_status {
            return Ok(status);
        }
        if self.controller {
            return Err(io::Error::other(
                "native owner preserved for receipt recovery",
            ));
        }
        // Handle-based/direct-child fallback also bounds a refused group signal.
        let _ = self.child.kill();
        self.child.wait()
    }
}

// Deliberately has no signal operation: a live native helper is recovery authority.
fn controller_retirement(status: Option<ExitStatus>) -> io::Result<ExitStatus> {
    status.ok_or_else(|| {
        io::Error::other("native owner still running; cleanup unknown, helper preserved")
    })
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.finish().is_err()
            && self.process.as_ref().is_some_and(|p| p.controller)
            && let Some(mut process) = self.process.take()
            && let Some(permit) = process.controller_permit.take()
        {
            super::controller_reaper::retain(process.child, permit);
        }
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn live_controller_retirement_has_no_signal_authority() {
        assert!(controller_retirement(None).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                controller_retirement(Some(ExitStatus::from_raw(0)))
                    .ok()
                    .and_then(|s| s.code()),
                Some(0)
            );
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            assert_eq!(
                controller_retirement(Some(ExitStatus::from_raw(0)))
                    .ok()
                    .and_then(|s| s.code()),
                Some(0)
            );
        }
    }
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
