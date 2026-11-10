use super::*;

#[test]
fn a_token_and_a_password_go_as_their_own_kind() {
    assert_eq!(Login::Token("t0k".into()).header(), "token t0k");
    let basic = Login::Basic {
        user: "ana".into(),
        password: "pw".into(),
    };
    assert_eq!(basic.header(), "Basic YW5hOnB3");
}

#[test]
fn the_proxy_is_the_schemes_unless_no_proxy_names_the_host() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    };
    let vars = env(&[
        ("HTTPS_PROXY", "http://proxy:8080"),
        ("HTTP_PROXY", "http://plain:3128"),
        (
            "NO_PROXY",
            "localhost, .corp.example,git.lan:3000,10.0.0.0/8,::1",
        ),
    ]);
    let proxy = |web: &str| proxy_for(web, vars);
    assert_eq!(
        proxy("https://codeberg.org").as_deref(),
        Some("http://proxy:8080")
    );
    assert_eq!(
        proxy("http://codeberg.org").as_deref(),
        Some("http://plain:3128")
    );
    for direct in [
        "http://localhost:3000",
        "https://git.corp.example",
        "https://corp.example",
        "http://git.lan:3000",
        "http://10.1.2.3:3000",
        "http://[::1]:3000",
    ] {
        assert_eq!(proxy(direct), None, "{direct}");
    }
    assert_eq!(
        proxy("https://notcorp.example").as_deref(),
        Some("http://proxy:8080")
    );
    assert_eq!(
        proxy("http://11.1.2.3").as_deref(),
        Some("http://plain:3128")
    );
    let everything = env(&[("HTTPS_PROXY", "http://p"), ("no_proxy", "*")]);
    assert_eq!(proxy_for("https://codeberg.org", everything), None);
    let all = env(&[("ALL_PROXY", "http://all")]);
    assert_eq!(
        proxy_for("https://x.org", all).as_deref(),
        Some("http://all")
    );
    assert_eq!(proxy_for("https://x.org", env(&[])), None);
}

#[test]
fn reads_what_an_error_says() {
    assert_eq!(
        error_message(r#"{"message":"not found","url":"u"}"#),
        "not found"
    );
    assert_eq!(
        error_message(r#"{"message":"invalid","errors":["title is empty"]}"#),
        "invalid: title is empty"
    );
    assert_eq!(
        error_message("<html>Bad gateway</html>"),
        "<html>Bad gateway</html>"
    );
}

#[test]
fn a_refusal_without_a_login_says_how_to_get_one() {
    let api = Api::new("Forgejo", "https://codeberg.org", "codeberg.org", None);
    let said = api.explain(401, r#"{"message":"token is required"}"#);
    assert!(
        said.starts_with("Forgejo: token is required (HTTP 401)"),
        "{said}"
    );
    assert!(said.contains("tea login add"), "{said}");
    let api = Api::new(
        "Forgejo",
        "https://codeberg.org",
        "codeberg.org",
        Some(Login::Token("x".into())),
    );
    assert!(api.explain(401, "").contains("was refused"));
    assert_eq!(api.explain(409, ""), "Forgejo: HTTP 409");
}
