use std::path::Path;

use serde_json::Value;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub n_pos: usize,
    pub n_time: usize,
    pub n_chan: usize,
    pub n_z: usize,
}

#[derive(Debug, Clone)]
pub struct MetadataPayload {
    pub normalized: Value,
    pub raw: Option<String>,
    pub raw_format: Option<String>,
}

pub struct ReaderSession {
    pub info: ImageInfo,
    pub width: u32,
    pub height: u32,
    read_frame: Box<dyn FnMut(usize, usize, usize, usize) -> Result<Vec<u16>>>,
    close: Box<dyn FnMut() -> Result<()>>,
}

impl ReaderSession {
    pub fn new(
        info: ImageInfo,
        width: u32,
        height: u32,
        read_frame: impl FnMut(usize, usize, usize, usize) -> Result<Vec<u16>> + 'static,
        close: impl FnMut() -> Result<()> + 'static,
    ) -> Self {
        Self {
            info,
            width,
            height,
            read_frame: Box::new(read_frame),
            close: Box::new(close),
        }
    }

    pub fn read_frame(&mut self, p: usize, t: usize, c: usize, z: usize) -> Result<Vec<u16>> {
        (self.read_frame)(p, t, c, z)
    }

    pub fn close(&mut self) -> Result<()> {
        (self.close)()
    }
}

pub trait ReaderAdapter {
    fn name(&self) -> &'static str;
    fn suffixes(&self) -> &'static [&'static str];
    fn inspect(&self, input_path: &Path) -> Result<ImageInfo>;
    fn inspect_metadata(&self, input_path: &Path) -> Result<MetadataPayload>;
    fn open(&self, input_path: &Path) -> Result<ReaderSession>;
}

pub fn axis_size(sizes: &std::collections::BTreeMap<String, usize>, axis: &str) -> usize {
    sizes.get(axis).copied().unwrap_or(1).max(1)
}