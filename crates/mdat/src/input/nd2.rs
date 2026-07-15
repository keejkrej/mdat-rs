use std::path::Path;

use nd2_rs::Nd2File;
use serde_json::{json, Value};

use super::types::{axis_size, ImageInfo, MetadataPayload, ReaderAdapter, ReaderSession};
use crate::error::{MdatError, Result};

pub struct Nd2ReaderAdapter;

impl ReaderAdapter for Nd2ReaderAdapter {
    fn name(&self) -> &'static str {
        "nd2"
    }

    fn suffixes(&self) -> &'static [&'static str] {
        &[".nd2"]
    }

    fn inspect(&self, input_path: &Path) -> Result<ImageInfo> {
        let mut handle = Nd2File::open(input_path)?;
        image_info_from_summary(&handle.summary()?)
    }

    fn inspect_metadata(&self, input_path: &Path) -> Result<MetadataPayload> {
        let mut handle = Nd2File::open(input_path)?;
        let summary = handle.summary()?;
        Ok(MetadataPayload {
            normalized: normalized_metadata(&summary),
            raw: None,
            raw_format: Some("ome_xml".to_owned()),
        })
    }

    fn open(&self, input_path: &Path) -> Result<ReaderSession> {
        let mut handle = Nd2File::open(input_path)?;
        let summary = handle.summary()?;
        let info = image_info_from_summary(&summary)?;
        let width = axis_size(&summary.sizes, "X") as u32;
        let height = axis_size(&summary.sizes, "Y") as u32;

        Ok(ReaderSession::new(
            info,
            width,
            height,
            move |p, t, c, z| handle.read_frame_2d(p, t, c, z).map_err(MdatError::from),
            move || Ok(()),
        ))
    }
}

fn image_info_from_summary(summary: &nd2_rs::DatasetSummary) -> Result<ImageInfo> {
    Ok(ImageInfo {
        n_pos: axis_size(&summary.sizes, "P"),
        n_time: axis_size(&summary.sizes, "T"),
        n_chan: axis_size(&summary.sizes, "C"),
        n_z: axis_size(&summary.sizes, "Z"),
    })
}

fn normalized_metadata(summary: &nd2_rs::DatasetSummary) -> Value {
    let channels: Vec<Value> = summary
        .channels
        .iter()
        .map(|channel| {
            json!({
                "index": channel.index,
                "id": null,
                "name": channel.name,
                "color": channel.color,
                "fluor": channel.name,
                "excitation_nm": null,
                "emission_nm": null,
                "detection_range_nm": null,
                "pixel_type": channel.pixel_type,
                "acquisition_mode": null,
                "illumination_type": null,
            })
        })
        .collect();

    let pixel_size = summary.scaling.as_ref().map(|scaling| {
        json!({
            "x": scaling.x,
            "y": scaling.y,
            "z": scaling.z,
        })
    });

    json!({
        "channels": channels,
        "pixel_size_um": pixel_size.unwrap_or(json!({ "x": null, "y": null, "z": null })),
        "objective": {
            "name": null,
            "magnification": null,
            "numerical_aperture": null,
            "immersion": null,
            "refractive_index": null,
        },
        "acquisition": {
            "datetime": null,
            "creation_datetime": null,
            "software": "NIS-Elements",
            "software_version": null,
            "microscope": null,
            "microscope_system": null,
            "time_increment_configured_s": null,
            "time_increment_s": null,
            "channel_count": channels.len(),
            "primary_channel": channels.first().and_then(|channel| channel.get("name")).cloned(),
            "z_step_um": null,
        },
        "dimensions": {
            "size_x": axis_size(&summary.sizes, "X"),
            "size_y": axis_size(&summary.sizes, "Y"),
            "size_z": axis_size(&summary.sizes, "Z"),
            "size_c": axis_size(&summary.sizes, "C"),
            "size_t": axis_size(&summary.sizes, "T"),
            "size_p": axis_size(&summary.sizes, "P"),
            "pixel_type": summary.pixel_type,
        },
    })
}
