use std::path::PathBuf;

pub const FOLDER_CACHE_DIR_NAME: &str = ".photoanalyzer";
pub const FEATURES_DIR_NAME: &str = "features";

pub fn data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("PHOTO_ANALYZER_DATA_DIR") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if let Some(portable) = portable_data_dir() {
        return portable;
    }

    platform_data_dir()
}

pub fn thumbs_dir() -> PathBuf {
    data_dir().join("thumbs")
}

pub fn features_dir() -> PathBuf {
    data_dir().join(FEATURES_DIR_NAME)
}

fn portable_data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidate = exe_dir.join("data");
    if std::fs::create_dir_all(&candidate).is_ok() {
        return Some(candidate);
    }
    None
}

fn platform_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("PhotoAnalyzer").join("data");
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".photoanalyzer").join("data");
        }
    }

    std::env::temp_dir().join("photo_analyzer").join("data")
}