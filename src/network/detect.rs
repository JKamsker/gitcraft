//! Asking the system whether there is a network connection and whether it is metered:
//! Windows through its connection profile (PowerShell reads it), Linux through NetworkManager
//! (`busctl`). Elsewhere, or when that fails, nothing is known and nothing pauses by itself.

#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

/// What the system says about the connection; `None`: it couldn't be told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detected {
    pub connected: Option<bool>,
    pub metered: Option<bool>,
}

impl Detected {
    pub const UNKNOWN: Self = Self {
        connected: None,
        metered: None,
    };
}

/// What the system says now (on Linux a moment: not on the window's thread).
pub(super) fn ask_system() -> Detected {
    #[cfg(windows)]
    {
        ask_windows()
    }
    #[cfg(target_os = "linux")]
    {
        let property = |name: &str| {
            output(Command::new("busctl").args([
                "--system",
                "get-property",
                "org.freedesktop.NetworkManager",
                "/org/freedesktop/NetworkManager",
                "org.freedesktop.NetworkManager",
                name,
            ]))
        };
        parse_linux(
            property("Connectivity").as_deref(),
            property("Metered").as_deref(),
        )
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Detected::UNKNOWN
    }
}

/// Windows' internet connection profile, through WinRT (`NetworkInformation`): its
/// connectivity level and its cost.
#[cfg(windows)]
fn ask_windows() -> Detected {
    use windows::Networking::Connectivity::NetworkInformation;
    // No profile: Windows found no internet connection, which a proxy or VPN can also make it
    // say: not trusted to pause anything.
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else {
        return Detected::UNKNOWN;
    };
    let level = profile.GetNetworkConnectivityLevel().ok().map(|l| l.0);
    let cost = profile.GetConnectionCost().ok();
    let flag = |f: Option<windows::core::Result<bool>>| f.and_then(Result::ok).unwrap_or(false);
    let cost_type = cost
        .as_ref()
        .and_then(|c| c.NetworkCostType().ok())
        .map(|t| t.0);
    from_windows(
        level,
        cost_type,
        flag(cost.as_ref().map(|c| c.Roaming())),
        flag(cost.as_ref().map(|c| c.OverDataLimit())),
        flag(cost.as_ref().map(|c| c.ApproachingDataLimit())),
    )
}

/// What `cmd` printed, when it ran and went well.
#[cfg(target_os = "linux")]
fn output(cmd: &mut Command) -> Option<String> {
    let out = cmd
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Reads Windows' answer. No connection only at level 0 (`None`): 1 and 2 (local, or limited)
/// are what Windows says when its own probe is blocked (a proxy, a VPN), while git may well
/// reach its remotes. Metered: a fixed or variable cost (2, 3), roaming, or at or near the
/// data limit.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn from_windows(
    level: Option<i32>,
    cost: Option<i32>,
    roaming: bool,
    over_limit: bool,
    near_limit: bool,
) -> Detected {
    Detected {
        connected: match level {
            Some(0) => Some(false),
            Some(3) => Some(true),
            _ => None,
        },
        metered: cost.map(|c| matches!(c, 2 | 3) || roaming || over_limit || near_limit),
    }
}

/// Reads NetworkManager's `Connectivity` and `Metered` (as `busctl` prints them: `u 4`).
/// Connectivity 1 is none, 2 to 4 some (a portal, limited, full); 0 unknown. Metered 1 and 3
/// (yes, guessed yes) are, 2 and 4 aren't; 0 unknown.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_linux(connectivity: Option<&str>, metered: Option<&str>) -> Detected {
    let number = |s: Option<&str>| {
        s.and_then(|s| s.trim().strip_prefix("u "))
            .and_then(|n| n.trim().parse::<u32>().ok())
    };
    Detected {
        connected: match number(connectivity) {
            Some(1) => Some(false),
            Some(2..=4) => Some(true),
            _ => None,
        },
        metered: match number(metered) {
            Some(1 | 3) => Some(true),
            Some(2 | 4) => Some(false),
            _ => None,
        },
    }
}
