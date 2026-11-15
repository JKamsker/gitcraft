//! Retry only a failed exec of our packaged helper, before any target can start.
use std::io;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub(super) fn spawn(command: &mut Command, deadline: Instant) -> io::Result<Child> {
    retry_text_busy(
        || command.spawn(),
        || {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(2)));
            Instant::now() < deadline
        },
    )
}

fn retry_text_busy<T>(
    mut attempt: impl FnMut() -> io::Result<T>,
    mut wait: impl FnMut() -> bool,
) -> io::Result<T> {
    loop {
        match attempt() {
            // install() closes its writer before publishing the helper. A concurrent
            // fork can still retain that descriptor until its exec closes CLOEXEC
            // handles. Linux then refuses our exec with ETXTBSY. No child was started,
            // so retrying this helper is safe; never retry the user's target Command.
            Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) => {
                if !wait() {
                    return Err(error);
                }
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn only_text_busy_retries_and_success_is_returned_once() -> io::Result<()> {
        for code in [libc::EACCES, libc::ENOENT, libc::ENOEXEC, libc::EINTR] {
            let attempts = Cell::new(0);
            let result: io::Result<()> = retry_text_busy(
                || {
                    attempts.set(attempts.get() + 1);
                    Err(io::Error::from_raw_os_error(code))
                },
                || panic!("a permanent spawn failure must not wait"),
            );
            assert_eq!(
                result.expect_err("spawn must fail").raw_os_error(),
                Some(code)
            );
            assert_eq!(attempts.get(), 1);
        }
        let attempts = Cell::new(0);
        let waits = Cell::new(0);
        let result = retry_text_busy(
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() < 3 {
                    Err(io::Error::from_raw_os_error(libc::ETXTBSY))
                } else {
                    Ok("exact child")
                }
            },
            || {
                waits.set(waits.get() + 1);
                true
            },
        )?;
        assert_eq!(result, "exact child");
        assert_eq!((attempts.get(), waits.get()), (3, 2));
        Ok(())
    }

    #[test]
    fn exhausted_startup_budget_preserves_the_original_error() {
        let attempts = Cell::new(0);
        let result: io::Result<()> = retry_text_busy(
            || {
                attempts.set(attempts.get() + 1);
                Err(io::Error::from_raw_os_error(libc::ETXTBSY))
            },
            || attempts.get() < 3,
        );
        assert_eq!(attempts.get(), 3);
        assert_eq!(
            result.expect_err("budget must stop retries").raw_os_error(),
            Some(libc::ETXTBSY)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_inherited_writer_can_outlive_install_and_refuse_exec() -> io::Result<()> {
        use std::fs::OpenOptions;
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;
        use std::path::PathBuf;
        use std::process::Stdio;

        const ISOLATED: &str = "GITCRAFT_TEST_INHERITED_WRITER";
        if std::env::var_os(ISOLATED).is_none() {
            // No other test may fork while this regression deliberately holds a
            // writable executable. Run the exact regression in its own process.
            let output = Command::new(std::env::current_exe()?)
                .args([
                    "unix_anchor::startup::tests::an_inherited_writer_can_outlive_install_and_refuse_exec",
                    "--exact",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(ISOLATED, "1")
                .output()?;
            assert!(output.status.success(), "{output:?}");
            return Ok(());
        }

        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
                if let Some(parent) = self.0.parent() {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        }
        struct WriterChild(Child);
        impl WriterChild {
            fn finish(&mut self) -> io::Result<()> {
                // EOF releases this exact child without a signal or process selector.
                drop(self.0.stdin.take());
                assert!(self.0.wait()?.success());
                Ok(())
            }
        }
        impl Drop for WriterChild {
            fn drop(&mut self) {
                let _ = self.finish();
            }
        }

        let fixture = Fixture(super::super::install()?);
        std::fs::copy("/bin/true", &fixture.0)?;
        let writer = OpenOptions::new().write(true).open(&fixture.0)?;
        // Model a concurrent fork retaining install's writer. Preserve it through
        // exec in this exact child, so EOF controls its lifetime without sleeps.
        let fd = writer.as_raw_fd();
        let mut holder = Command::new("/bin/cat");
        holder
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: only an async-signal-safe fcntl on the inherited live descriptor.
        // The parent's descriptor keeps CLOEXEC, and writer lives until spawn returns.
        unsafe {
            holder.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut inherited = WriterChild(holder.spawn()?);
        drop(writer);

        let mut command = Command::new(&fixture.0);
        let attempts = Cell::new(0);
        let mut child = retry_text_busy(
            || {
                attempts.set(attempts.get() + 1);
                let result = command.spawn();
                if attempts.get() == 1 {
                    assert_eq!(
                        result
                            .as_ref()
                            .expect_err("inherited writer blocks exec")
                            .raw_os_error(),
                        Some(libc::ETXTBSY)
                    );
                }
                result
            },
            || {
                inherited.finish().expect("release only the owned writer");
                true
            },
        )?;
        assert!(child.wait()?.success());
        assert_eq!(attempts.get(), 2);
        Ok(())
    }
}
