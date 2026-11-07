// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew, wgpu};
use gitcraft::{
    app,
    standalone::{self, Invocation},
};

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let explicit = args.first().and_then(|a| a.to_str()).is_some_and(|s| {
        matches!(
            s,
            "--diff" | "--merge" | "--mergetool" | "--help" | "-h" | "--version" | "-V"
        )
    });
    // Started by ssh to ask the running app something (see `remote::askpass`): no window.
    // Helpers expect UTF-8 protocol fields. Native file arguments never pass through
    // their std::env::args conversion, repository discovery or profile fallback.
    if !explicit && let Some(code) = app::askpass_helper() {
        return std::process::ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    let invocation = match standalone::parse(args) {
        Ok(mode) => mode,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    match &invocation {
        Invocation::Help => {
            print!("{}", standalone::USAGE);
            return std::process::ExitCode::SUCCESS;
        }
        Invocation::Version => {
            println!("gitcraft {}", env!("CARGO_PKG_VERSION"));
            return std::process::ExitCode::SUCCESS;
        }
        Invocation::Repository(_) => app::warm_up(),
        _ => {}
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
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
        ..Default::default()
    };
    if matches!(&invocation, Invocation::Diff(_) | Invocation::Merge(_)) {
        return match standalone::run(invocation, options) {
            Ok(code) => std::process::ExitCode::from(code),
            Err(e) => {
                eprintln!("File tool failed: {e}");
                std::process::ExitCode::from(2)
            }
        };
    }
    let Invocation::Repository(open_arg) = invocation else {
        return std::process::ExitCode::from(2);
    };
    let result = eframe::run_native(
        "gitcraft",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, open_arg)))),
    );
    app::after_exit();
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
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
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}
