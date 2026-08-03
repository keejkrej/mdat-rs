use std::path::Path;

use crate::error::{MdatError, Result};
use crate::input::ReaderSession;
use crate::selection::ConvertSelection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressEvent {
    pub phase: ProgressPhase,
    pub done: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    Start,
    Advance,
    Finish,
}

pub trait ProgressCallback {
    fn on_progress(&mut self, event: ProgressEvent);
}

pub fn emit_progress(
    callback: &mut Option<&mut dyn ProgressCallback>,
    phase: ProgressPhase,
    done: usize,
    total: usize,
    message: impl Into<String>,
) {
    if let Some(callback) = callback.as_deref_mut() {
        callback.on_progress(ProgressEvent {
            phase,
            done,
            total,
            message: message.into(),
        });
    }
}

pub trait OutputFormatWriter {
    fn name(&self) -> &'static str;

    fn position_label(&self, p_idx: usize) -> String;

    fn run_convert(
        &self,
        input_path: &Path,
        output: &Path,
        selection: &ConvertSelection,
        info: &crate::input::ImageInfo,
        session: &mut ReaderSession,
        on_progress: &mut Option<&mut dyn ProgressCallback>,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Mdat,
    Acdc,
}

impl OutputFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mdat" => Ok(Self::Mdat),
            "acdc" => Ok(Self::Acdc),
            _ => Err(MdatError::InvalidInput(format!(
                "unsupported output format {value:?}. Expected \"mdat\" or \"acdc\"."
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mdat => "mdat",
            Self::Acdc => "acdc",
        }
    }

    pub fn writer(self) -> &'static dyn OutputFormatWriter {
        match self {
            Self::Mdat => &mdat::MdatOutputFormat,
            Self::Acdc => &acdc::AcdcOutputFormat,
        }
    }

    pub fn run_convert(
        self,
        input_path: &Path,
        output: &Path,
        selection: &ConvertSelection,
        info: &crate::input::ImageInfo,
        session: &mut ReaderSession,
        on_progress: &mut Option<&mut dyn ProgressCallback>,
    ) -> Result<()> {
        self.writer()
            .run_convert(input_path, output, selection, info, session, on_progress)
    }

    pub fn position_label(self, p_idx: usize) -> String {
        self.writer().position_label(p_idx)
    }
}

pub fn position_label(p_idx: usize, format: OutputFormat) -> String {
    format.position_label(p_idx)
}

mod acdc;
mod mdat;
