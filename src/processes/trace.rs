//! Opt-in libtest diagnostics. Never log command arguments, paths, environment or input.
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

pub(super) struct Trace {
    test: String,
    command: &'static str,
    started: Instant,
    stage: AtomicUsize,
    pid: AtomicUsize,
    pipes: AtomicUsize,
    written: AtomicUsize,
    stdout: AtomicUsize,
    stderr: AtomicUsize,
}

const STAGES: [&str; 7] = [
    "queued",
    "spawning",
    "supervising",
    "running",
    "stopping",
    "reaping",
    "draining",
];

#[derive(Clone, Copy)]
#[repr(usize)]
pub(super) enum Stage {
    Queued,
    Spawning,
    Supervising,
    Running,
    Stopping,
    Reaping,
    Draining,
}

impl Trace {
    pub(super) fn new(command: &Command) -> Option<Self> {
        if std::env::var_os("GITCRAFT_TEST_PROCESS_TRACE").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return None;
        }
        Some(Self {
            test: std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
                .take(200)
                .collect(),
            command: label(command),
            started: Instant::now(),
            stage: AtomicUsize::new(Stage::Queued as usize),
            pid: AtomicUsize::new(0),
            pipes: AtomicUsize::new(0),
            written: AtomicUsize::new(0),
            stdout: AtomicUsize::new(0),
            stderr: AtomicUsize::new(0),
        })
    }
    pub(super) fn stage(&self, stage: Stage) {
        self.stage.store(stage as usize, Relaxed);
    }
    pub(super) fn spawned(&self, pid: Option<u32>) {
        self.pid.store(pid.unwrap_or(0) as usize, Relaxed);
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn io(
        &self,
        input: bool,
        out: bool,
        err: bool,
        exited: bool,
        written: usize,
        stdout: usize,
        stderr: usize,
    ) {
        self.pipes.store(
            usize::from(input)
                | (usize::from(out) << 1)
                | (usize::from(err) << 2)
                | (usize::from(exited) << 3),
            Relaxed,
        );
        self.written.store(written, Relaxed);
        self.stdout.store(stdout, Relaxed);
        self.stderr.store(stderr, Relaxed);
    }
    fn snapshot(&self) -> String {
        let pipes = self.pipes.load(Relaxed);
        format!(
            "process wait: test={} command={} elapsed={}s stage={} pid={} stdin_open={} stdout_open={} stderr_open={} exited={} accepted_input={} captured_stdout={} captured_stderr={}",
            self.test,
            self.command,
            self.started.elapsed().as_secs(),
            STAGES[self.stage.load(Relaxed)],
            self.pid.load(Relaxed),
            pipes & 1 != 0,
            pipes & 2 != 0,
            pipes & 4 != 0,
            pipes & 8 != 0,
            self.written.load(Relaxed),
            self.stdout.load(Relaxed),
            self.stderr.load(Relaxed)
        )
    }
    pub(super) fn report(&self) {
        eprintln!("{}", self.snapshot());
    }
}

// An allowlisted Git verb is enough to find the call site without exposing arguments.
fn label(command: &Command) -> &'static str {
    let program = std::path::Path::new(command.get_program())
        .file_name()
        .and_then(|s| s.to_str());
    if !matches!(program, Some("git" | "git.exe")) {
        return "other";
    }
    const VERBS: [&str; 25] = [
        "init",
        "config",
        "add",
        "commit",
        "rev-parse",
        "status",
        "diff",
        "log",
        "show",
        "branch",
        "checkout",
        "switch",
        "merge",
        "rebase",
        "reset",
        "update-ref",
        "for-each-ref",
        "cat-file",
        "hash-object",
        "write-tree",
        "commit-tree",
        "reflog",
        "bisect",
        "bundle",
        "fetch",
    ];
    let mut args = command.get_args();
    while let Some(arg) = args.next() {
        if arg == "-c" || arg == "-C" {
            let _ = args.next();
            continue;
        }
        let Some(arg) = arg.to_str() else {
            return "git:other";
        };
        if arg.starts_with('-') {
            continue;
        }
        return VERBS
            .into_iter()
            .find(|&verb| verb == arg)
            .unwrap_or("git:other");
    }
    "git:other"
}

#[test]
fn labels_never_include_arguments_paths_or_unknown_programs() {
    let mut command = Command::new("git");
    command.args([
        "-c",
        "secret=value",
        "-C",
        "/private/repo",
        "commit",
        "-m",
        "secret message",
    ]);
    assert_eq!(label(&command), "commit");
    assert_eq!(
        label(Command::new("git").arg("private-command")),
        "git:other"
    );
    assert_eq!(
        label(Command::new("private-program").arg("commit")),
        "other"
    );
}

#[test]
fn snapshots_record_stages_and_pipe_state_without_process_control() {
    let trace = Trace {
        test: "synthetic".into(),
        command: "commit",
        started: Instant::now(),
        stage: AtomicUsize::new(Stage::Queued as usize),
        pid: AtomicUsize::new(0),
        pipes: AtomicUsize::new(0),
        written: AtomicUsize::new(0),
        stdout: AtomicUsize::new(0),
        stderr: AtomicUsize::new(0),
    };
    assert!(trace.snapshot().contains("stage=queued"));
    trace.stage(Stage::Supervising);
    trace.spawned(Some(42));
    assert!(trace.snapshot().contains("stage=supervising pid=42"));
    trace.stage(Stage::Running);
    trace.io(true, false, true, false, 10, 20, 30);
    let snapshot = trace.snapshot();
    assert!(snapshot.contains("stdin_open=true stdout_open=false stderr_open=true exited=false"));
    assert!(snapshot.contains("accepted_input=10 captured_stdout=20 captured_stderr=30"));
}
