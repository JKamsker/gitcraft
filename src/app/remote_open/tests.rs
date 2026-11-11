use super::*;

fn form(kind: Kind, path: &str) -> RemoteForm {
    RemoteForm {
        kind,
        distros: vec!["Ubuntu".to_owned()],
        distro: "Ubuntu".to_owned(),
        distros_rx: None,
        hosts: Vec::new(),
        host: "build".to_owned(),
        path: path.to_owned(),
        error: None,
        focus: false,
        browser: None,
    }
}

#[test]
fn the_form_names_a_folder_there() {
    let build = || Host::Ssh("build".to_owned());
    assert_eq!(
        form(Kind::Ssh, " ~/priv/repo ").target(),
        Ok((build(), "~/priv/repo".to_owned()))
    );
    assert_eq!(
        form(Kind::Ssh, "/srv/x").target(),
        Ok((build(), "/srv/x".to_owned()))
    );
    assert!(form(Kind::Ssh, "relative").target().is_err());
    let mut bad = form(Kind::Ssh, "/x");
    bad.host = "-oProxyCommand=x".to_owned();
    assert!(bad.target().is_err());
}

#[test]
fn a_wsl_folder_names_its_distribution() {
    assert_eq!(
        form(Kind::Wsl, "/home/me/repo").target(),
        Ok((Host::Wsl("Ubuntu".to_owned()), "/home/me/repo".to_owned()))
    );
}
