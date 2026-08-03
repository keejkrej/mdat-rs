use std::path::Path;

use serde_json::Value;

use crate::error::{MdatError, Result};
use crate::input::MetadataPayload;
use crate::input::registry::resolve_reader_adapter;

pub fn collect_metadata(input_path: &Path) -> Result<Value> {
    let adapter = resolve_reader_adapter(input_path)?;
    let info = adapter.inspect(input_path)?;
    let payload = adapter.inspect_metadata(input_path)?;

    Ok(serde_json::json!({
        "source": input_path.display().to_string(),
        "format": adapter.name(),
        "summary": {
            "n_pos": info.n_pos,
            "n_time": info.n_time,
            "n_chan": info.n_chan,
            "n_z": info.n_z,
        },
        "normalized": payload.normalized,
        "raw_format": payload.raw_format,
    }))
}

pub fn collect_raw_metadata(input_path: &Path) -> Result<MetadataPayload> {
    let adapter = resolve_reader_adapter(input_path)?;
    let payload = adapter.inspect_metadata(input_path)?;
    if payload.raw.is_none() {
        return Err(MdatError::RawMetadataUnavailable {
            format: adapter.name().to_ascii_uppercase(),
        });
    }
    Ok(payload)
}

pub fn render_metadata_json(input_path: &Path) -> Result<String> {
    let value = collect_metadata(input_path)?;
    let mut serialized = serde_json::to_string_pretty(&value)?;
    serialized.push('\n');
    Ok(serialized)
}

pub fn run_metadata(
    input_path: &Path,
    output: Option<&Path>,
    raw: bool,
) -> Result<String> {
    let content = if raw {
        let payload = collect_raw_metadata(input_path)?;
        payload.raw.unwrap_or_default()
    } else {
        render_metadata_json(input_path)?
    };

    if let Some(output_path) = output {
        std::fs::write(output_path, &content)?;
    }

    Ok(content)
}
