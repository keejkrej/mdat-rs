use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

use crate::error::Result;
use crate::input::ImageInfo;
use crate::input::ReaderSession;
use crate::io::write_multipage_tiff;
use crate::metadata::collect_metadata;
use crate::output::{emit_progress, OutputFormatWriter, ProgressPhase};
use crate::selection::ConvertSelection;

pub struct AcdcOutputFormat;

impl OutputFormatWriter for AcdcOutputFormat {
    fn name(&self) -> &'static str {
        "acdc"
    }

    fn position_label(&self, p_idx: usize) -> String {
        format!("Position_{}", p_idx + 1)
    }

    fn run_convert(
        &self,
        input_path: &Path,
        output: &Path,
        selection: &ConvertSelection,
        info: &ImageInfo,
        session: &mut ReaderSession,
        on_progress: &mut Option<&mut dyn crate::output::ProgressCallback>,
    ) -> Result<()> {
        let total = selection.pos_indices.len()
            * selection.time_indices.len()
            * selection.channel_indices.len()
            * selection.z_indices.len();

        emit_progress(
            on_progress,
            ProgressPhase::Start,
            0,
            total,
            format!(
                "Selected {} positions, {} timepoints, {} channels, {} z-slices. Total frames: {total}",
                selection.pos_indices.len(),
                selection.time_indices.len(),
                selection.channel_indices.len(),
                selection.z_indices.len(),
            ),
        );

        std::fs::create_dir_all(output)?;
        let file_metadata = collect_metadata(input_path)?;
        let normalized = file_metadata
            .get("normalized")
            .cloned()
            .unwrap_or(Value::Null);
        let channels = normalized
            .get("channels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let channel_labels = channel_labels_for(&selection.channel_indices, &channels);
        let metadata_fields = acdc_metadata_fields(&normalized);
        let num_pos_digits = info.n_pos.to_string().len().max(1);
        let size_t = selection.time_indices.len();
        let size_z = selection.z_indices.len();

        let mut done = 0usize;
        for &p_idx in &selection.pos_indices {
            let images_dir = output
                .join(format!("Position_{}", p_idx + 1))
                .join("Images");
            std::fs::create_dir_all(&images_dir)?;
            let basename = acdc_basename(input_path, p_idx, num_pos_digits);

            write_acdc_metadata_csv(
                &images_dir.join(format!("{basename}metadata.csv")),
                &basename,
                size_t,
                size_z,
                &selection.channel_indices,
                &channel_labels,
                &metadata_fields,
            )?;

            for &c_orig in &selection.channel_indices {
                let mut pages = Vec::new();
                for &t_orig in &selection.time_indices {
                    for &z_orig in &selection.z_indices {
                        let frame = session.read_frame(p_idx, t_orig, c_orig, z_orig)?;
                        pages.push(frame);
                        done += 1;
                        emit_progress(
                            on_progress,
                            ProgressPhase::Advance,
                            done,
                            total,
                            "Writing Cell-ACDC TIFFs",
                        );
                    }
                }

                let filename = format!("{basename}{}", channel_labels[&c_orig]);
                write_multipage_tiff(
                    &images_dir.join(filename),
                    &pages,
                    session.width,
                    session.height,
                )?;
            }
        }

        emit_progress(
            on_progress,
            ProgressPhase::Finish,
            done,
            total,
            format!("Wrote {}", output.display()),
        );
        Ok(())
    }
}

fn sanitize_label(value: &str, fallback: &str) -> String {
    static INVALID: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let invalid = INVALID.get_or_init(|| Regex::new(r#"[<>:"/\\|?*\x00-\x1f]"#).unwrap());
    let normalized = value.trim().replace('.', "_");
    let cleaned = invalid.replace_all(normalized.as_ref(), "_");
    let cleaned = cleaned
        .trim_matches(|character: char| "._ ".contains(character))
        .to_string();
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

fn channel_for_read_index<'a>(channels: &'a [Value], c_orig: usize) -> Option<&'a Value> {
    channels
        .iter()
        .find(|channel| channel.get("index").and_then(Value::as_u64) == Some(c_orig as u64))
        .or_else(|| channels.get(c_orig))
}

fn channel_label(channel: Option<&Value>, fallback: &str) -> String {
    let name = channel
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or(fallback);
    sanitize_label(name, fallback)
}

fn channel_labels_for(channel_indices: &[usize], channels: &[Value]) -> HashMap<usize, String> {
    channel_indices
        .iter()
        .map(|&c_orig| {
            let fallback = format!("channel_{c_orig:03}");
            let channel = channel_for_read_index(channels, c_orig);
            (c_orig, channel_label(channel, &fallback))
        })
        .collect()
}

fn acdc_basename(input_path: &Path, p_idx: usize, num_pos_digits: usize) -> String {
    let stem = input_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let stem = sanitize_label(stem, "image");
    format!("{stem}_s{:0width$}_", p_idx + 1, width = num_pos_digits)
}

fn acdc_metadata_fields(normalized: &Value) -> HashMap<String, Option<f64>> {
    let pixel_size = normalized.get("pixel_size_um");
    let objective = normalized.get("objective");
    let acquisition = normalized.get("acquisition");
    HashMap::from([
        (
            "pixel_size_x".to_owned(),
            pixel_size
                .and_then(|value| value.get("x"))
                .and_then(Value::as_f64),
        ),
        (
            "pixel_size_y".to_owned(),
            pixel_size
                .and_then(|value| value.get("y"))
                .and_then(Value::as_f64),
        ),
        (
            "pixel_size_z".to_owned(),
            pixel_size
                .and_then(|value| value.get("z"))
                .and_then(Value::as_f64),
        ),
        (
            "lens_na".to_owned(),
            objective
                .and_then(|value| value.get("numerical_aperture"))
                .and_then(Value::as_f64),
        ),
        (
            "time_increment".to_owned(),
            acquisition
                .and_then(|value| value.get("time_increment_s"))
                .and_then(Value::as_f64),
        ),
        (
            "time_increment_configured".to_owned(),
            acquisition
                .and_then(|value| value.get("time_increment_configured_s"))
                .and_then(Value::as_f64),
        ),
    ])
}

fn write_acdc_metadata_csv(
    path: &Path,
    basename: &str,
    size_t: usize,
    size_z: usize,
    channel_indices: &[usize],
    channel_labels: &HashMap<usize, String>,
    metadata_fields: &HashMap<String, Option<f64>>,
) -> Result<()> {
    let rows = vec![
        ("LensNA", metadata_fields.get("lens_na").copied().flatten()),
        ("SizeT", Some(size_t as f64)),
        ("SizeZ", Some(size_z as f64)),
        (
            "TimeIncrement",
            metadata_fields.get("time_increment").copied().flatten(),
        ),
        (
            "TimeIncrementConfigured",
            metadata_fields
                .get("time_increment_configured")
                .copied()
                .flatten(),
        ),
        (
            "PhysicalSizeZ",
            metadata_fields.get("pixel_size_z").copied().flatten(),
        ),
        (
            "PhysicalSizeY",
            metadata_fields.get("pixel_size_y").copied().flatten(),
        ),
        (
            "PhysicalSizeX",
            metadata_fields.get("pixel_size_x").copied().flatten(),
        ),
        ("basename", None),
    ];

    let mut content = String::from("Description,values\n");
    for (description, value) in rows {
        let rendered = match description {
            "basename" => basename.to_owned(),
            _ => value.map(|number| number.to_string()).unwrap_or_default(),
        };
        content.push_str(&format!("{description},{rendered}\n"));
    }

    for (c_idx, &c_orig) in channel_indices.iter().enumerate() {
        let label = channel_labels.get(&c_orig).cloned().unwrap_or_default();
        content.push_str(&format!("channel_{c_idx}_name,{label}\n"));
    }

    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{acdc_basename, channel_labels_for, sanitize_label};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn sanitize_label_replaces_invalid_chars() {
        assert_eq!(sanitize_label("GFP.channel", "ch0"), "GFP_channel");
        assert_eq!(sanitize_label("  ", "ch0"), "ch0");
    }

    #[test]
    fn acdc_basename_uses_position_number() {
        assert_eq!(
            acdc_basename(Path::new("experiment/sample.nd2"), 0, 2),
            "sample_s01_"
        );
    }

    #[test]
    fn channel_labels_use_read_index() {
        let channels = vec![
            json!({"index": 0, "name": "RhodB-T1"}),
            json!({"index": 1, "name": "AF405-T2"}),
        ];
        let labels = channel_labels_for(&[0, 1], &channels);
        assert_eq!(labels.get(&0), Some(&"RhodB-T1".to_owned()));
        assert_eq!(labels.get(&1), Some(&"AF405-T2".to_owned()));
    }
}
