use super::guard::{Runner, Snapshot, text};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    pub name: String,
    pub fetch_urls: Vec<String>,
    pub push_urls: Vec<String>,
    pub fetch: Vec<String>,
    pub push: Vec<String>,
    pub mirror: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Preview {
    pub draft: Settings,
    pub mappings: Vec<String>,
    pub prune: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Editor {
    pub snapshot: Snapshot,
    pub original: Settings,
    pub draft: Settings,
    pub preview: Option<Preview>,
    pub saving_blocked: Option<String>,
}
pub(crate) fn values(repo: &Path, key: &str) -> Result<Vec<String>, String> {
    let normalized = key
        .rsplit_once('.')
        .map(|(prefix, field)| format!("{prefix}.{}", field.to_ascii_lowercase()))
        .unwrap_or_else(|| key.to_ascii_lowercase());
    let all = text(repo, &["config", "--null", "--list", "--includes"])?;
    Ok(all
        .split('\0')
        .filter_map(|s| {
            let (k, v) = s.split_once('\n')?;
            (k == normalized).then(|| v.to_owned())
        })
        .collect())
}
pub(crate) fn inspect(repo: &Path, name: &str) -> Result<Editor, String> {
    let remotes = text(repo, &["remote"])?;
    let name = if name.is_empty() {
        remotes.lines().next().ok_or("Add a remote first")?
    } else {
        name
    };
    if !remotes.lines().any(|r| r == name) {
        return Err("Remote no longer exists".into());
    }
    let get = |field: &str| values(repo, &format!("remote.{name}.{field}"));
    let draft = Settings {
        name: name.into(),
        fetch_urls: get("url")?,
        push_urls: get("pushurl")?,
        fetch: get("fetch")?,
        push: get("push")?,
        mirror: boolean(repo, &format!("remote.{name}.mirror"), false)?,
    };
    Ok(Editor {
        snapshot: Snapshot::capture(repo)?,
        original: draft.clone(),
        draft,
        saving_blocked: local_edit_blocker(repo, name)?,
        preview: None,
    })
}
#[derive(Clone, Debug)]
struct Spec {
    source: String,
    destination: Option<String>,
    negative: bool,
}
fn valid_ref(s: &str) -> bool {
    s.starts_with("refs/")
        && !s.bytes().any(|b| b <= 32 || b == 127)
        && !s.contains([' ', '\t', '\n', '\r', '\0', ':', '^', '?', '[', '\\'])
        && !s.contains("..")
        && !s.contains("@{")
        && !s.ends_with(['/', '.'])
        && !s
            .split('/')
            .any(|c| c.is_empty() || c.starts_with('.') || c.ends_with(".lock"))
        && s.matches('*').count() <= 1
}
fn parse(s: &str, negative_allowed: bool) -> Result<Spec, String> {
    let (negative, s) = s
        .strip_prefix('^')
        .map_or((false, s.strip_prefix('+').unwrap_or(s)), |s| (true, s));
    if negative {
        if !negative_allowed || !valid_ref(s) {
            return Err("Negative fetch refspecs must name one refs/ prefix or pattern, without a destination".into());
        }
        return Ok(Spec {
            source: s.into(),
            destination: None,
            negative,
        });
    }
    let Some((a, b)) = s.split_once(':') else {
        return Err("Use source:destination refspecs with full refs/ names".into());
    };
    if !valid_ref(a) || !valid_ref(b) || a.matches('*').count() != b.matches('*').count() {
        return Err("Refspecs need valid full refs and matching single wildcards".into());
    }
    Ok(Spec {
        source: a.into(),
        destination: Some(b.into()),
        negative,
    })
}
fn map(pattern: &str, replacement: &str, value: &str) -> Option<String> {
    if let Some((a, b)) = pattern.split_once('*') {
        let middle = value.strip_prefix(a)?.strip_suffix(b)?;
        Some(replacement.replace('*', middle))
    } else {
        (pattern == value).then(|| replacement.into())
    }
}
pub(crate) fn boolean(repo: &Path, key: &str, default: bool) -> Result<bool, String> {
    let out = super::captured(repo, &["config", "--bool", "--get", key], None)?;
    match out.code {
        Some(0) => match out.stdout.trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(format!("Invalid boolean configuration for {key}")),
        },
        Some(1) => Ok(default),
        _ => Err(format!("Cannot read {key}: {}", out.stderr.trim())),
    }
}
pub(crate) fn validate_url(url: &str) -> Result<(), String> {
    let helper = url
        .find("::")
        .is_some_and(|i| !url[..i].contains(['/', '\\']));
    if url.is_empty() || url.starts_with('-') || url.contains(['\n', '\r', '\0']) || helper {
        return Err("Use a regular HTTPS, SSH, Git or filesystem URL without options or external transport helpers".into());
    }
    Ok(())
}
fn local_edit_blocker(repo: &Path, name: &str) -> Result<Option<String>, String> {
    let local = text(
        repo,
        &["config", "--local", "--no-includes", "--null", "--list"],
    )?;
    for field in ["url", "pushurl", "fetch", "push", "mirror"] {
        let key = format!("remote.{name}.{field}");
        let own: Vec<_> = local
            .split('\0')
            .filter_map(|r| r.split_once('\n'))
            .filter(|(k, _)| *k == key)
            .map(|(_, v)| v.to_owned())
            .collect();
        if values(repo, &key)? != own {
            return Ok(Some(format!(
                "{key} contains inherited, included, profile or worktree values. This editor writes the repository config only; remove or edit those values at their source first."
            )));
        }
    }
    Ok(None)
}
pub(crate) fn validate(settings: &Settings) -> Result<(), String> {
    if settings.fetch_urls.is_empty() {
        return Err("A fetch URL is required".into());
    }
    for url in settings.fetch_urls.iter().chain(&settings.push_urls) {
        validate_url(url)?;
    }
    for s in &settings.fetch {
        parse(s, true)?;
    }
    for s in &settings.push {
        parse(s, false)?;
    }
    Ok(())
}
pub(crate) fn preview(repo: &Path, mut editor: Editor) -> Result<Editor, String> {
    validate(&editor.draft)?;
    editor.snapshot.check(repo)?;
    if let Some(pause) = crate::network::refused() {
        return Err(pause.why().into());
    }
    let specs: Vec<_> = editor
        .draft
        .fetch
        .iter()
        .map(|s| parse(s, true))
        .collect::<Result<_, _>>()?;
    let advertised = text(
        repo,
        &["ls-remote", "--refs", "--", &editor.draft.fetch_urls[0]],
    )?;
    let sources: BTreeSet<&str> = advertised
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(_, r)| r))
        .collect();
    let excluded = |s: &str| {
        specs
            .iter()
            .any(|p| p.negative && map(&p.source, "*", s).is_some())
    };
    let mut mappings = Vec::new();
    for src in &sources {
        if excluded(src) {
            mappings.push(format!("{src} excluded"));
            continue;
        }
        for spec in specs.iter().filter(|s| !s.negative) {
            if let Some(dst) = map(
                &spec.source,
                spec.destination.as_deref().unwrap_or_default(),
                src,
            ) {
                mappings.push(format!("{src} → {dst}"));
            }
        }
    }
    let local = text(repo, &["for-each-ref", "--format=%(refname)"])?;
    let mut prune = BTreeSet::new();
    for dst in local.lines() {
        for spec in specs.iter().filter(|s| !s.negative) {
            if let Some(src) = map(
                spec.destination.as_deref().unwrap_or_default(),
                &spec.source,
                dst,
            ) && !excluded(&src)
                && !sources.contains(src.as_str())
            {
                prune.insert(dst.to_owned());
            }
        }
    }
    for s in &editor.draft.fetch {
        mappings.push(format!("Fetch mapping: {s}"));
    }
    for push in &editor.draft.push {
        let spec = parse(push, false)?;
        for src in local.lines() {
            if let Some(dst) = map(
                &spec.source,
                spec.destination.as_deref().unwrap_or_default(),
                src,
            ) {
                mappings.push(format!("Push: {src} → {dst}"));
            }
        }
    }
    if editor.draft.mirror {
        mappings.push("Push mirror: force all refs and delete destination refs absent locally. This can overwrite other people's work.".into());
    }
    editor.snapshot.check(repo)?;
    editor.preview = Some(Preview {
        draft: editor.draft.clone(),
        mappings,
        prune: prune.into_iter().collect(),
    });
    Ok(editor)
}
pub(crate) fn set_values(runner: &Runner<'_>, key: &str, values: &[String]) -> Result<(), String> {
    let existing = values.first();
    if let Some(first) = existing {
        runner.run(&["config", "--local", "--replace-all", key, first])?;
    } else {
        // --unset-all exits 5 when no local value exists. Do not confuse inherited values
        // with local overrides, and do not suppress genuine I/O/locking errors.
        let local = super::captured(runner.repo, &["config", "--local", "--get-all", key], None)?;
        match local.code {
            Some(0) => {
                runner.run(&["config", "--local", "--unset-all", key])?;
            }
            Some(1) => (),
            _ => return Err(local.stderr),
        }
    }
    for value in values.iter().skip(1) {
        runner.run(&["config", "--local", "--add", key, value])?;
    }
    Ok(())
}
pub(crate) fn save(runner: &Runner<'_>, editor: &Editor) -> Result<String, String> {
    validate(&editor.draft)?;
    if editor.original.name != editor.draft.name
        || editor
            .preview
            .as_ref()
            .is_none_or(|p| p.draft != editor.draft)
    {
        return Err("Preview these exact edits before saving".into());
    }
    editor.snapshot.check(runner.repo)?;
    if let Some(why) = local_edit_blocker(runner.repo, &editor.draft.name)? {
        return Err(why);
    }
    let s = &editor.draft;
    for (field, value) in [
        ("url", s.fetch_urls.clone()),
        ("pushurl", s.push_urls.clone()),
        ("fetch", s.fetch.clone()),
        ("push", s.push.clone()),
        ("mirror", vec![s.mirror.to_string()]),
    ] {
        set_values(runner, &format!("remote.{}.{field}", s.name), &value).map_err(|e| format!("Remote configuration save stopped at {field}: {e}. Earlier fields may already be saved; reopen the editor before retrying."))?;
    }
    if inspect(runner.repo, &s.name)?.draft != *s {
        return Err("Effective remote settings differ after saving. Reopen the editor and check inherited configuration before using this remote.".into());
    }
    Ok(
        "Remote URLs, refspecs and mirror policy saved. No fetch, push or prune was performed."
            .into(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inherited_remote_values_are_refused_before_any_config_write() -> Result<(), String> {
        let source = crate::git::fixture::Repo::new("refspec-inherited-source")?;
        source.commit("base", &[])?;
        let r = crate::git::fixture::Repo::new("refspec-inherited-local")?;
        r.commit("base", &[])?;
        r.git(&["remote", "add", "vendor", &source.path().to_string_lossy()])?;
        let included = r.path().join(".git/remote-include");
        std::fs::write(
            &included,
            "[remote \"vendor\"]\n fetch = +refs/tags/*:refs/tags/*\n",
        )
        .map_err(|e| e.to_string())?;
        r.git(&[
            "config",
            "include.path",
            included.to_str().ok_or("include path")?,
        ])?;
        let mut e = inspect(r.path(), "vendor")?;
        assert!(e.saving_blocked.is_some());
        e.draft.push_urls = vec![source.path().to_string_lossy().into_owned()];
        let e = preview(r.path(), e)?;
        let before = r.git(&["config", "--local", "--no-includes", "--list"])?;
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        assert!(save(&runner, &e).is_err());
        assert_eq!(
            r.git(&["config", "--local", "--no-includes", "--list"])?,
            before
        );
        r.git(&["config", "remote.vendor.mirror", "TRUE"])?;
        assert!(inspect(r.path(), "vendor")?.draft.mirror);
        assert!(validate_url("ssh://git@[::1]/repo.git").is_ok());
        assert!(validate_url("ext::arbitrary command").is_err());
        Ok(())
    }
    #[test]
    fn negative_mapping_pruning_and_guarded_url_edits() -> Result<(), String> {
        let source = crate::git::fixture::Repo::new("refspec-source")?;
        source.commit("base", &[])?;
        source.git(&["branch", "release/a"])?;
        source.git(&["branch", "private/x"])?;
        let r = crate::git::fixture::Repo::new("refspec-destination")?;
        r.commit("base", &[])?;
        r.git(&["remote", "add", "vendor", &source.path().to_string_lossy()])?;
        r.git(&["update-ref", "refs/remotes/vendor/gone", "HEAD"])?;
        r.git(&["update-ref", "refs/remotes/vendor/private/old", "HEAD"])?;
        let mut e = inspect(r.path(), "vendor")?;
        e.draft.fetch = vec![
            "+refs/heads/*:refs/remotes/vendor/*".into(),
            "^refs/heads/private/*".into(),
        ];
        e.draft.push_urls = vec![source.path().to_string_lossy().into_owned()];
        let e = preview(r.path(), e)?;
        let p = e.preview.as_ref().ok_or("preview absent")?;
        assert_eq!(p.prune, ["refs/remotes/vendor/gone"]);
        assert!(
            p.mappings
                .iter()
                .any(|l| l == "refs/heads/release/a → refs/remotes/vendor/release/a")
        );
        let control = crate::git::OpControl::new(|| {});
        let runner = Runner {
            repo: r.path(),
            control: &control,
            credentials: None,
        };
        save(&runner, &e)?;
        assert!(save(&runner, &e).is_err());
        assert_eq!(inspect(r.path(), "vendor")?.draft, e.draft);
        assert!(parse("^refs/heads/no:refs/remotes/no", true).is_err());
        assert!(parse("+refs/heads/*:refs/remotes/vendor/no", true).is_err());
        Ok(())
    }
}
