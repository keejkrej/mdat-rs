use std::path::{Path, PathBuf};

use crate::error::{MdatError, Result};
use crate::czi::CziReaderAdapter;
use crate::nd2::Nd2ReaderAdapter;
use crate::tiff_series::{TiffSeriesReaderAdapter, TIFF_SERIES_EXPECTED_PATTERN};
use crate::types::ReaderAdapter;

pub fn location_suffix(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default()
}

pub fn resolve_reader_adapter(path: &Path) -> Result<&'static dyn ReaderAdapter> {
    if path.is_dir() {
        if TiffSeriesReaderAdapter::matches_directory(path) {
            return Ok(&TiffSeriesReaderAdapter);
        }
        return Err(MdatError::UnsupportedFormat {
            suffix: format!("directory (expected mdat convert layout `{TIFF_SERIES_EXPECTED_PATTERN}`)"),
        });
    }
    let suffix = location_suffix(path);
    if TiffSeriesReaderAdapter.suffixes().contains(&suffix.as_str()) {
        return Ok(&TiffSeriesReaderAdapter);
    }
    if Nd2ReaderAdapter.suffixes().contains(&suffix.as_str()) {
        return Ok(&Nd2ReaderAdapter);
    }
    if CziReaderAdapter.suffixes().contains(&suffix.as_str()) {
        return Ok(&CziReaderAdapter);
    }
    Err(MdatError::UnsupportedFormat { suffix })
}

pub fn ensure_input_exists(path: &Path) -> Result<()> {
    if !path.is_file() && !path.is_dir() {
        return Err(MdatError::InputNotFound {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

pub fn open_input(path: &Path) -> Result<PathBuf> {
    ensure_input_exists(path)?;
    resolve_reader_adapter(path)?;
    Ok(path.to_path_buf())
}