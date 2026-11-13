//! `vna-gui` — desktop front-end for LiteVNA and NanoVNA-F analysers, built on egui.

// A release build on Windows is a GUI program without a console window
// (debug builds keep it, for RUST_LOG output).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod smith;
mod worker;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use eframe::egui_wgpu::{wgpu, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    // `RUST_LOG=info` shows eframe's and wgpu's choices (adapter, backend) on stderr.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,vna_gui=info")).init();
    let mut wgpu_setup = WgpuSetupCreateNew::without_display_handle();
    wgpu_setup.native_adapter_selector = Some(Arc::new(pick_adapter));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("vna-r {}", app::VERSION))
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        persist_window: true,
        wgpu_options: WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(wgpu_setup),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native("vna-r", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

/// The browser build: the same [`app::App`] in a canvas, without a device
/// (the serial port and the worker thread do not exist there): a viewer for
/// Touchstone files, see `docs/09-wasm-proef.md`.
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    use eframe::web_sys;

    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().and_then(|w| w.document()).expect("no document");
        let canvas = document
            .get_element_by_id("vna-r-canvas")
            .expect("no element with id vna-r-canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("vna-r-canvas is not a canvas");
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
            )
            .await;
        // The page shows a "loading" line, which the first frame takes away
        // (`WebFiles::frame_shown`); an error replaces it here.
        if let Err(e) = started {
            if let Some(loading) = document.get_element_by_id("loading") {
                loading.set_inner_html("vna-r could not start in this browser; the console has the details.");
            }
            log::error!("starting eframe: {e:?}");
        }
    });
}

/// Choose the graphics adapter. wgpu's own choice is "Vulkan first", which on
/// an old NVIDIA card under the open-source nouveau driver lands on the
/// experimental NVK Vulkan driver: frames arrive late and menus miss clicks,
/// while the same card's OpenGL path is fine. So: skip drivers known to be
/// shaky or software-only when a hardware alternative exists, then prefer a
/// modern backend and a real GPU. `WGPU_BACKEND=vulkan|gl|…` still limits the
/// candidates beforehand, so it remains a way to force a backend.
#[cfg(not(target_arch = "wasm32"))]
fn pick_adapter(adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>) -> Result<wgpu::Adapter, String> {
    let usable: Vec<&wgpu::Adapter> = adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .collect();
    if usable.is_empty() {
        return Err("no graphics adapter can draw to this window".into());
    }
    for a in &usable {
        log::debug!("graphics adapter candidate: {}", describe(&a.get_info()));
    }
    let best = usable.iter().max_by_key(|a| adapter_rank(&a.get_info())).expect("non-empty");
    for a in &usable {
        let info = a.get_info();
        if is_shaky(&info) && !std::ptr::eq(*a, *best) {
            log::info!(
                "skipping graphics adapter {} (known to stutter here; WGPU_BACKEND={:?} forces it)",
                describe(&info),
                format!("{:?}", info.backend).to_lowercase()
            );
        }
    }
    Ok((*best).clone())
}

/// Higher is better. Order of importance: not a shaky driver, not software,
/// a modern backend, a real GPU.
#[cfg(not(target_arch = "wasm32"))]
fn adapter_rank(info: &wgpu::AdapterInfo) -> (bool, bool, u8, u8) {
    let backend = match info.backend {
        wgpu::Backend::Metal | wgpu::Backend::Vulkan | wgpu::Backend::Dx12 => 2,
        wgpu::Backend::Gl => 1,
        _ => 0,
    };
    let device = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 3,
        wgpu::DeviceType::IntegratedGpu => 2,
        wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 1,
        wgpu::DeviceType::Cpu => 0,
    };
    (!is_shaky(info), !is_software(info), backend, device)
}

/// Mesa's NVK (nouveau Vulkan) on the pre-Turing cards it supports only experimentally.
#[cfg(not(target_arch = "wasm32"))]
fn is_shaky(info: &wgpu::AdapterInfo) -> bool {
    info.backend == wgpu::Backend::Vulkan && (info.driver.contains("NVK") || info.name.contains("NVK"))
}

/// Software rasterisers: fine as a last resort, slow for a plot that redraws on every mouse move.
#[cfg(not(target_arch = "wasm32"))]
fn is_software(info: &wgpu::AdapterInfo) -> bool {
    let n = format!("{} {}", info.name, info.driver).to_lowercase();
    info.device_type == wgpu::DeviceType::Cpu
        || n.contains("llvmpipe")
        || n.contains("lavapipe")
        || n.contains("swiftshader")
        || n.contains("softpipe")
}

#[cfg(not(target_arch = "wasm32"))]
fn describe(info: &wgpu::AdapterInfo) -> String {
    format!(
        "{:?} {} ({:?}; {} {})",
        info.backend, info.name, info.device_type, info.driver, info.driver_info
    )
}
