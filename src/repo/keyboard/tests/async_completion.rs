// Included in the parent test module to preserve qualified test names.

#[test]
fn delayed_patch_completion_cannot_apply_during_a_real_full_reload() -> Result<(), String> {
    let mut t = Test::new("callback-loading")?;
    let (saved, prepared) = delay_real_hunk_preparation(&mut t)?;
    let read = t.tab.work_read;
    let epoch = t.tab.work_epoch;
    t.tab.reload(&t.ctx);
    assert!(t.tab.loading);
    assert_eq!(
        t.tab.work_read, read,
        "full reads retain the displayed status while pending"
    );
    assert_eq!(t.tab.work_pending, 0);
    assert_ne!(
        t.tab.work_epoch, epoch,
        "ownership must expire at full reload start"
    );
    t.tab.picked_prepared(prepared, Ok(saved), &t.ctx);
    assert!(t.tab.busy_op.is_none());
    assert!(
        t.git(&["diff", "--cached"])?.is_empty(),
        "a retained main-branch patch must never enter the other branch's index"
    );
    assert!(
        matches!(t.tab.status.as_deref(), Some((message, true)) if message.contains("Changes reloaded"))
    );
    t.wait()?;
    assert_eq!(t.git(&["branch", "--show-current"])?.trim(), "other");
    assert!(t.git(&["diff", "--cached"])?.is_empty());
    Ok(())
}

#[test]
fn delayed_patch_completion_cannot_apply_after_an_actual_failed_full_reload() -> Result<(), String>
{
    let mut t = Test::new("callback-read-error")?;
    let (saved, prepared) = delay_real_hunk_preparation(&mut t)?;
    let read = t.tab.work_read;
    let epoch = t.tab.work_epoch;
    // Fail the real reload worker with invalid config while retaining the same repository,
    // then restore it before delivering the saved patch. Git could apply it successfully.
    let config = t.path.join(".git/config");
    let original_config = std::fs::read(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config, "[incomplete\n").map_err(|e| e.to_string())?;
    t.tab.reload(&t.ctx);
    let deadline = Instant::now() + Duration::from_secs(10);
    while t.tab.loading {
        t.tab.poll(&t.ctx);
        if Instant::now() >= deadline {
            let _ = std::fs::write(&config, &original_config);
            return Err("The expected failed reload did not finish".to_owned());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    std::fs::write(&config, &original_config).map_err(|e| e.to_string())?;
    assert!(t.tab.load_error.is_some());
    assert_eq!(
        t.tab.work_read, read,
        "a failed full read retains the old status"
    );
    assert_eq!(t.tab.work_pending, 0);
    assert_ne!(t.tab.work_epoch, epoch);
    t.git(&["apply", "--cached", "--check", &saved])?;
    t.tab.picked_prepared(prepared, Ok(saved), &t.ctx);
    assert!(t.tab.busy_op.is_none());
    assert!(
        t.git(&["diff", "--cached"])?.is_empty(),
        "failed reloads must not revive retained patch ownership"
    );
    assert!(
        matches!(t.tab.status.as_deref(), Some((message, true)) if message.contains("Changes reloaded"))
    );
    Ok(())
}

#[test]
fn session_transfer_rejects_delayed_patch_completion_without_changing_the_index()
-> Result<(), String> {
    let mut t = Test::new("transfer-callback")?;
    let (saved, prepared) = delay_real_hunk_preparation(&mut t)?;
    t.tab.cancel_keyboard_ownership(&t.ctx);
    t.tab.picked_prepared(prepared, Ok(saved), &t.ctx);
    assert!(t.tab.busy_op.is_none());
    assert!(t.git(&["diff", "--cached"])?.is_empty());
    assert!(
        matches!(t.tab.status.as_deref(), Some((message, true)) if message.contains("Changes reloaded"))
    );
    Ok(())
}
