// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui_wgpu::{SurfaceConfig, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew, wgpu};
use gitcraft::app;

fn main() -> eframe::Result {
    if let Some(code) =
        gitcraft::connections::credential_helper().or_else(gitcraft::connections::ssh_helper)
    {
        std::process::exit(code);
    }
    // Started by ssh to ask the running app something (see `remote::askpass`): no window.
    if let Some(code) = app::askpass_helper() {
        std::process::exit(code);
    }
    // `gitcraft mcp`: the MCP server for coding agents, on stdin and stdout; no window.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "mcp") {
        std::process::exit(gitcraft::mcp::main(&args[1..]));
    }
    // Run by a coding agent's hook after each turn (see `agent::checkpoint`): no window.
    if let Some(code) = app::checkpoint_cli() {
        std::process::exit(code);
    }
    let args = gitcraft::launch::args::from_system(std::env::args_os().skip(1));
    let launch = match gitcraft::launch::parse(args.clone()) {
        gitcraft::launch::Parsed::Run(cmd) => cmd,
        gitcraft::launch::Parsed::Print(text) => {
            app::tell(&text, false);
            return Ok(());
        }
        gitcraft::launch::Parsed::Fail(why) => {
            app::tell(&why, true);
            std::process::exit(2);
        }
    };
    // gitcraft is running already: what this start was given goes to its window.
    match app::single_instance(launch.as_ref(), &args) {
        app::Start::Window => {}
        app::Start::HandedOver => return Ok(()),
        app::Start::Failed(message) => {
            app::report_failed_start(&message);
            std::process::exit(1);
        }
    }
    app::install_panic_log();
    app::warm_up();
    let options = eframe::NativeOptions {
        viewport: app::window_viewport()
            .with_title("GitCraft")
            // Matches gitcraft.desktop, so Linux desktops show its icon and group its windows.
            .with_app_id("gitcraft")
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(app::window_icon()),
        // Runs after eframe restored the last session's size and position.
        window_builder: Some(Box::new(|builder| {
            if cfg!(windows) && builder.maximized == Some(true) {
                app::MAXIMIZE_WHEN_SHOWN.store(true, Ordering::Relaxed);
                builder.with_maximized(false)
            } else {
                builder
            }
        })),
        wgpu_options: wgpu_options(),
        // Next to gitcraft when it runs portable; eframe's own place otherwise.
        persistence_path: app::persistence_path(),
        ..Default::default()
    };
    let result = eframe::run_native(
        "gitcraft",
        options,
        Box::new(|cc| Ok(Box::new(app::App::launched(cc, launch)))),
    );
    app::after_exit();
    result
}

/// egui-wgpu's defaults, made lighter on memory. The `WGPU_*` environment variables still win.
fn wgpu_options() -> WgpuConfiguration {
    let mut setup = WgpuSetupCreateNew::without_display_handle();

    // wgpu's default `MemoryHints::Performance` reserves GPU memory in blocks of 128 MB and
    // 64 MB. On integrated graphics those blocks are system RAM, about 190 MB of it, when the
    // UI needs a few MB. `MemoryUsage` starts the blocks at 8 MB and 4 MB.
    let default_descriptor = Arc::clone(&setup.device_descriptor);
    setup.device_descriptor = Arc::new(move |adapter| wgpu::DeviceDescriptor {
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        ..default_descriptor(adapter)
    });

    if cfg!(windows) {
        let instance = &mut setup.instance_descriptor;
        // Only DirectX 12, which every supported Windows has (with WARP as a software
        // fallback). The default also loads each GPU's Vulkan and OpenGL drivers.
        if wgpu::Backends::from_env().is_none() {
            instance.backends = wgpu::Backends::DX12;
        }
        // FXC ships with Windows. The default looks for `dxcompiler.dll` along the DLL search
        // path and takes whichever copy it finds there, such as another app's.
        if wgpu::Dx12Compiler::from_env().is_none() {
            instance.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
        }
    }

    WgpuConfiguration {
        // Prefer lower memory and input latency over queueing another frame. On macOS the
        // extra full-window drawable alone costs 16 MiB at 2800×1720.
        surface: SurfaceConfig::LOW_LATENCY,
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}
