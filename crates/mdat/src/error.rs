use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MdatError>;

#[derive(Debug, Error)]
pub enum MdatError {
    #[error(transparent)]
    Nd2(#[from] nd2_rs::Nd2Error),
    #[error(transparent)]
    Czi(#[from] czi_rs::CziError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Tiff(#[from] tiff::TiffError),
    #[error("unsupported input file format: {suffix}")]
    UnsupportedFormat { suffix: String },
    #[error("{0}")]
    InvalidInput(String),
    #[error("no raw metadata export is available for {format}")]
    RawMetadataUnavailable { format: String },
    #[error("input file not found: {path}")]
    InputNotFound { path: PathBuf },
}
