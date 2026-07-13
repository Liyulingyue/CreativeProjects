use std::path::PathBuf;

pub const FOLDER_CACHE_DIR_NAME: &str = ".photoanalyzer";
pub const FEATURES_DIR_NAME: &str = "features";

pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
}

pub fn thumbs_dir() -> PathBuf {
    data_dir().join("thumbs")
}

pub fn features_dir() -> PathBuf {
    data_dir().join(FEATURES_DIR_NAME)
}