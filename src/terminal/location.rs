//! Live cwd observation for the terminal's owned shell. Linux procfs includes a birth
//! token so a recycled PID is never mistaken for that shell. Unsupported/unreadable
//! observations return None: cleanup conservatively blocks while that terminal is open.
use std::path::PathBuf;
use std::sync::Arc;

type Probe = Arc<dyn Fn() -> Option<Vec<PathBuf>> + Send + Sync>;

pub(super) fn probe(
    pid: Option<u32>,
    foreground: Arc<dyn Fn() -> Option<u32> + Send + Sync>,
) -> Probe {
    #[cfg(target_os = "linux")]
    {
        let identity = pid.and_then(|pid| birth(pid).map(|birth| (pid, birth)));
        Arc::new(move || {
            let (pid, expected) = identity.as_ref()?;
            if birth(*pid).as_ref() != Some(expected) {
                return None;
            }
            let active = foreground()?;
            let active_birth = birth(active)?;
            let paths = locations(*pid, active, |pid| {
                std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
            })?;
            (birth(*pid).as_ref() == Some(expected)
                && birth(active).as_ref() == Some(&active_birth))
            .then_some(paths)
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, foreground);
        Arc::new(|| None)
    }
}

#[cfg(target_os = "linux")]
fn locations(
    shell: u32,
    foreground: u32,
    cwd: impl Fn(u32) -> Option<PathBuf>,
) -> Option<Vec<PathBuf>> {
    let mut paths = vec![cwd(shell)?];
    let active = cwd(foreground)?;
    if !paths.contains(&active) {
        paths.push(active);
    }
    Some(paths)
}

#[cfg(target_os = "linux")]
fn birth(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    token(&stat).map(str::to_owned)
}

#[cfg(target_os = "linux")]
fn token(stat: &str) -> Option<&str> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn birth_token_handles_parentheses_in_process_names() {
        let fields = (3..=22)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            super::token(&format!("42 (a ) shell) {fields}")),
            Some("22")
        );
        assert_eq!(super::token("invalid"), None);
    }

    #[test]
    fn divergent_shell_and_foreground_locations_are_both_observed() {
        let observe = |pid| {
            Some(std::path::PathBuf::from(if pid == 1 {
                "/worktree/nested"
            } else {
                "/elsewhere"
            }))
        };
        assert_eq!(
            super::locations(1, 2, observe),
            Some(vec!["/worktree/nested".into(), "/elsewhere".into()])
        );
        assert!(super::locations(1, 2, |pid| (pid == 1).then(|| "/worktree".into())).is_none());
    }

    #[test]
    fn current_owned_process_cwd_is_observed() -> Result<(), Box<dyn std::error::Error>> {
        let probe = super::probe(
            Some(std::process::id()),
            std::sync::Arc::new(|| Some(std::process::id())),
        );
        assert_eq!(probe(), Some(vec![std::env::current_dir()?]));
        assert!(super::probe(None, std::sync::Arc::new(|| None))().is_none());
        Ok(())
    }
}
