//! System motion policy for graph feedback and celebrations. This narrow adapter can delegate
//! to PR66's shared a11y::motion::reduced when that feature lands.
use std::sync::Once;
use std::sync::atomic::{AtomicU8, Ordering};

const UNKNOWN: u8 = 0;
const REDUCED: u8 = 1;
const FULL: u8 = 2;
static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);
static ASK: Once = Once::new();

pub(super) fn reduced(ctx: &egui::Context) -> bool {
    if ctx.style().animation_time <= 0.0 {
        return true;
    }
    ASK.call_once(|| start(ctx.clone()));
    // Quiet until the platform preference is known, including failed or unsupported queries.
    STATE.load(Ordering::Relaxed) != FULL
}

fn override_value(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
fn start(_: egui::Context) {
    // Fixture source never queries native preferences or opens desktop services.
    STATE.store(REDUCED, Ordering::Relaxed);
}

#[cfg(not(test))]
fn start(ctx: egui::Context) {
    if let Some(reduced) = override_value(std::env::var("GITCRAFT_REDUCE_MOTION").ok().as_deref()) {
        STATE.store(if reduced { REDUCED } else { FULL }, Ordering::Relaxed);
        return;
    }
    if std::thread::Builder::new()
        .name("graph-motion-preference".into())
        .spawn(move || {
            STATE.store(
                if system_preference() == Some(false) {
                    FULL
                } else {
                    REDUCED
                },
                Ordering::Relaxed,
            );
            ctx.request_repaint();
        })
        .is_err()
    {
        STATE.store(REDUCED, Ordering::Relaxed);
    }
}

#[cfg(not(test))]
fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = crate::bounded_process::run_input(
        cmd,
        None,
        &|| true,
        std::time::Duration::from_secs(1),
        4096,
        4096,
    )
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(all(not(test), target_os = "windows"))]
fn system_preference() -> Option<bool> {
    let text = output(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\Desktop\WindowMetrics",
            "/v",
            "MinAnimate",
        ],
    )?;
    text.lines().find_map(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields.as_slice() {
            ["MinAnimate", "REG_SZ", "0"] => Some(true),
            ["MinAnimate", "REG_SZ", "1"] => Some(false),
            _ => None,
        }
    })
}

#[cfg(all(not(test), target_os = "macos"))]
fn system_preference() -> Option<bool> {
    let text = output(
        "defaults",
        &["read", "com.apple.universalaccess", "reduceMotion"],
    )?;
    match text.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(all(not(test), not(any(target_os = "macos", target_os = "windows"))))]
fn system_preference() -> Option<bool> {
    let gnome = output(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "enable-animations"],
    )
    .and_then(|text| match text.trim() {
        "false" => Some(true),
        "true" => Some(false),
        _ => None,
    });
    if gnome == Some(true) {
        return gnome;
    }
    let kde = ["kreadconfig6", "kreadconfig5"].iter().find_map(|program| {
        output(
            program,
            &["--group", "KDE", "--key", "AnimationDurationFactor"],
        )
        .and_then(|text| text.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value == 0.0)
    });
    if kde == Some(true) {
        kde
    } else {
        gnome.or(kde)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn respects_the_shared_override_contract_without_native_queries() {
        assert_eq!(override_value(Some(" YES ")), Some(true));
        assert_eq!(override_value(Some("off")), Some(false));
        assert_eq!(override_value(Some("unknown")), None);
        assert_eq!(override_value(None), None);
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        assert!(reduced(&ctx));
    }
}
