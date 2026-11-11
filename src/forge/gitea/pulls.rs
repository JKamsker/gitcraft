//! Pull requests, retargeting, releases and asset publication.
use super::*;

pub(super) fn list(api: &Api, hosting: &Hosting) -> Result<Vec<PullRequest>, String> {
    let endpoint = format!("repos/{}/pulls?state=open&sort=recentupdate", hosting.path);
    let items = api.get_all(&endpoint, 100)?;
    Ok(items.iter().map(parse_pull_request).collect())
}

/// The latest closed pull requests that were merged; the API lists those closed unmerged too.
pub(super) fn merged(api: &Api, hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
    let endpoint = format!(
        "repos/{}/pulls?state=closed&sort=recentupdate",
        hosting.path
    );
    let items = api.get_all(&endpoint, 100)?;
    Ok(merged_only(&items))
}

pub(super) fn merged_only(items: &[Value]) -> Vec<MergedPullRequest> {
    let merged: Vec<Value> = items
        .iter()
        .filter(|item| item.get("merged").and_then(Value::as_bool) == Some(true))
        .cloned()
        .collect();
    super::super::merged_items(&merged, ["/number", "/head/ref", "/head/sha", "/base/ref"])
}

/// How many of the pull requests updated last are looked through for a branch's: the API
/// can't be asked by branch.
const FIND_IN: usize = 300;

/// The latest pull request proposing `branch` of this repository, whatever its state.
pub(super) fn find(
    api: &Api,
    hosting: &Hosting,
    branch: &str,
) -> Result<Option<super::super::Found>, String> {
    let endpoint = format!("repos/{}/pulls?state=all&sort=recentupdate", hosting.path);
    let items = api.get_all(&endpoint, FIND_IN)?;
    let found = items
        .iter()
        .filter(|item| at(item, "/head/ref") == branch)
        .map(|item| {
            let merged = item.get("merged").and_then(Value::as_bool).unwrap_or(false);
            let state = match at(item, "/state") {
                _ if merged => PrState::Merged,
                "open" => PrState::Open,
                _ => PrState::Closed,
            };
            super::super::Found {
                pr: parse_pull_request(item),
                state,
            }
        })
        .collect();
    Ok(super::super::pick_found(found))
}

/// Points pull request `number` at `base`.
pub(super) fn retarget(
    api: &Api,
    hosting: &Hosting,
    number: u64,
    base: &str,
) -> Result<(), String> {
    let endpoint = format!("repos/{}/pulls/{number}", hosting.path);
    api.send("PATCH", &endpoint, Some(&json!({ "base": base })))
        .map(drop)
        .map_err(String::from)
}

pub(super) fn create(api: &Api, hosting: &Hosting, new: &NewPullRequest) -> Result<String, String> {
    // Drafts are marked in the title, which both forges understand.
    let title = if new.draft && !is_draft_title(new.title) {
        format!("WIP: {}", new.title)
    } else {
        new.title.to_owned()
    };
    let payload = json!({
        "head": new.branch,
        "base": new.base,
        "title": title,
        "body": new.body,
    });
    let endpoint = format!("repos/{}/pulls", hosting.path);
    // A branch pushed a moment ago may not be known yet: the forge takes pushes in turn. Without
    // a login, "not found" is the private repository's answer, which waiting won't change.
    let mut tries = 0;
    let answer = loop {
        match api.send("POST", &endpoint, Some(&payload)) {
            Err(failure) if failure.status == Some(404) && api.logged_in() && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_secs(1));
            }
            answer => break answer?,
        }
    };
    let url = match at(&answer, "/html_url") {
        "" => {
            return Err(crate::i18n::message(
                "backend-didn-t-say-which-pull-request-it-opened-56835e",
                &[("name", (api.name()).to_string().into())],
            ));
        }
        url => url.to_owned(),
    };
    let number = answer.get("number").and_then(Value::as_u64).unwrap_or(0);
    if new.reviewers.is_empty() {
        return Ok(url);
    }
    if number == 0 {
        return Err(crate::i18n::message(
            "backend-opened-url-but-didn-t-say-its-number-ask-for-the-reviews-on-its-p-449045",
            &[
                ("url", (url).to_string().into()),
                ("name", (api.name()).to_string().into()),
            ],
        ));
    }
    let endpoint = format!("repos/{}/pulls/{number}/requested_reviewers", hosting.path);
    super::super::ask_reviewers(url, new.reviewers, |who| {
        api.send(
            "POST",
            &endpoint,
            Some(&reviewers_payload(&[who.to_owned()])),
        )
        .map(|_| ())
        .map_err(|e| e.message)
    })
}

/// Who to ask, as the API takes them: accounts, and teams by their name (the organisation is
/// the repository's).
pub(super) fn reviewers_payload(reviewers: &[String]) -> Value {
    let (teams, users): (Vec<&String>, Vec<&String>) =
        reviewers.iter().partition(|r| r.contains('/'));
    let teams: Vec<&str> = teams
        .iter()
        .map(|t| t.rsplit('/').next().unwrap_or(t))
        .collect();
    json!({ "reviewers": users, "team_reviewers": teams })
}

/// Makes the release of the pushed tag `new.tag`; its web page.
pub(super) fn create_release(
    api: &Api,
    hosting: &Hosting,
    new: &super::super::NewRelease,
) -> Result<String, String> {
    let tag = encode(new.tag);
    // Made already (a step that timed out after the forge made it): that one.
    if let Ok(made) = api.get(&format!("repos/{}/releases/tags/{tag}", hosting.path)) {
        return Ok(at(&made, "/html_url").to_owned());
    }
    // The tag must be on the server: without it, the forge would make one of the default branch.
    api.get(&format!("repos/{}/tags/{tag}", hosting.path))
        .map_err(|e| {
            crate::i18n::message(
                "backend-isn-t-on-yet-push-it-first-1584cd",
                &[
                    ("tag", (new.tag).to_string().into()),
                    ("host", (hosting.host).to_string().into()),
                    ("message", (e.message).to_string().into()),
                ],
            )
        })?;
    let answer = api.send(
        "POST",
        &format!("repos/{}/releases", hosting.path),
        Some(&json!({
            "tag_name": new.tag,
            "name": new.title,
            "body": new.notes,
            "prerelease": new.prerelease,
            "draft": false,
        })),
    )?;
    Ok(at(&answer, "/html_url").to_owned())
}

/// Sends each of `files` to the release of `tag`, found by its tag.
pub(super) fn upload_assets(
    api: &Api,
    hosting: &Hosting,
    tag: &str,
    files: &[PathBuf],
) -> Result<(), String> {
    let release = api.get(&format!(
        "repos/{}/releases/tags/{}",
        hosting.path,
        encode(tag)
    ))?;
    let attached: Vec<String> = release
        .get("assets")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|x| at(x, "/name").to_owned()).collect())
        .unwrap_or_default();
    let id = release.get("id").and_then(Value::as_u64).ok_or_else(|| {
        crate::i18n::message(
            "backend-didn-t-say-which-release-tag-is-1ede89",
            &[
                ("name", (api.name()).to_string().into()),
                ("tag", (tag).to_string().into()),
            ],
        )
    })?;
    for file in files {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_owned());
        // Sent already (a resumed upload): not twice.
        if attached.contains(&name) {
            continue;
        }
        let bytes = std::fs::read(file).map_err(|e| {
            crate::i18n::message(
                "backend-couldn-t-read-e-decf41",
                &[
                    ("path", (file.display()).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })?;
        let query = encode(&name);
        api.upload(
            &format!("repos/{}/releases/{id}/assets?name={query}", hosting.path),
            "attachment",
            &name,
            &bytes,
        )
        .map_err(|e| {
            crate::i18n::message(
                "backend-couldn-t-attach-name-30c34d",
                &[
                    ("name", (name).to_string().into()),
                    ("message", (e.message).to_string().into()),
                ],
            )
        })?;
    }
    Ok(())
}

/// `text` as one part of a URL: everything but letters, digits and `-_.~` percent-encoded.
pub(super) fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Whether a title marks its pull request a draft (work in progress), as both forges read it.
pub(super) fn is_draft_title(title: &str) -> bool {
    let title = title.trim_start().to_lowercase();
    ["wip:", "[wip]"].iter().any(|mark| title.starts_with(mark))
}

/// The account at `pointer`: a deleted one is "Ghost" to these forges.
pub(super) fn user(v: &Value, pointer: &str) -> String {
    match at(v, pointer) {
        "" => "Ghost".to_owned(),
        name => name.to_owned(),
    }
}

pub(super) fn parse_pull_request(item: &Value) -> PullRequest {
    let title = at(item, "/title");
    PullRequest {
        number: item.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: title.to_owned(),
        branch: at(item, "/head/ref").to_owned(),
        base: at(item, "/base/ref").to_owned(),
        base_oid: String::new(),
        from_fork: item.pointer("/head/repo_id") != item.pointer("/base/repo_id"),
        sha: at(item, "/head/sha").to_owned(),
        author: user(item, "/user/login"),
        url: at(item, "/html_url").to_owned(),
        draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false) || is_draft_title(title),
    }
}
