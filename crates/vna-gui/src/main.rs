//! `vna-gui` — desktop front-end for the LiteVNA 67, built on egui.

mod app;
mod smith;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("vna-r — LiteVNA")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("vna-r", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
