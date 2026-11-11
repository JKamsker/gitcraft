use super::security::tests::effective;
use super::*;
use std::time::Instant;

/// Actual OpenSSH, not a fake multiplexer: disposable server/config supplied by the harness.
/// GITCRAFT_SSH_TEST_HOST must differ from the special in-process `gitcraft-loopback` host.
#[test]
#[ignore = "requires a disposable real sshd; run ci/test-ssh-transport.py"]
fn real_openssh_reuses_concurrently_and_recovers() -> Result<(), Box<dyn std::error::Error>> {
    use super::super::super::{Host, Request};
    let dest = std::env::var("GITCRAFT_SSH_TEST_HOST")?;
    assert_ne!(dest, super::super::super::session::LOOPBACK);
    let host = Host::Ssh(dest.clone());
    let request = |script: &str| Request {
        script: script.to_owned(),
        ..Request::default()
    };
    let connection = |script: &str| -> Result<String, String> {
        let output = super::super::super::exec(&host, &request(script))?;
        if output.code != 0 {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let first = connection("printf '%s' \"$SSH_CONNECTION\"")?;
    assert!(!first.is_empty());
    let since = Instant::now();
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| connection("sleep 0.2; printf '%s' \"$SSH_CONNECTION\"")))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().map_err(|_| "worker panicked".to_owned())?)
            .collect::<Result<Vec<_>, String>>()
    })?;
    assert!(
        results.iter().all(|id| id == &first),
        "all shells must share a TCP connection: {results:?}"
    );
    assert!(
        since.elapsed() < Duration::from_millis(1100),
        "independent commands must run concurrently"
    );
    let mut options = crate::processes::Options::default();
    let command =
        super::super::super::streaming_command(&host, &request("printf '%s' \"$SSH_CONNECTION\""))?
            .configure(&mut options);
    let stream = crate::processes::run(command, options)?.complete()?;
    assert!(stream.status.success());
    assert_eq!(String::from_utf8_lossy(&stream.stdout).trim(), first);
    // Preparing a streaming request must not reserve the pooled-shell startup gate
    // while it waits in the process service queue: cancellation cleanup needs that gate.
    let queued = super::super::super::streaming_command(&host, &request("true"))?;
    let same_config = config(&dest).ok_or("no managed configuration")?;
    let (sent, received) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let opening = same_config.take();
        let _ = sent.send(opening.is_some());
    });
    let available = received.recv_timeout(Duration::from_secs(1));
    drop(queued);
    waiter.join().map_err(|_| "gate waiter panicked")?;
    assert_eq!(
        available,
        Ok(true),
        "queued streaming preparation must leave cleanup free to start"
    );
    // The server allows ten sessions. Six idle pooled shells plus twelve streaming
    // clients exceed that limit: OpenSSH auto must fall back to independent connections
    // for refused mux channels without retrying a command that already started.
    let overflow = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..12)
            .map(|_| {
                scope.spawn(|| {
                    let mut options = crate::processes::Options::default();
                    let command = super::super::super::streaming_command(
                        &host,
                        &request("sleep 0.3; printf '%s' \"$SSH_CONNECTION\""),
                    )?
                    .configure(&mut options);
                    crate::processes::run(command, options)?.complete()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .map_err(|_| "overflow worker panicked".to_owned())?
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    assert!(
        overflow
            .iter()
            .all(|o| o.status.success() && !o.stdout.is_empty()),
        "mux overflow must fall back cleanly"
    );
    assert!(
        overflow
            .iter()
            .any(|o| String::from_utf8_lossy(&o.stdout).trim() != first),
        "test must actually exceed MaxSessions"
    );
    assert_eq!(connection("printf '%s' \"$SSH_CONNECTION\"")?, first);
    // Killing one streaming client closes only its channel, not the authenticated master.
    let streaming = super::super::super::streaming_command(&host, &request("sleep 30"))?;
    let lease = streaming.lease.clone();
    let mut options = crate::processes::Options::default();
    let command = streaming.configure(&mut options);
    let ticket = crate::processes::service()?.submit(command, options)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while lease.endpoint().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    ticket.cancel();
    let stopped = ticket.wait()?;
    assert!(
        stopped
            .stopped
            .as_ref()
            .is_some_and(|reason| !reason.contains("unconfirmed"))
    );
    assert_eq!(connection("printf '%s' \"$SSH_CONNECTION\"")?, first);
    let config = config(&dest).ok_or("no managed configuration")?;
    let effective = effective(&config.path, &dest)?;
    let socket = effective
        .lines()
        .find_map(|line| line.strip_prefix("controlpath "))
        .ok_or("no controlpath")?;
    assert!(
        socket.starts_with("/tmp/gc-"),
        "test config must allow managed multiplexing"
    );
    let pool = super::super::super::pool::pool(&host, super::super::super::pool::Lane::Quick);
    assert!(
        !pool.looks().2.is_empty(),
        "reconnect must start with pooled clients still open"
    );
    assert!(!pool.idle_shells_exited()?);
    let status = Command::new("ssh")
        .args(["-S", socket, "-O", "exit", "--", &dest])
        .status()?;
    assert!(status.success());
    // Unlinking the master socket precedes the pooled clients observing EOF. Wait
    // for their owned processes too, so the next request cannot reach a dying shell.
    // Commands sent before a disconnect remain deliberately non-retriable.
    let until = Instant::now() + Duration::from_secs(5);
    while Path::new(socket).exists() || !pool.idle_shells_exited()? {
        assert!(
            Instant::now() < until,
            "the stopped master and its pooled clients must exit before reconnecting"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    // Leave a stale socket, as after a crashed master. OpenSSH auto removes/replaces it.
    let stale = std::os::unix::net::UnixListener::bind(socket)?;
    drop(stale);
    let reconnected = connection("printf '%s' \"$SSH_CONNECTION\"")?;
    assert_ne!(reconnected, first);
    assert_eq!(connection("printf '%s' \"$SSH_CONNECTION\"")?, reconnected);
    super::super::super::pool::shutdown();
    Ok(())
}

// Keep historical runner names, including the real SSH harness entry point.
#[test]
fn sockets_and_configuration_are_private_and_instance_local()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::sockets_and_configuration_are_private_and_instance_local()
}

#[test]
fn explicit_user_choices_and_aliases_precede_managed_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::explicit_user_choices_and_aliases_precede_managed_defaults()
}

#[test]
fn final_pass_choices_precede_managed_defaults() -> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::final_pass_choices_precede_managed_defaults()
}

#[test]
fn separate_config_roots_do_not_inherit_the_previous_host_condition()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::separate_config_roots_do_not_inherit_the_previous_host_condition()
}

#[test]
fn context_changes_for_config_includes_keys_and_agent_but_not_repository()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::context_changes_for_config_includes_keys_and_agent_but_not_repository()
}

#[test]
fn relative_identity_files_are_fingerprinted_relative_to_process_cwd()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::relative_identity_files_are_fingerprinted_relative_to_process_cwd()
}

#[test]
fn passwd_home_is_used_even_when_home_environment_is_overridden()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::passwd_home_is_used_even_when_home_environment_is_overridden()
}

#[test]
fn channel_start_gate_serializes_starts_and_failure_releases_waiters()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::channel_start_gate_serializes_starts_and_failure_releases_waiters()
}

#[test]
fn dynamic_conditions_and_unresolved_security_tokens_disable_reuse()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::dynamic_conditions_and_unresolved_security_tokens_disable_reuse()
}

#[test]
fn nested_include_paths_keep_the_original_user_or_system_base()
-> Result<(), Box<dyn std::error::Error>> {
    super::security::tests::nested_include_paths_keep_the_original_user_or_system_base()
}

#[test]
fn root_config_filenames_are_never_reinterpreted_as_include_tokens() {
    super::security::tests::root_config_filenames_are_never_reinterpreted_as_include_tokens()
}

#[test]
fn config_path_newlines_are_rejected() {
    super::security::tests::config_path_newlines_are_rejected()
}
