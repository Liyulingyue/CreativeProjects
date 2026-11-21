use thiserror::Error;

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("xml: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BridgeError>;
