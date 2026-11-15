use super::*;
#[test]
fn growing_reader_cannot_exceed_limit_and_checks_each_chunk() -> Result<(), String> {
    let error = chunks(std::io::repeat(0), 12)
        .err()
        .ok_or("expected limit")?;
    assert_eq!(error, budget());
    assert_eq!(chunks(std::io::Cursor::new(b"exact"), 5)?, b"exact");
    struct NeverRead;
    impl Read for NeverRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("an expired operation must not read")
        }
    }
    let reader = NeverRead;
    crate::processes::deadline::within(std::time::Instant::now(), || {
        assert!(chunks(reader, 12).is_err());
    });
    Ok(())
}
#[test]
fn opened_regular_handle_survives_replacement_and_metadata_limits_apply() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join("index");
    assert!(read(&path)?.is_none());
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    assert!(read(&path).is_err());
    std::fs::remove_dir(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, b"original").map_err(|e| e.to_string())?;
    let mut handle = File::open(&path).map_err(|e| e.to_string())?;
    // Portable replacement: release the handle before rename on Windows; Unix
    // specifically proves the bounded reader pins the already-open inode.
    #[cfg(unix)]
    {
        std::fs::rename(&path, path.with_extension("old")).map_err(|e| e.to_string())?;
        std::fs::write(&path, b"replacement").map_err(|e| e.to_string())?;
    }
    assert_eq!(chunks(&mut handle, 32)?, b"original");
    drop(handle);
    let file = File::options()
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.set_len(LIMIT + 1).map_err(|e| e.to_string())?;
    drop(file);
    assert_eq!(read(&path).err(), Some(budget()));
    Ok(())
}
#[cfg(unix)]
#[test]
fn fifo_and_symlink_are_refused_without_waiting_for_a_writer() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let fifo = dir.path().join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(read(&fifo).err(), Some(regular()));
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&fifo, &link).map_err(|e| e.to_string())?;
    assert_eq!(read(&link).err(), Some(regular()));
    Ok(())
}

#[test]
fn cancellation_between_chunks_stops_further_reads() {
    struct CancelOnRead(crate::tasks::Cancellation, usize);
    impl Read for CancelOnRead {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.1 += 1;
            assert_eq!(self.1, 1, "cancelled reader was polled again");
            bytes[0] = 1;
            self.0.cancel();
            Ok(1)
        }
    }
    let cancel = crate::tasks::Cancellation::default();
    let reader = CancelOnRead(cancel.clone(), 0);
    crate::tasks::with_cancellation(cancel, || assert!(chunks(reader, 64).is_err()));
}
