use std::fs::File;
use std::io::Write;
use std::path::Path;

use tiff::encoder::{colortype, TiffEncoder};

use crate::error::Result;

pub fn write_tiff(path: &Path, pixels: &[u16], width: u32, height: u32) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }

    let file = File::create(path)?;
    let mut encoder = TiffEncoder::new(file)?;
    let image = encoder.new_image::<colortype::Gray16>(width, height)?;
    image.write_data(pixels)?;
    Ok(())
}

pub fn write_multipage_tiff(
    path: &Path,
    pages: &[Vec<u16>],
    width: u32,
    height: u32,
) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }

    let file = File::create(path)?;
    let mut encoder = TiffEncoder::new(file)?;
    for page in pages {
        let image = encoder.new_image::<colortype::Gray16>(width, height)?;
        image.write_data(page)?;
    }
    Ok(())
}

pub fn write_text_output(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}
