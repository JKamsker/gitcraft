use super::*;
#[cfg(unix)]
#[test]
#[ignore = "runtime hold: exact source-reviewed invocation only"]
fn owned_runtime_claim_fence_turnover() -> Result<(), Box<dyn std::error::Error>> {
    use super::super::{native_owner, owned_runtime as fixture, workspace_authority};
    // The existing native Lease API has a singleton, dedicated Service lane. This
    // exact test owns that lane; no other test may run in this invocation.
    struct LeaseLane;
    impl Drop for LeaseLane {
        fn drop(&mut self) {
            processes::shutdown_leases();
        }
    }
    struct PendingClaim {
        cancel: Cancellation,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for PendingClaim {
        fn drop(&mut self) {
            self.cancel.cancel();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
    let mut scratch = fixture::Scratch::new()?;
    let _leases = LeaseLane;
    let service = processes::owned_fixture_service()?;
    let canary = fixture::canary(&service, &scratch.0)?;
    fixture::challenge(&canary, &scratch.0, b"before-claim")?;
    let root = scratch.0.join("ledger");
    let mut winner = Lease::native(&root)?;
    let authority = winner.authority(FenceMode::Mutation)?;
    let generation = std::fs::read(root.join("generation"))?;
    assert_eq!(generation, format!("{}\n", authority.generation).as_bytes());
    verify_busy_namespace(&root, &winner, &authority, &generation)?;

    let operation = scratch.0.join("operation");
    std::fs::create_dir(&operation)?;
    let name = format!("{}-mutation-{}.owner", authority.generation, "a".repeat(32));
    let record = root.join(&name);
    let native = native_owner::command_with_authority(
        fixture::command("gate", &operation)?,
        &record,
        fixture::LIMIT,
        Some(authority.clone()),
    )?;
    let mut options = fixture::options();
    let command = native.configure(&mut options);
    let mutation = service.submit(command, options)?;
    fixture::until(|| Ok(operation.join("target-ready").exists()))?;
    let owner_bytes = std::fs::read(&record)?;
    let endpoint_bytes =
        workspace_authority::bound_endpoint(&name, &owner_bytes, &authority.anchor)?;
    let endpoint = native_owner::Endpoint::from_record(&endpoint_bytes)?;
    assert!(endpoint.terminal().is_err());
    assert!(!operation.join("mutation-effect").exists());
    // Cancellation of this retained controller closes only its owned input. The
    // claim helper sees EOF and exits; no numeric signal or target stop is used.
    let lease_ticket = winner.ticket.take().ok_or("winner ticket missing")?;
    lease_ticket.cancel();
    let released = fixture::wait(&lease_ticket)?;
    assert_eq!(released.stop_kind, Some(processes::StopKind::Cancelled));
    assert_eq!(released.status.code(), Some(0));
    assert!(winner.authority(FenceMode::Mutation).is_err());
    drop(winner);

    let cancel = Cancellation::default();
    let child_cancel = cancel.clone();
    let child_root = root.clone();
    let (send, receive) = mpsc::sync_channel(1);
    let pending = PendingClaim {
        cancel,
        thread: Some(
            std::thread::Builder::new()
                .name("finite-claim-turnover".into())
                .spawn(move || {
                    let result = crate::tasks::with_cancellation(child_cancel, || {
                        Lease::native(&child_root)
                    });
                    let _ = send.send(result);
                })?,
        ),
    };
    // The old helper has exited. A busy primary now proves the new actual helper
    // owns primary and is waiting behind the mutation owner's kernel fence.
    let primary = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("primary"))?;
    fixture::until(|| match primary.try_lock() {
        Ok(()) => {
            primary.unlock().map_err(|e| e.to_string())?;
            Ok(false)
        }
        Err(std::fs::TryLockError::WouldBlock) => Ok(true),
        Err(std::fs::TryLockError::Error(error)) => Err(error.to_string()),
    })?;
    assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(std::fs::read(root.join("generation"))?, generation);
    assert_eq!(std::fs::read(&record)?, owner_bytes);
    assert!(endpoint.terminal().is_err());
    fixture::challenge(&canary, &scratch.0, b"fence-retained-after-eof")?;

    std::fs::write(operation.join("release"), b"release owned mutation")?;
    let receipt = verify_mutation(&mutation, &operation, &endpoint)?;
    let mut successor = receive.recv_timeout(fixture::LIMIT)??;
    let new_authority = successor.authority(FenceMode::Mutation)?;
    assert_ne!(new_authority.generation, authority.generation);
    assert_eq!(new_authority.anchor, authority.anchor);
    assert_eq!(
        std::fs::read(root.join("generation"))?,
        format!("{}\n", new_authority.generation).as_bytes()
    );
    // Real WorkspaceFence/Grant rejects old authority before any publication.
    let deadline = Instant::now() + fixture::LIMIT;
    assert!(
        gitcraft_os::WorkspaceFence::acquire(&authority.native_grant(), || Instant::now()
            < deadline)
        .is_err()
    );
    let stale_directory = scratch.0.join("stale");
    std::fs::create_dir(&stale_directory)?;
    let stale_record = root.join(format!(
        "{}-mutation-{}.owner",
        authority.generation,
        "b".repeat(32)
    ));
    let stale = native_owner::command_with_authority(
        fixture::command("touch", &stale_directory)?,
        &stale_record,
        fixture::LIMIT,
        Some(authority),
    )?;
    let mut options = fixture::options();
    let command = stale.configure(&mut options);
    let refused = service.submit(command, options)?;
    let refused = fixture::wait(&refused)?;
    assert_eq!(refused.status.code(), Some(125));
    assert_eq!(refused.stop_kind, Some(processes::StopKind::Uncertain));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("stale workspace generation"));
    assert!(!stale_record.exists());
    assert!(!stale_directory.join("unexpected-effect").exists());
    assert_eq!(std::fs::read(&record)?, owner_bytes);
    assert_eq!(endpoint.terminal()?, receipt);
    assert_eq!(super::super::recover_owner(&record)?, owner_bytes);
    let fence = gitcraft_os::WorkspaceFence::acquire(&new_authority.native_grant(), || {
        Instant::now() < deadline
    })?;
    fence.verify(&new_authority.native_grant())?;
    drop(fence);
    endpoint.retire_proven(&receipt)?;
    endpoint.retire_proven(&receipt)?;
    fixture::challenge(&canary, &scratch.0, b"after-turnover-and-recovery")?;
    successor.close()?;
    drop(pending);
    canary.cancel();
    assert_eq!(
        fixture::wait(&canary)?.stop_kind,
        Some(processes::StopKind::Cancelled)
    );
    drop(service);
    processes::shutdown_leases();
    scratch.complete();
    Ok(())
}
#[test]
fn helper_entrypoint() {
    if std::env::var_os("GITCRAFT_CLAIM_HELPER").is_some() {
        std::process::exit(entrypoint());
    }
}
#[test]
fn recording_claim_acknowledgement_requires_exact_generation_and_complete_bounded_frame()
-> Result<(), Box<dyn std::error::Error>> {
    let generation = "a".repeat(64);
    let reply = Reply {
        root: "/synthetic/repo/ledger".into(),
        anchor: "unix:1:2".into(),
        generation: generation.clone(),
    };
    let mut frame = MARKER.to_vec();
    frame.extend(serde_json::to_vec(&reply)?);
    frame.push(b'\n');
    for split in 0..frame.len() {
        let mut parser = Frame::default();
        assert!(parser.feed(&frame[..split], &generation).is_none());
        assert!(parser.feed(&frame[split..], &generation).is_some());
        assert!(parser.feed(&frame, &generation).is_none());
    }
    assert!(Frame::default().feed(&frame, &"b".repeat(64)).is_none());
    assert!(
        Frame::default()
            .feed(&vec![b'x'; 4097], &generation)
            .is_none()
    );
    Ok(())
}

#[cfg(unix)]
fn verify_busy_namespace(
    root: &Path,
    winner: &Lease,
    authority: &WorkspaceAuthority,
    generation: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let namespace_before: std::collections::BTreeSet<_> = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    // Acquisition fails before exposing authority: the loser cannot publish, clear
    // or dispatch through a Lease, and cannot change the winner's namespace.
    let loser = Lease::native(root);
    assert!(loser.is_err());
    let error = loser.err().ok_or("busy claimant unexpectedly acquired")?;
    assert!(error.contains("another workspace claimant"), "{error}");
    assert_eq!(std::fs::read(root.join("generation"))?, generation);
    let namespace_after: std::collections::BTreeSet<_> = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    assert_eq!(namespace_after, namespace_before);
    assert_eq!(
        winner.authority(FenceMode::Mutation)?.generation,
        authority.generation
    );

    Ok(())
}

#[cfg(unix)]
fn verify_mutation(
    mutation: &processes::Ticket,
    operation: &Path,
    endpoint: &super::super::native_owner::Endpoint,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use super::super::owned_runtime as fixture;
    let output = fixture::wait(mutation)?;
    assert!(output.stopped.is_none(), "{:?}", output.stopped);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, fixture::stdout(b"mutation-complete\n"));
    assert!(output.stderr.is_empty());
    assert_eq!(
        std::fs::read(operation.join("mutation-effect"))?,
        b"owned mutation"
    );
    let receipt = endpoint.terminal()?;
    assert!(endpoint.confirms(&receipt));
    let proof: serde_json::Value = serde_json::from_slice(&receipt)?;
    assert_eq!(proof["code"], 0);
    assert_eq!(proof["complete"], true);
    Ok(receipt)
}
