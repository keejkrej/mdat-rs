use std::path::Path;

use crate::error::Result;
use crate::input::{open_reader, ImageInfo};
use crate::output::{OutputFormat, ProgressCallback};
use crate::selection::resolve_selection;

pub fn run_convert(
    input_path: &Path,
    position_slice: &str,
    time_slice: &str,
    channel_slice: &str,
    z_slice: &str,
    output: &Path,
    output_format: OutputFormat,
    on_progress: &mut Option<&mut dyn ProgressCallback>,
) -> Result<()> {
    let mut session = open_reader(input_path)?;
    let selection = resolve_selection(
        &session.info,
        position_slice,
        time_slice,
        channel_slice,
        z_slice,
    )?;
    let info = session.info;
    let result = output_format.run_convert(
        input_path,
        output,
        &selection,
        &info,
        &mut session,
        on_progress,
    );
    session.close()?;
    result
}

pub fn preview_selection(
    info: &ImageInfo,
    position_slice: &str,
    time_slice: &str,
    channel_slice: &str,
    z_slice: &str,
) -> Result<(ConvertSelectionSummary, crate::selection::ConvertSelection)> {
    let selection = resolve_selection(info, position_slice, time_slice, channel_slice, z_slice)?;
    Ok((
        ConvertSelectionSummary {
            pos_count: selection.pos_indices.len(),
            time_count: selection.time_indices.len(),
            channel_count: selection.channel_indices.len(),
            z_count: selection.z_indices.len(),
            total_frames: selection.pos_indices.len()
                * selection.time_indices.len()
                * selection.channel_indices.len()
                * selection.z_indices.len(),
        },
        selection,
    ))
}

#[derive(Debug, Clone, Copy)]
pub struct ConvertSelectionSummary {
    pub pos_count: usize,
    pub time_count: usize,
    pub channel_count: usize,
    pub z_count: usize,
    pub total_frames: usize,
}
