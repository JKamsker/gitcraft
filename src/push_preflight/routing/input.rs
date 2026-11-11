//! Capture operation companions and checkout before route resolution.
use super::super::*;

pub(super) struct Input {
    pub(super) initial_config: String,
    pub(super) config_before_push: String,
    pub(super) checkout: String,
    pub(super) checkout_oid: String,
    pub(super) local_refs: BTreeMap<String, String>,
    pub(super) skip_hooks: bool,
    pub(super) tracking: bool,
    pub(super) extras: Vec<String>,
    pub(super) force: bool,
    pub(super) remote: Option<String>,
    pub(super) spec: Option<String>,
    pub(super) implicit_route: bool,
    pub(super) delete_option: bool,
    pub(super) words: Vec<String>,
}

pub(super) fn capture(
    repo: &Path,
    original: &[Vec<String>],
    active: &dyn Fn() -> bool,
) -> Result<Input, String> {
    let initial_config = config_stamp(repo, active)?;
    validate_operations(original)?;
    let pushes: Vec<_> = original.iter().filter_map(|o| push_words(o)).collect();
    if pushes.len() != 1 {
        return Err("Preview one push at a time; no push was run.".into());
    }
    let skip_hooks = pushes[0].iter().any(|w| w == "--no-verify");
    let (remote, spec, force, tracking, extras, delete_option) = parse(pushes[0])?;
    let implicit_route = remote.is_none() && spec.is_none();
    let mut local_refs = BTreeMap::new();
    for op in original {
        if let Some(name) = companion_branch(op) {
            let reference = format!("refs/heads/{name}");
            valid_ref(repo, &reference, active)?;
            let oid = optional(
                repo,
                &[
                    "rev-parse",
                    "--verify",
                    "-q",
                    "--end-of-options",
                    &reference,
                ],
                active,
            )?;
            local_refs.insert(reference, oid.trim().to_owned());
        }
    }
    let removed_before_push: Vec<_> = original
        .iter()
        .take_while(|op| push_words(op).is_none())
        .filter_map(|op| match op.as_slice() {
            [command, flag, end, name]
                if command == "branch" && matches!(flag.as_str(), "-d" | "-D") && end == "--" =>
            {
                Some(format!("branch.{name}."))
            }
            _ => None,
        })
        .collect();
    let config_before_push = initial_config
        .split('\0')
        .filter(|item| {
            let key = item.split_once('\n').map_or(*item, |(key, _)| key);
            !removed_before_push
                .iter()
                .any(|prefix| key.starts_with(prefix))
        })
        .filter(|item| !item.is_empty())
        .map(|item| format!("{item}\0"))
        .collect::<String>();
    let checkout = optional(repo, &["symbolic-ref", "-q", "HEAD"], active)?
        .trim()
        .to_owned();
    let checkout_oid = read(repo, &["rev-parse", "--verify", "HEAD"], active)?
        .trim()
        .to_owned();
    let words = pushes[0].to_vec();
    Ok(Input {
        initial_config,
        config_before_push,
        checkout,
        checkout_oid,
        local_refs,
        skip_hooks,
        tracking,
        extras,
        force,
        remote,
        spec,
        implicit_route,
        delete_option,
        words,
    })
}
