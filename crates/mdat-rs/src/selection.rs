use crate::error::Result;
use crate::input::ImageInfo;
use crate::slices::parse_slice_string;

#[derive(Debug, Clone)]
pub struct ConvertSelection {
    pub pos_indices: Vec<usize>,
    pub time_indices: Vec<usize>,
    pub channel_indices: Vec<usize>,
    pub z_indices: Vec<usize>,
}

pub fn resolve_selection(
    info: &ImageInfo,
    position_slice: &str,
    time_slice: &str,
    channel_slice: &str,
    z_slice: &str,
) -> Result<ConvertSelection> {
    Ok(ConvertSelection {
        pos_indices: parse_slice_string(position_slice, info.n_pos)?,
        time_indices: parse_slice_string(time_slice, info.n_time)?,
        channel_indices: parse_slice_string(channel_slice, info.n_chan)?,
        z_indices: parse_slice_string(z_slice, info.n_z)?,
    })
}
