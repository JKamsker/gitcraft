//! Settle only the already-owned child and authenticated remote callbacks.
use super::*;

pub(super) async fn finish(
    child: &mut gitcraft_os::OwnedChild,
    pipes: &mut pipe_io::Pipes,
    options: &mut Options,
    status: &mut Option<ExitStatus>,
    stopped: &mut Option<String>,
    blocking: &Arc<tokio::sync::Semaphore>,
    #[cfg(test)] trace: Option<&trace::Trace>,
) -> Result<bool, String> {
    let mut remote_confirmed = stopped.is_none() || options.stop_remote.is_none();
    if stopped.is_some() {
        // Deliver controller EOF before recovery. Never kill an unconfirmed helper.
        if options.native_controller {
            drop(pipes.input.take());
        }
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Stopping);
        }
        if let Some(stop) = options.stop_remote.take() {
            let result = match Arc::clone(blocking).acquire_owned().await {
                Ok(_permit) => match tokio::task::spawn_blocking(stop).await {
                    Ok(result) => result,
                    Err(error) => Err(format!("cleanup worker failed: {error}")),
                },
                Err(error) => Err(format!("cleanup worker capacity unavailable: {error}")),
            };
            record_stop_confirmation(stopped, &mut remote_confirmed, result);
        }
        // Keep the Unix leader unreaped / Windows Job owned through the final signal.
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Reaping);
        }
        if !options.native_controller {
            match child.finish() {
                Ok(s) => {
                    if status.is_none() {
                        *status = Some(s);
                    }
                }
                Err(e) => {
                    return Err(format!(
                        "{}; could not finish owned process: {e}",
                        stopped.as_deref().unwrap_or("cleanup failed")
                    ));
                }
            }
        }
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.stage(trace::Stage::Draining);
        }
        pipes.drain(child, status, options).await;
    }
    // Successful owners also retire descendants with closed output. Idempotent after stop.
    child.finish().map_err(|e| {
        format!(
            "{}; could not finish owned process: {e}",
            stopped.as_deref().unwrap_or("process completion failed")
        )
    })?;
    if stopped.is_none()
        && let Some(confirm) = options.complete_remote.take()
        && let Err(error) = confirm_delivery(confirm, blocking).await
    {
        *stopped = Some(error);
        remote_confirmed = false;
    }
    if remote_confirmed
        && let Some(retire) = options.retire_remote.take()
        && let Ok(_permit) = Arc::clone(blocking).acquire_owned().await
    {
        // The endpoint requires its atomic receipt before unlinking anything.
        // Failed retirement leaves evidence; it never authorizes further signals.
        let _ = tokio::task::spawn_blocking(retire).await;
    }
    Ok(remote_confirmed)
}

async fn confirm_delivery(
    confirm: Box<dyn FnOnce() -> Result<(), String> + Send>,
    blocking: &Arc<tokio::sync::Semaphore>,
) -> Result<(), String> {
    match Arc::clone(blocking).acquire_owned().await {
        Ok(_permit) => match tokio::task::spawn_blocking(confirm).await {
            Ok(result) => result,
            Err(error) => Err(format!("output delivery confirmation failed: {error}")),
        },
        Err(error) => Err(format!("output delivery confirmation unavailable: {error}")),
    }
}
