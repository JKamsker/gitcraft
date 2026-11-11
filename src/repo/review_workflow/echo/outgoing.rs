//! Outgoing responsibilities for echo.
use super::*;

pub(super) fn outgoing(
    repo: &Path,
    book: &Book,
    steps: &[Vec<String>],
) -> Result<Vec<Hit>, String> {
    if !book.enabled || !book.rules.iter().any(|r| r.enabled) {
        return Ok(vec![]);
    }
    let prepared = echo_push::prepare(repo, steps)?;
    outgoing_pairs(repo, book, prepared.pairs?, &prepared.source)
}
pub(super) fn outgoing_pairs(
    repo: &Path,
    book: &Book,
    pairs: Vec<(String, String)>,
    source: &git::CommandSource,
) -> Result<Vec<Hit>, String> {
    let mut hits = vec![];
    for (base, head) in pairs {
        let args = source.bind(&[
            "-c",
            "core.quotePath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            &base,
            &head,
            "--",
        ]);
        let patch = git::run(
            Some(repo),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )?;
        hits.extend(check_patch(&patch, book)?);
    }
    Ok(hits)
}
pub(super) fn push_stamp(repo: &Path) -> Result<String, String> {
    let git = echo_push::stamp(repo)?;
    let book = serde_json::to_string(&load()?).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", md5::compute(format!("{git}\0{book}"))))
}
pub(in crate::repo::review_workflow) fn model(
    repo: &Path,
    settings: &ai::AiSettings,
    prompt: &str,
    cancel: &ai::Cancel,
) -> Result<String, String> {
    let command = ai::run::CommandLine::for_tool(settings)?;
    ai::run::run(&ai::run::Job {
        repo,
        command: &command,
        input: prompt.as_bytes(),
        timeout: ai::TIMEOUT,
        cancel,
    })
    .map(|s| ai::prompt::clean_answer(&s))
    .map_err(|e| format!("{e:?}"))
}
