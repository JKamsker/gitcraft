#[test]
fn owned_acl_fixture_records_literal_single_file_arguments_without_recursion() {
    use std::path::Path;
    for grant in [false, true] {
        let command = super::fixture_acl_command(
            Path::new("icacls-fixture"),
            Path::new("owned ' $() & ü"),
            grant,
        );
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(args[0], "owned ' $() & ü");
        assert_eq!(args[1], if grant { "/grant:r" } else { "/remove:g" });
        assert_eq!(args[2], if grant { "*S-1-1-0:(R)" } else { "*S-1-1-0" });
        assert_eq!(args[3], "/Q");
        assert_eq!(args.len(), 4);
    }
}
