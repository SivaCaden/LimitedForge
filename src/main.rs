mod app;
mod data;
mod mtgjson;
mod pack;
mod tts;

fn main() -> eframe::Result {
    // Force Mesa software rendering in environments without GPU access (e.g. WSL2).
    if std::env::var("LIBGL_ALWAYS_SOFTWARE").is_err() {
        unsafe { std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1") };
    }
    eframe::run_native(
        "LimitedForge",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([640.0, 720.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(app::LimitedForgeApp::new()))),
    )
}
