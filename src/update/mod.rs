//! New versions of gitcraft, from its GitHub releases: looked for on every start, downloaded in
//! the background and parked on disk, then put in place of this build with a restart.

mod install;
mod previous;

use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use install::{clean_up, go_back, install, previous, relaunch, relaunch_after_exit};
pub use previous::{PIN_THIS, Previous, settle_pin};

/// This build's version.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Where the releases are published.
const REPO: &str = "https://github.com/JKamsker/gitcraft";

/// Which releases a build follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    /// The releases.
    Main,
    /// A prerelease of every change to the dev branch, and the releases.
    Dev,
}

impl Default for Channel {
    /// The one this build came from.
    fn default() -> Self {
        if CURRENT.contains('-') {
            Channel::Dev
        } else {
            Channel::Main
        }
    }
}

impl Channel {
    pub const ALL: [Channel; 2] = [Channel::Main, Channel::Dev];

    pub fn label(self) -> &'static str {
        match self {
            Channel::Main => crate::i18n::text("interaction-presentation-releases"),
            Channel::Dev => crate::i18n::text("interaction-presentation-dev"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Without the leading `v`.
    pub version: String,
    pub tag: String,
    /// The release's page.
    pub url: String,
}

/// A release newer than this build.
#[derive(Clone, Debug)]
pub struct Found {
    pub release: Release,
    /// The new version, downloaded and unpacked, ready to go in place of this one.
    pub parked: Option<PathBuf>,
}

/// The newest release of `channel` if it is newer than this build and not `skipped`, downloaded
/// and parked when `download` (and this copy of gitcraft can replace itself). Nothing while a
/// version is `pinned` (see [`previous`]), not even a look.
pub fn look(
    channel: Channel,
    download: bool,
    skipped: &str,
    pinned: &str,
) -> Result<Option<Found>, String> {
    if !pinned.is_empty() {
        return Ok(None);
    }
    let Some(release) = latest(channel)?.filter(|r| offered(&r.version, CURRENT, skipped)) else {
        return Ok(None);
    };
    // When it can't be had, the release page still can.
    let parked = (download && install::can_update())
        .then(|| install::prefetch(&release).ok())
        .flatten();
    Ok(Some(Found { release, parked }))
}

/// Whether release `version` is offered to a build of `current`: newer, and not skipped.
fn offered(version: &str, current: &str, skipped: &str) -> bool {
    newer(version, current) && version != skipped
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent("gitcraft")
        .build()
}

/// The newest release of `channel`, if any was published.
fn latest(channel: Channel) -> Result<Option<Release>, String> {
    let agent = agent();
    // Redirects to the latest release's page, `…/releases/tag/vX.Y.Z`. The web pages rather
    // than the REST API: the API allows 60 anonymous requests an hour per IP, which shared
    // addresses (VPNs, offices) use up.
    let resp = agent
        .head(&format!("{REPO}/releases/latest"))
        .call()
        .map_err(|e| e.to_string())?;
    // No release yet: the redirect lands on the list.
    let stable = release_from_url(resp.get_url());
    if channel == Channel::Main {
        return Ok(stable);
    }
    // The latest release is never a prerelease: those are in the feed of the last few.
    let feed = agent
        .get(&format!("{REPO}/releases.atom"))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    Ok(stable
        .into_iter()
        .chain(releases_in_feed(&feed))
        .max_by(|a, b| order(&a.version, &b.version)))
}

/// The releases a page links to.
fn releases_in_feed(feed: &str) -> Vec<Release> {
    feed.split("/releases/tag/")
        .skip(1)
        .filter_map(|rest| {
            let tag = rest.split(['"', '<', '?', '#']).next()?;
            release_from_url(&format!("{REPO}/releases/tag/{tag}"))
        })
        .collect()
}

fn release_from_url(url: &str) -> Option<Release> {
    let tag = url
        .split_once("/releases/tag/")?
        .1
        .split(['?', '#'])
        .next()?;
    // It names files and goes into URLs: a version, nothing else.
    let plain = tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    let version = tag.strip_prefix('v')?;
    (plain && version.starts_with(|c: char| c.is_ascii_digit())).then(|| Release {
        version: version.to_owned(),
        tag: tag.to_owned(),
        url: url.to_owned(),
    })
}

/// Whether version `a` comes after `b`: `1.10.0` > `1.9.2`, and a prerelease comes before its
/// release, `0.3.3` < `0.3.4-dev.9` < `0.3.4-dev.10` < `0.3.4`.
pub fn newer(a: &str, b: &str) -> bool {
    order(a, b) == Ordering::Greater
}

/// Versions in semver's order.
fn order(a: &str, b: &str) -> Ordering {
    let split = |v: &str| -> (Vec<u64>, Option<String>) {
        let v = v.trim_start_matches('v');
        let v = v.split('+').next().unwrap_or_default();
        let (core, pre) = match v.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_owned())),
            None => (v, None),
        };
        let core = core.split('.').map(|n| n.parse().unwrap_or(0)).collect();
        (core, pre)
    };
    let ((a, a_pre), (b, b_pre)) = (split(a), split(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x.cmp(&y);
        }
    }
    match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => prerelease_order(&a, &b),
    }
}

/// `dev.9` < `dev.10`: numbers by their value, before words; a shorter one first.
fn prerelease_order(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.split('.'), b.split('.'));
    loop {
        let (x, y) = match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => (x, y),
        };
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_page_urls() {
        assert_eq!(
            release_from_url("https://github.com/JKamsker/gitcraft/releases/tag/v0.2.0"),
            Some(Release {
                version: "0.2.0".into(),
                tag: "v0.2.0".into(),
                url: "https://github.com/JKamsker/gitcraft/releases/tag/v0.2.0".into()
            })
        );
        // No release yet: the redirect lands on the list.
        assert_eq!(
            release_from_url("https://github.com/JKamsker/gitcraft/releases"),
            None
        );
        // Not a version, or one that would reach into a path or a URL.
        assert_eq!(release_from_url("https://x/releases/tag/nightly"), None);
        assert_eq!(release_from_url("https://x/releases/tag/v1.0/../x"), None);
    }

    #[test]
    fn releases_in_the_feed() {
        let feed = r#"<feed><entry><id>tag:github.com,2008:Repository/1/v0.3.4-dev.7</id>
            <link rel="alternate" type="text/html" href="https://github.com/JKamsker/gitcraft/releases/tag/v0.3.4-dev.7"/>
            </entry><entry>
            <link rel="alternate" type="text/html" href="https://github.com/JKamsker/gitcraft/releases/tag/v0.3.3"/>
            </entry></feed>"#;
        let versions: Vec<String> = releases_in_feed(feed)
            .into_iter()
            .map(|r| r.version)
            .collect();
        assert_eq!(versions, ["0.3.4-dev.7", "0.3.3"]);
    }

    #[test]
    fn what_is_offered() {
        assert!(offered("0.3.4", "0.3.3", ""));
        assert!(!offered("0.3.4", "0.3.3", "0.3.4"));
        assert!(!offered("0.3.3", "0.3.3", ""));
        // Gone back from 0.3.4: an older build is offered neither it nor anything older.
        assert!(!offered("0.3.2", "0.3.3", "0.3.4"));
        assert!(offered("0.3.5", "0.3.3", "0.3.4"));
    }

    #[test]
    fn a_pin_looks_for_nothing() {
        // Returns before any request: no network needed.
        assert!(matches!(look(Channel::Main, true, "", "0.3.3"), Ok(None)));
    }

    #[test]
    fn version_order() {
        assert!(newer("0.2.0", "0.1.0"));
        assert!(newer("v1.10.0", "1.9.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.1.1"));
        assert!(!newer("1.0.0-rc1", "1.0.0"));
        // Prereleases of the dev branch.
        assert!(newer("0.3.4-dev.1", "0.3.3"));
        assert!(newer("0.3.4-dev.10", "0.3.4-dev.9"));
        assert!(newer("0.3.4", "0.3.4-dev.10"));
        assert!(newer("0.3.5-dev.1", "0.3.4"));
        assert!(!newer("0.3.4-dev.7", "0.3.4-dev.7"));
        assert!(newer("1.0.0-dev.1.1", "1.0.0-dev.1"));
        assert!(newer("1.0.0-rc", "1.0.0-1"));
    }
}
