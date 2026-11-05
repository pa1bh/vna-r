//! `vna-gui` — desktop front-end for LiteVNA and NanoVNA-F analysers, built on egui.

mod app;
mod smith;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("vna-r {}", app::VERSION))
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("vna-r", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
