use std::path::Path;

use crate::error::Result;
use crate::input::registry::resolve_reader_adapter;
use crate::input::types::{ImageInfo, ReaderSession};

pub fn inspect_input(input_path: &Path) -> Result<ImageInfo> {
    resolve_reader_adapter(input_path)?.inspect(input_path)
}

pub fn open_reader(input_path: &Path) -> Result<ReaderSession> {
    resolve_reader_adapter(input_path)?.open(input_path)
}
