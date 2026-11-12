use windows_reg_generator::app::RegGeneratorApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([660.0, 860.0])
            .with_min_inner_size([540.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "WindowsRegGenerator",
        options,
        Box::new(
            |_cc| -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
                Ok(Box::new(RegGeneratorApp::default()))
            },
        ),
    )
}
