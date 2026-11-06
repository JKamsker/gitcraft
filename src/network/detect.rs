//! Asking the system whether there is a network connection and whether it is metered:
//! Windows through its connection profile (PowerShell reads it), Linux through NetworkManager
//! (`busctl`). Elsewhere, or when that fails, nothing is known and nothing pauses by itself.

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

/// The PowerShell that prints the internet connection profile's level and cost:
/// `<level> <cost type> <roaming> <over the limit> <approaching it>`, or `none` without one.
#[cfg(windows)]
const WINDOWS_SCRIPT: &str = "$n=[Windows.Networking.Connectivity.NetworkInformation,Windows.Networking.Connectivity,ContentType=WindowsRuntime];\
$p=$n::GetInternetConnectionProfile();\
if($p -eq $null){'none'}else{$c=$p.GetConnectionCost();\
\"$([int]$p.GetNetworkConnectivityLevel()) $([int]$c.NetworkCostType) $($c.Roaming) $($c.OverDataLimit) $($c.ApproachingDataLimit)\"}";

/// What the system says now (a second or so: not on the window's thread).
pub(super) fn ask_system() -> Detected {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", WINDOWS_SCRIPT])
            .creation_flags(CREATE_NO_WINDOW);
        output(&mut cmd).map_or(Detected::UNKNOWN, |out| parse_windows(&out))
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

/// What `cmd` printed, when it ran and went well.
#[allow(dead_code)]
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

/// Reads `WINDOWS_SCRIPT`'s answer. Connected: the profile has internet access (level 3);
/// metered: a fixed or variable cost (2, 3), roaming, or at or near its data limit.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_windows(out: &str) -> Detected {
    let out = out.trim();
    if out == "none" {
        return Detected {
            connected: Some(false),
            metered: None,
        };
    }
    let words: Vec<&str> = out.split_whitespace().collect();
    let [level, cost, roaming, over, near] = words[..] else {
        return Detected::UNKNOWN;
    };
    let yes = |w: &str| w.eq_ignore_ascii_case("true");
    let connected = level.parse::<u8>().ok().map(|l| l == 3);
    let metered = cost
        .parse::<u8>()
        .ok()
        .map(|c| matches!(c, 2 | 3) || yes(roaming) || yes(over) || yes(near));
    Detected { connected, metered }
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
