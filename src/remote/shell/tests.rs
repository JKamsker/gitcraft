use super::*;
use crate::remote::session::{Request, framed};

fn local_shell() -> Result<Shell, String> {
    Shell::start(Command::new("sh"), "test")
}

fn run_in(shell: &mut Shell, req: &Request) -> Result<Output, String> {
    let nonce = nonce();
    shell
        .run(
            &framed(req, &nonce)?,
            &nonce,
            Instant::now() + Duration::from_secs(30),
        )
        .map_err(Failure::message)
}

/// The framing runs as it is in a real `sh`, binary output and input included.
#[test]
fn a_framed_request_runs_in_sh() -> Result<(), String> {
    let mut shell = local_shell()?;
    let req = Request {
        script: "cat; printf 'err' >&2; exit 3".to_owned(),
        stdin: Some(vec![0, 1, 2, b'\n', 255]),
        ..Request::default()
    };
    let out = run_in(&mut shell, &req)?;
    assert_eq!(out.code, 3);
    assert_eq!(out.stdout, vec![0, 1, 2, b'\n', 255]);
    assert_eq!(out.stderr, b"err");
    let out = run_in(
        &mut shell,
        &Request {
            script: "printf hi".to_owned(),
            ..Request::default()
        },
    )?;
    assert_eq!((out.code, out.stdout), (0, b"hi".to_vec()));
    Ok(())
}

/// Something a command leaves writing to its output after it ends doesn't spill into the
/// next command's answer.
#[test]
fn output_written_late_stays_out_of_the_next_answer() -> Result<(), String> {
    let mut shell = local_shell()?;
    let late = Request {
        script: "printf a; (sleep 0.2; printf bbbbbbbb) &".to_owned(),
        ..Request::default()
    };
    assert!(run_in(&mut shell, &late)?.stdout.starts_with(b"a"));
    std::thread::sleep(Duration::from_millis(400));
    let next = Request {
        script: "printf next".to_owned(),
        ..Request::default()
    };
    assert_eq!(run_in(&mut shell, &next)?.stdout, b"next");
    Ok(())
}

/// A folder that isn't there says so in words the caller knows.
#[test]
fn a_missing_folder_is_told_apart() -> Result<(), String> {
    let mut shell = local_shell()?;
    let req = Request {
        cwd: Some("/no/such/folder".to_owned()),
        script: "true".to_owned(),
        ..Request::default()
    };
    let out = run_in(&mut shell, &req)?;
    assert_eq!(out.code, 1);
    assert!(crate::remote::session::is_missing_folder(&out.stderr));
    Ok(())
}

/// The actual owner handles a timeout, including a request with no output.
#[test]
fn a_command_that_never_finishes_times_out() -> Result<(), String> {
    let mut shell = local_shell()?;
    let nonce = nonce();
    let request = Request {
        script: "sleep 5".into(),
        ..Default::default()
    };
    let started = Instant::now();
    let ran = shell.run(
        &framed(&request, &nonce)?,
        &nonce,
        Instant::now() + Duration::from_millis(300),
    );
    assert!(matches!(ran, Err(Failure::TimedOut(true))));
    assert!(started.elapsed() < Duration::from_secs(6));
    Ok(())
}

/// A shell idle a while is checked before it's used, and one that ended is not used.
#[test]
fn idle_shells_are_checked() -> Result<(), String> {
    let mut shell = local_shell()?;
    shell.used = Instant::now() - CHECK_AFTER - Duration::from_secs(1);
    assert!(shell.usable());
    shell.stdin.take();
    let _ = shell.child.finish();
    assert!(!shell.usable());
    Ok(())
}
