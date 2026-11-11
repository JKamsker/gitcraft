use super::*;
#[test]
fn passive_reads_are_guarded_locally_and_on_the_repository_machine() {
    for sub in [
        "show",
        "diff",
        "log",
        "cat-file",
        "blame",
        "status",
        "ls-tree",
        "sparse-checkout",
    ] {
        let args = [sub, "HEAD"];
        let mut cmd = Command::new("/recorded/git");
        object_read_policy(&mut cmd, &args);
        let cmd = apply_command_extras(
            cmd,
            Some(Path::new("/recorded/repo")),
            &Extra::default(),
            &args,
        );
        assert_eq!(
            cmd.get_args().next(),
            Some(std::ffi::OsStr::new("--no-lazy-fetch"))
        );
        let req = remote_request(
            &Target {
                host: remote::Host::Ssh("recorded".into()),
                path: "/repo".into(),
            },
            &Extra::default(),
            &args,
        );
        assert!(req.script.starts_with("exec git '--no-lazy-fetch'"));
    }
}
