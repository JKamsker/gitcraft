use std::cell::RefCell;
use std::collections::VecDeque;
use std::process::Command;

#[derive(Debug)]
pub struct Call {
    pub tool: String,
    pub args: Vec<String>,
    pub env: Vec<(String, Option<String>)>,
    pub input: Option<String>,
}
#[derive(Default)]
struct Recording {
    calls: Vec<Call>,
    answers: VecDeque<Result<String, String>>,
    remotes: Option<Vec<crate::git::Remote>>,
}
thread_local! {
    static ACTIVE: RefCell<Option<Recording>> = const { RefCell::new(None) };
}
pub fn run(
    answers: Vec<Result<String, String>>,
    work: impl FnOnce() -> Result<(), String>,
) -> Result<Vec<Call>, String> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|a| *a.borrow_mut() = None);
        }
    }
    ACTIVE.with(|a| {
        assert!(a.borrow().is_none());
        *a.borrow_mut() = Some(Recording {
            answers: answers.into(),
            ..Default::default()
        });
    });
    let _reset = Reset;
    work()?;
    ACTIVE.with(|a| {
        let mut a = a.borrow_mut();
        let recording = a.take().ok_or("missing active recorder")?;
        assert!(recording.answers.is_empty(), "unconsumed CLI responses");
        Ok(recording.calls)
    })
}
pub fn set_remotes(remotes: Vec<crate::git::Remote>) {
    ACTIVE.with(|a| {
        let mut a = a.borrow_mut();
        assert!(a.is_some(), "missing recorder");
        if let Some(r) = a.as_mut() {
            r.remotes = Some(remotes);
        }
    });
}
pub(super) fn remotes() -> Option<Vec<crate::git::Remote>> {
    ACTIVE.with(|a| a.borrow().as_ref().and_then(|r| r.remotes.clone()))
}
pub(super) fn answer(cmd: &Command, input: Option<&str>) -> Option<Result<String, String>> {
    ACTIVE.with(|a| {
        let mut a = a.borrow_mut();
        let recording = a.as_mut()?;
        recording.calls.push(Call {
            tool: cmd.get_program().to_string_lossy().into_owned(),
            args: cmd
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect(),
            env: cmd
                .get_envs()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.map(|v| v.to_string_lossy().into_owned()),
                    )
                })
                .collect(),
            input: input.map(str::to_owned),
        });
        assert!(!recording.answers.is_empty(), "unexpected forge invocation");
        Some(
            recording
                .answers
                .pop_front()
                .unwrap_or_else(|| Err("unexpected forge invocation".into())),
        )
    })
}
