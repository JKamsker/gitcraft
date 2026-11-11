use super::*;

/// A diff or merge tool, allowed longer than any network operation, never takes a shell
/// fetch and push need; and, the lane full, a further one is refused at once.
#[test]
fn a_tool_runs_in_a_lane_of_its_own() {
    let req = |long, timeout| Request {
        long,
        timeout,
        ..Request::default()
    };
    assert_eq!(req(false, None).lane(), Lane::Quick);
    assert_eq!(req(true, None).lane(), Lane::Long);
    assert_eq!(req(true, Some(LONG_FOR)).lane(), Lane::Long);
    assert_eq!(req(true, Some(LONG_FOR * 24)).lane(), Lane::Tool);
    let busy = Lane::Tool.busy(&Host::Ssh("build".to_owned()));
    assert_eq!(
        busy,
        "4 diff or merge tools are already open on SSH · build: close one of them first"
    );
}

#[test]
fn nothing_runs_without_an_open_connection() {
    let host = Host::Ssh("gitcraft-never-connected.invalid".to_owned());
    let req = Request {
        script: "true".to_owned(),
        ..Request::default()
    };
    assert!(exec_if_connected(&host, &req).is_none());
}

/// Reads nobody asked for run in a shell already open, don't count as its use (so the
/// connection still closes when idle), and one that times out never opens another
/// connection, not even to stop what hung.
#[cfg(unix)]
#[test]
fn unasked_reads_never_connect_nor_keep_a_connection_alive() -> Result<(), String> {
    let host = Host::Ssh("gitcraft-unasked-test.invalid".to_owned());
    let pool = pool(&host, Lane::Quick);
    pool.add_idle(Shell::start(Command::new("sh"), "test")?);
    let (_, _, before) = pool.looks();
    std::thread::sleep(Duration::from_millis(20));
    let quick = Request {
        script: "printf ok".to_owned(),
        ..Request::default()
    };
    let out = exec_if_connected(&host, &quick).ok_or("no shell taken")??;
    assert_eq!(out.stdout, b"ok");
    let (open, failed, after) = pool.looks();
    assert_eq!((open, failed), (1, false));
    assert_eq!(after, before, "the read counted as use");
    // Hangs past its limit: the shell is closed, and nothing connects.
    let slow = Request {
        script: "sleep 5".to_owned(),
        timeout: Some(Duration::from_millis(300)),
        ..Request::default()
    };
    let timed_out = exec_if_connected(&host, &slow).ok_or("no shell taken")?;
    assert!(timed_out.is_err_and(|e| e.contains("didn't answer in time")));
    std::thread::sleep(Duration::from_millis(300));
    let (open, failed, _) = pool.looks();
    assert_eq!((open, failed), (0, false), "a connection was tried");
    assert!(exec_if_connected(&host, &quick).is_none());
    Ok(())
}

#[test]
fn headers_read_with_padded_numbers_for_their_command_only() {
    assert_eq!(parse_header("R ab1 0 12 0\n", "ab1"), Some((0, 12, 0)));
    assert_eq!(
        parse_header("R ab1 128      7       3\n", "ab1"),
        Some((128, 7, 3))
    );
    assert_eq!(parse_header("R ab2 0 1 2\n", "ab1"), None);
    assert_eq!(parse_header("X ab1 0 1 2\n", "ab1"), None);
    assert_eq!(parse_header("R ab1 0 1\n", "ab1"), None);
    // A size no output has.
    assert_eq!(parse_header("R ab1 0 99999999999 0\n", "ab1"), None);
}

#[test]
fn a_request_sets_its_folder_and_variables_in_a_subshell() {
    let req = Request {
        cwd: Some("~/my repo".to_owned()),
        script: "exec git status".to_owned(),
        env: vec![
            ("A".to_owned(), "it's".to_owned()),
            ("X;rm -rf ~".to_owned(), "1".to_owned()),
        ],
        unset: vec!["B".to_owned(), "$(x)".to_owned()],
        ..Request::default()
    };
    let s = inline(&req, "</dev/null");
    assert_eq!(
        s,
        "(cd -- \"$HOME\"'/my repo' 2>/dev/null || { printf 'gitcraft: no such folder: %s\\n' \"$HOME\"'/my repo' >&2; exit 1; }\nunset B\nA='it'\\''s'; export A\nexec git status\n) </dev/null"
    );
}

#[test]
fn a_framed_request_has_one_complete_json_line() -> Result<(), String> {
    let request = framed(
        &Request {
            script: "echo x\necho y".into(),
            stdin: Some(vec![0, 255, b'\n']),
            ..Default::default()
        },
        "ab1",
    )?;
    assert_eq!(request.bytes().filter(|&b| b == b'\n').count(), 1);
    let value: serde_json::Value = serde_json::from_str(&request).map_err(|e| e.to_string())?;
    assert_eq!(value["nonce"], "ab1");
    assert!(
        value["script"]
            .as_str()
            .is_some_and(|s| s.contains("echo x\necho y"))
    );
    Ok(())
}

#[test]
fn a_ready_supervisor_never_grants_pid_authority() {
    assert!(parse_ready(b"__GITCRAFT_READY__\n"));
    assert!(!parse_ready(b"__GITCRAFT_READY__ 4242\n"));
    assert!(!parse_ready(b"login banner\n"));
}

/// This production timeout must acknowledge the owner; no process-list assertion.
#[cfg(unix)]
#[test]
fn stopping_ends_a_whole_tree() -> Result<(), String> {
    let effect = std::env::temp_dir().join(format!("gitcraft-owned-stop-{}", nonce()));
    let request = Request {
        script: format!(
            "sh -c {}",
            sh_quote(&format!(
                "sleep 1; printf survived >{}",
                sh_quote(&effect.to_string_lossy())
            ))
        ),
        timeout: Some(Duration::from_millis(200)),
        ..Default::default()
    };
    let result = exec(&Host::Ssh(LOOPBACK.into()), &request);
    assert!(result.is_err_and(|e| e.contains("confirmed stop and reap")));
    std::thread::sleep(Duration::from_secs(1));
    assert!(!effect.exists(), "a child survived cancellation");
    Ok(())
}

#[test]
fn a_request_shows_no_values() {
    let req = Request {
        env: vec![("PASSWORD".to_owned(), "hunter2".to_owned())],
        ..Request::default()
    };
    let shown = format!("{req:?}");
    assert!(
        shown.contains("PASSWORD") && !shown.contains("hunter2"),
        "{shown}"
    );
}
