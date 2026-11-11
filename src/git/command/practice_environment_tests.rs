use super::*;
#[test]
fn synthetic_practice_command_removes_every_inherited_route() {
    let cmd = practice_command(Path::new("sandbox"));
    let env: std::collections::HashMap<_, _> = cmd.get_envs().collect();
    for name in crate::practice::ISOLATED_ENV_REMOVE {
        assert_eq!(env.get(std::ffi::OsStr::new(name)), Some(&None), "{name}");
    }
    for (name, value) in crate::practice::ISOLATED_ENV_SET {
        assert_eq!(
            env.get(std::ffi::OsStr::new(name)).copied().flatten(),
            Some(std::ffi::OsStr::new(value)),
            "{name}"
        );
    }
    assert!(cmd.get_args().any(|a| a == "--no-replace-objects"));
}
