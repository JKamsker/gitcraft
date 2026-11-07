//! The system's reduced-motion setting, for gitcraft's own animations and egui's.
//!
//! Asked once, in the background, on the first frame: GNOME's `enable-animations` and KDE's
//! animation speed on Linux and the BSDs, Reduce motion on macOS, Windows' "Animate windows
//! when minimizing and maximizing" (`MinAnimate`, which "Adjust for best performance" turns off
//! too). A change takes effect at the next start.
//! `GITCRAFT_REDUCE_MOTION=1` (or `0`) decides instead, for a desktop that isn't asked.

use std::process::Command;
use std::sync::Once;
use std::sync::atomic::{AtomicU8, Ordering};

/// Not known yet (being asked), or what the answer was.
const UNKNOWN: u8 = 0;
const FULL: u8 = 1;
const REDUCED: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);
static ASK: Once = Once::new();

/// Whether the system asks for less motion. Until it has answered, it doesn't.
pub fn reduced(ctx: &egui::Context) -> bool {
    ASK.call_once(|| start(ctx.clone()));
    STATE.load(Ordering::Relaxed) == REDUCED
}

/// How long an animation of `secs` seconds takes: none at all with reduced motion, so it
/// jumps to its end.
pub fn seconds(ctx: &egui::Context, secs: f32) -> f32 {
    shortened(reduced(ctx), secs)
}

pub(crate) fn shortened(reduced: bool, secs: f32) -> f32 {
    if reduced { 0.0 } else { secs }
}

/// egui's own animations (panels folding, sections opening, scrolling to a row) stop too.
pub(super) fn apply(ctx: &egui::Context) {
    stop_egui_animations(ctx, reduced(ctx));
}

/// With `reduced`, egui's animations take no time (checked every frame: a theme change sets a
/// new style).
pub(crate) fn stop_egui_animations(ctx: &egui::Context, reduced: bool) {
    if !reduced || ctx.global_style().animation_time == 0.0 {
        return;
    }
    ctx.all_styles_mut(|style| {
        style.animation_time = 0.0;
        style.scroll_animation = egui::style::ScrollAnimation::none();
    });
}

fn start(ctx: egui::Context) {
    if let Some(forced) = from_env(std::env::var("GITCRAFT_REDUCE_MOTION").ok().as_deref()) {
        STATE.store(if forced { REDUCED } else { FULL }, Ordering::Relaxed);
        return;
    }
    // A program that hangs (a stuck D-Bus) holds up only this thread.
    let asked = std::thread::Builder::new()
        .name("reduced-motion".into())
        .spawn(move || {
            let state = if ask_system() { REDUCED } else { FULL };
            STATE.store(state, Ordering::Relaxed);
            if state == REDUCED {
                ctx.request_repaint();
            }
        });
    if asked.is_err() {
        STATE.store(FULL, Ordering::Relaxed);
    }
}

/// `GITCRAFT_REDUCE_MOTION`: `1`, `true`, `yes`, `on` reduce; `0`, `false`, `no`, `off` don't;
/// anything else (or unset) leaves it to the system.
pub(crate) fn from_env(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn ask_system() -> bool {
    // "Animate windows when minimizing and maximizing", the window animations' own setting.
    output(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\Desktop\WindowMetrics",
            "/v",
            "MinAnimate",
        ],
    )
    .is_some_and(|out| windows_reduced(&out))
}

#[cfg(target_os = "macos")]
fn ask_system() -> bool {
    output(
        "defaults",
        &["read", "com.apple.universalaccess", "reduceMotion"],
    )
    .is_some_and(|out| macos_reduced(&out))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn ask_system() -> bool {
    let gnome = output(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "enable-animations"],
    )
    .is_some_and(|out| gnome_reduced(&out));
    gnome
        || ["kreadconfig6", "kreadconfig5"].iter().any(|tool| {
            output(
                tool,
                &["--group", "KDE", "--key", "AnimationDurationFactor"],
            )
            .is_some_and(|out| kde_reduced(&out))
        })
}

/// What `program args` prints, when it runs and succeeds.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `gsettings get org.gnome.desktop.interface enable-animations` printed `false`.
#[cfg_attr(any(target_os = "windows", target_os = "macos"), allow(dead_code))]
pub(crate) fn gnome_reduced(out: &str) -> bool {
    out.trim() == "false"
}

/// KDE's animation speed is 0 ("Instant").
#[cfg_attr(any(target_os = "windows", target_os = "macos"), allow(dead_code))]
pub(crate) fn kde_reduced(out: &str) -> bool {
    out.trim().parse::<f32>().is_ok_and(|factor| factor == 0.0)
}

/// `defaults read com.apple.universalaccess reduceMotion` printed `1`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn macos_reduced(out: &str) -> bool {
    out.trim() == "1"
}

/// `reg query … /v MinAnimate` shows the value `0`: a line `MinAnimate    REG_SZ    0`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn windows_reduced(out: &str) -> bool {
    out.lines().any(|line| {
        let mut words = line.split_whitespace();
        words.next() == Some("MinAnimate") && words.last() == Some("0")
    })
}
