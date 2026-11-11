//! Local cards responsibilities for intents.
use super::*;

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
pub(super) fn identity(repo: &Path) -> Result<(String, String), String> {
    let name = git::run(Some(repo), &["config", "user.name"])?;
    let email = git::run(Some(repo), &["config", "user.email"])?;
    if name.trim().is_empty() || email.trim().is_empty() {
        return Err("Configure your Git name and email before posting an intent".into());
    }
    Ok((name.trim().into(), email.trim().into()))
}
pub(super) fn save(repo: &Path, reference: &str, card: &Card) -> Result<(), String> {
    let old = git::run(Some(repo), &["rev-parse", "--verify", reference]).ok();
    let data = serde_json::to_string(card).map_err(|e| e.to_string())?;
    let blob = git::run_input(Some(repo), &["hash-object", "-w", "--stdin"], &data)?;
    let tree = git::run_input(
        Some(repo),
        &["mktree"],
        &format!("100644 blob {}\tintent.json\n", blob.trim()),
    )?;
    let mut args = vec![
        "-c",
        "commit.gpgSign=false",
        "commit-tree",
        tree.trim(),
        "-m",
        "gitcraft: advisory intent",
    ];
    if let Some(old) = &old {
        args.extend(["-p", old.trim()]);
    }
    let commit = git::run(Some(repo), &args)?;
    git::run(
        Some(repo),
        &[
            "update-ref",
            reference,
            commit.trim(),
            old.as_deref().unwrap_or("").trim(),
        ],
    )
    .map(|_| ())
}
pub(super) fn post(
    repo: &Path,
    path: String,
    symbol: String,
    message: String,
    minutes: u32,
) -> Result<(), String> {
    if path.trim().is_empty()
        || path.starts_with('/')
        || path.split('/').any(|p| p == "..")
        || message.trim().is_empty()
    {
        return Err("Give a relative code path and a short intent".into());
    }
    let (author, email) = identity(repo)?;
    let created = now();
    let id = format!(
        "{:x}",
        md5::compute(format!(
            "{email}:{path}:{symbol}:{message}:{created}:{}",
            std::process::id()
        ))
    );
    let reference = format!("{ROOT}{:x}/{id}", md5::compute(&email));
    let card = Card {
        id,
        author,
        email,
        path,
        symbol,
        message,
        created,
        expires: created.saturating_add(u64::from(minutes.clamp(1, 1440)) * 60),
    };
    validate(&card, &reference, ROOT, created)?;
    save(repo, &reference, &card)
}
