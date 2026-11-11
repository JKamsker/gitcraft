//! Native helper controller: record publication, dispatch, cancellation and terminal proof.
use super::*;

pub(super) fn private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    gitcraft_os::close_file(file).map_err(|e| e.to_string())
}
pub(super) fn emit_hello(bytes: &[u8]) -> Result<(), String> {
    println!("\n{READY}");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    std::io::stderr()
        .write_all(MARKER)
        .and_then(|()| std::io::stderr().write_all(bytes))
        .and_then(|()| std::io::stderr().write_all(b"\n"))
        .and_then(|()| std::io::stderr().flush())
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub(super) type TerminalParts = (i32, bool, Vec<u8>, Vec<u8>);
pub(super) fn terminal_parts(
    result: Result<crate::processes::Output, String>,
) -> Result<TerminalParts, String> {
    use crate::processes::{Failure, StopKind};
    match crate::processes::checked(result) {
        Err(Failure::CancelledBeforeStart | Failure::TimedOutBeforeStart) => {
            Ok((125, false, Vec::new(), Vec::new()))
        }
        Err(error) => Err(error.message()),
        Ok(result) => {
            if result.stop_kind == Some(StopKind::Uncertain) {
                return Err("native cleanup unconfirmed".into());
            }
            Ok((
                result.status.code().unwrap_or(125),
                result.stopped.is_none(),
                result.stdout,
                result.stderr,
            ))
        }
    }
}
pub(super) fn terminal_receipt(
    path: &Path,
    endpoint: &Endpoint,
    code: i32,
    complete: bool,
) -> Result<Vec<u8>, String> {
    let receipt = Receipt {
        endpoint: endpoint.clone(),
        state: "stopped".into(),
        code: i64::from(code),
        complete,
    };
    let mut bytes = serde_json::to_vec(&receipt).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    private_file(&path.join("terminal.new"), &bytes)?;
    std::fs::rename(path.join("terminal.new"), path.join("terminal")).map_err(|e| e.to_string())?;
    Ok(bytes)
}
pub(super) fn helper(fence: &mut Option<gitcraft_os::WorkspaceFence>) -> Result<i32, String> {
    gitcraft_os::detach_controller().map_err(|e| e.to_string())?;
    let mut controller = BufReader::new(std::io::stdin());
    let mut line = Vec::new();
    controller
        .by_ref()
        .take(MAX_REQUEST as u64 + 1)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if line.len() > MAX_REQUEST || !line.ends_with(b"\n") {
        return Err("invalid native request".into());
    }
    let request: Request = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
    if !hex(&request.nonce, 64)
        || request.milliseconds == 0
        || request.milliseconds > 7 * 86400 * 1000
    {
        return Err("invalid native request identity/deadline".into());
    }
    if request.cleanup
        && !request
            .workspace
            .as_ref()
            .is_some_and(|a| a.mode == super::super::workspace_authority::FenceMode::Mutation)
    {
        return Err("cleanup requires full actual-machine mutation authority".into());
    }
    let cancel = crate::tasks::Cancellation::default();
    let eof = cancel.clone();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 256];
        loop {
            match controller.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    eof.cancel();
                    break;
                }
                Ok(_) => {}
            }
        }
    });
    let started = Instant::now();
    *fence = request
        .workspace
        .as_ref()
        .map(|authority| {
            authority.record_name(&request.record)?;
            gitcraft_os::WorkspaceFence::acquire(&authority.native_grant(), || {
                !cancel.is_cancelled()
                    && started.elapsed() < Duration::from_millis(request.milliseconds)
            })
            .map_err(|e| e.to_string())
        })
        .transpose()?;
    let mut command = Command::new(request.program.restore()?);
    for word in &request.args {
        command.arg(word.restore()?);
    }
    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd.restore()?);
    }
    for (key, value) in &request.env {
        let key = key.restore()?;
        if let Some(value) = value {
            command.env(key, value.restore()?);
        } else {
            command.env_remove(key);
        }
    }
    command.env_remove("GITCRAFT_NATIVE_HELPER");
    let path = std::env::temp_dir().join(format!("{PREFIX}{}", random_hex(16)?));
    gitcraft_os::create_private_directory(&path).map_err(|e| e.to_string())?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let endpoint = Endpoint {
        v: 2,
        nonce: request.nonce.clone(),
        path: path.clone(),
        port: listener.local_addr().map_err(|e| e.to_string())?.port(),
        token: random_hex(32)?,
    };
    let bytes = serde_json::to_vec(&endpoint).map_err(|e| e.to_string())?;
    // Exclusive record creation precedes target submission. Workspace created its private
    // directory, with owner-only inherited ACLs on Windows and mode 0700 on Unix.
    if let Some(authority) = &request.workspace {
        let name = authority.record_name(&request.record)?;
        let bound = super::super::workspace_authority::record(&bytes, authority)?;
        fence
            .as_ref()
            .ok_or("missing actual workspace fence")?
            .publish_record(name, &bound)
            .map_err(|e| e.to_string())?;
    } else {
        private_file(&request.record, &bytes)?;
    }
    if let Some(directory) = &request.directory {
        let authority = request
            .workspace
            .as_ref()
            .ok_or("private directory creation requires workspace authority")?;
        if authority.mode != super::super::workspace_authority::FenceMode::Mutation
            || authority
                .root
                .parent()
                .map(|p| p.join("gitcraft-bisect-work"))
                .as_ref()
                != Some(directory)
        {
            return Err("private directory is outside claimed workspace".into());
        }
        let result = (|| {
            emit_hello(&bytes)?;
            if cancel.is_cancelled()
                || started.elapsed() >= Duration::from_millis(request.milliseconds)
            {
                return Err("private directory creation cancelled before mutation".to_owned());
            }
            fence
                .as_ref()
                .ok_or("missing directory fence")?
                .verify(&authority.native_grant())
                .map_err(|e| e.to_string())?;
            gitcraft_os::create_private_directory(directory).map_err(|e| e.to_string())
        })();
        terminal_receipt(
            &path,
            &endpoint,
            if result.is_ok() { 0 } else { 125 },
            result.is_ok(),
        )?;
        return result.map(|()| 0);
    }
    let mut cleanup_plan = None;
    let submitted = (|| {
        emit_hello(&bytes)?;
        let input = if request.cleanup {
            let authority = request
                .workspace
                .as_ref()
                .ok_or("cleanup authority missing")?;
            let plan = super::super::workspace_ledger::NativePlan::prepare(
                authority,
                &request.record,
                || {
                    !cancel.is_cancelled()
                        && started.elapsed() < Duration::from_millis(request.milliseconds)
                },
            )?;
            let input = plan.shell_proof();
            cleanup_plan = Some(plan);
            Some(input)
        } else {
            use base64::Engine;
            request
                .input
                .as_ref()
                .map(|value| {
                    if value.len() > (64usize << 20).div_ceil(3) * 4 {
                        return Err("oversized native input".to_owned());
                    }
                    base64::engine::general_purpose::STANDARD
                        .decode(value)
                        .map_err(|e| e.to_string())
                })
                .transpose()?
        };
        crate::processes::service()?.submit(
            command,
            crate::processes::Options {
                cancel: cancel.clone(),
                input,
                timeout: Some(
                    Duration::from_millis(request.milliseconds).saturating_sub(started.elapsed()),
                ),
                ..Default::default()
            },
        )
    })();
    let ticket = match submitted {
        Ok(ticket) => ticket,
        Err(error) => {
            // No successful submit: there is no target or future queued dispatch. Preserve
            // an authenticated NotDispatched terminal proof when the record was published.
            if let Err(proof) = terminal_receipt(&path, &endpoint, 125, false) {
                return Err(format!(
                    "{error}; NotDispatched proof publication failed: {proof}"
                ));
            }
            return Err(error);
        }
    };
    if request
        .workspace
        .as_ref()
        .is_some_and(|a| a.mode == super::super::workspace_authority::FenceMode::Dispatch)
    {
        // The actual helper and cancellation token are live, the durable record is complete,
        // and the only pending ticket is now sticky-cancelable before releasing this fence.
        drop(fence.take());
    }
    let mut clients = Vec::new();
    let mut pending_clients: Vec<(TcpStream, Vec<u8>, Instant)> = Vec::new();
    let start = Instant::now();
    let result = loop {
        if let Some(result) = ticket.try_result() {
            break terminal_parts(result)?;
        }
        if start.elapsed() > Duration::from_millis(request.milliseconds) + Duration::from_secs(10) {
            cancel.cancel();
        }
        match listener.accept() {
            Ok((socket, _)) => {
                if pending_clients.len() < 4 && clients.len() < 4 {
                    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
                    pending_clients.push((
                        socket,
                        Vec::new(),
                        Instant::now() + Duration::from_millis(100),
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
        let mut index = 0;
        while index < pending_clients.len() {
            let (socket, bytes, deadline) = &mut pending_clients[index];
            let mut done = Instant::now() >= *deadline;
            let mut buffer = [0u8; 256];
            if !done {
                match socket.read(&mut buffer) {
                    Ok(0) => done = true,
                    Ok(n) => {
                        bytes.extend_from_slice(&buffer[..n]);
                        done = bytes.len() > 2048 || bytes.contains(&b'\n');
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => done = true,
                }
            }
            if done {
                let (socket, bytes, deadline) = pending_clients.swap_remove(index);
                if Instant::now() < deadline
                    && Endpoint::from_record(&bytes).is_ok_and(|other| other == endpoint)
                {
                    cancel.cancel();
                    if clients.len() < 4 {
                        clients.push(socket);
                    }
                }
            } else {
                index += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (mut code, mut complete, mut stdout, mut stderr) = result;
    if let Some(plan) = cleanup_plan
        && super::super::workspace_ledger::cleanup_complete_allowed(code, complete)
    {
        match plan.complete() {
            Ok(proof) => stdout = proof,
            Err(error) => {
                code = 125;
                complete = false;
                stderr.extend_from_slice(
                    format!("cleanup partial; external ledger retained: {error}").as_bytes(),
                );
            }
        }
    }
    let bytes = terminal_receipt(&path, &endpoint, code, complete)?;
    // Mutation authority remains in this actual owner until target cleanup and durable
    // terminal proof; no parent-side lock lifetime is substituted for it.
    drop(fence.take());
    for mut client in clients {
        let deadline = Instant::now() + Duration::from_millis(100);
        let _ = super::super::deadline_io::write_frame(
            &mut client,
            &bytes,
            || Instant::now() >= deadline,
            || std::thread::sleep(Duration::from_millis(2)),
        );
    }
    std::io::stdout()
        .write_all(&stdout)
        .and_then(|()| std::io::stderr().write_all(&stderr))
        .map_err(|e| e.to_string())?;
    Ok(code)
}
