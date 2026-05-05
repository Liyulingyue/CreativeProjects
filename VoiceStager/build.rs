use image::GenericImageView;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let png_path = "assets/app_icon.png";
        let ico_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app_icon.ico");
        
        let img = image::open(png_path).expect("Failed to load PNG");
        let img = img.resize(256, 256, image::imageops::FilterType::Lanczos3);
        let (w, h) = img.dimensions();
        
        let mut ico_file = std::fs::File::create(&ico_path).expect("Failed to create ICO file");
        let encoder = image::codecs::ico::IcoEncoder::new(&mut ico_file);
        #[allow(deprecated)]
        encoder.encode(
            img.to_rgba8().as_raw(),
            w,
            h,
            image::ColorType::Rgba8,
        ).expect("Failed to encode ICO");
        
        let mut res = winres::WindowsResource::new();
        res.set_icon(ico_path.to_str().unwrap());
        res.compile().unwrap();
    }
}
