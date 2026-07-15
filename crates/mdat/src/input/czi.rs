use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use czi_rs::CziFile;
use roxmltree::Document;
use serde_json::{json, Value};

use super::types::{axis_size, ImageInfo, MetadataPayload, ReaderAdapter, ReaderSession};
use crate::error::{MdatError, Result};

pub struct CziReaderAdapter;

impl ReaderAdapter for CziReaderAdapter {
    fn name(&self) -> &'static str {
        "czi"
    }

    fn suffixes(&self) -> &'static [&'static str] {
        &[".czi"]
    }

    fn inspect(&self, input_path: &Path) -> Result<ImageInfo> {
        let mut handle = CziFile::open(input_path)?;
        image_info_from_summary(&handle.summary()?)
    }

    fn inspect_metadata(&self, input_path: &Path) -> Result<MetadataPayload> {
        let mut handle = CziFile::open(input_path)?;
        let summary = handle.summary()?;
        let raw = read_raw_metadata_xml(input_path)?;
        let normalized = if raw.trim().is_empty() {
            normalized_metadata_from_summary(&summary)
        } else {
            normalized_metadata_from_xml(&raw, &summary)?
        };

        Ok(MetadataPayload {
            normalized,
            raw: Some(raw),
            raw_format: Some("xml".to_owned()),
        })
    }

    fn open(&self, input_path: &Path) -> Result<ReaderSession> {
        let mut handle = CziFile::open(input_path)?;
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

fn image_info_from_summary(summary: &czi_rs::DatasetSummary) -> Result<ImageInfo> {
    let n_pos = {
        let scenes = axis_size(&summary.sizes, "S");
        if scenes > 1 {
            scenes
        } else {
            1
        }
    };

    Ok(ImageInfo {
        n_pos,
        n_time: axis_size(&summary.sizes, "T"),
        n_chan: axis_size(&summary.sizes, "C"),
        n_z: axis_size(&summary.sizes, "Z"),
    })
}

fn normalized_metadata_from_summary(summary: &czi_rs::DatasetSummary) -> Value {
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
            "x": scaling_to_um(scaling.x, scaling.unit.as_deref()),
            "y": scaling_to_um(scaling.y, scaling.unit.as_deref()),
            "z": scaling_to_um(scaling.z, scaling.unit.as_deref()),
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
            "software": null,
            "software_version": null,
            "microscope": null,
            "microscope_system": null,
            "time_increment_configured_s": null,
            "time_increment_s": null,
            "channel_count": channels.len(),
            "primary_channel": channels.first().and_then(|channel| channel.get("name")).cloned(),
        },
        "dimensions": {
            "size_x": axis_size(&summary.sizes, "X"),
            "size_y": axis_size(&summary.sizes, "Y"),
            "size_z": axis_size(&summary.sizes, "Z"),
            "size_c": axis_size(&summary.sizes, "C"),
            "size_t": axis_size(&summary.sizes, "T"),
            "size_p": if axis_size(&summary.sizes, "S") > 1 {
                axis_size(&summary.sizes, "S")
            } else {
                1
            },
            "pixel_type": summary.pixel_type,
        },
    })
}

fn normalized_metadata_from_xml(raw: &str, summary: &czi_rs::DatasetSummary) -> Result<Value> {
    let mut value = normalized_metadata_from_summary(summary);
    let doc = Document::parse(raw).map_err(|err| {
        MdatError::InvalidInput(format!("failed to parse CZI metadata XML: {err}"))
    })?;
    let root = doc.root_element();

    if let Some(information) = find_path(root, &["Metadata", "Information"]) {
        let image_node = child(information, "Image");
        let document_node = child(information, "Document");
        let application_node = child(information, "Application");
        let instrument_node = child(information, "Instrument");

        let channels = parse_channels(image_node, &summary);
        let pixel_size = parse_pixel_size(root);
        let objective = parse_objective(instrument_node, image_node);
        let size_t = axis_size(&summary.sizes, "T");
        let acquisition = parse_acquisition(
            image_node,
            document_node,
            application_node,
            instrument_node,
            root,
            size_t,
            channels.len(),
        );

        value["channels"] = Value::Array(channels);
        value["pixel_size_um"] = pixel_size;
        value["objective"] = objective;
        value["acquisition"] = acquisition;
    }

    Ok(value)
}

fn parse_channels(image_node: Option<roxmltree::Node<'_, '_>>, summary: &czi_rs::DatasetSummary) -> Vec<Value> {
    let display_by_id = parse_display_channels(image_node);
    let image_channels = image_node
        .and_then(|node| find_path(node, &["Dimensions", "Channels"]))
        .map(|node| {
            node.children()
                .filter(|child| child.is_element() && child.tag_name().name() == "Channel")
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if image_channels.is_empty() {
        return summary
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
    }

    image_channels
        .into_iter()
        .enumerate()
        .map(|(read_index, channel)| {
            let display = channel
                .attribute("Id")
                .and_then(|id| display_by_id.get(id).copied());
            json!({
                "index": read_index,
                "id": channel.attribute("Id"),
                "name": channel.attribute("Name").or(display.and_then(|node| node.attribute("Name"))),
                "color": child_text(channel, "Color").or_else(|| display.and_then(|node| child_text(node, "Color"))),
                "fluor": child_text(channel, "Fluor").or_else(|| display.and_then(|node| child_text(node, "DyeName"))),
                "excitation_nm": child_text(channel, "ExcitationWavelength").and_then(|value| value.parse::<f64>().ok()),
                "emission_nm": child_text(channel, "EmissionWavelength").and_then(|value| value.parse::<f64>().ok()),
                "detection_range_nm": child(channel, "DetectionWavelength")
                    .and_then(|node| child_text(node, "Ranges")),
                "pixel_type": child_text(channel, "PixelType"),
                "acquisition_mode": child_text(channel, "AcquisitionMode"),
                "illumination_type": child_text(channel, "IlluminationType"),
            })
        })
        .collect()
}

fn parse_display_channels<'a>(
    image_node: Option<roxmltree::Node<'a, 'a>>,
) -> std::collections::BTreeMap<String, roxmltree::Node<'a, 'a>> {
    let mut out = std::collections::BTreeMap::new();
    let Some(image_node) = image_node else {
        return out;
    };
    if let Some(display) = find_path(image_node, &["DisplaySetting", "Channels"]) {
        for channel in display
            .children()
            .filter(|child| child.is_element() && child.tag_name().name() == "Channel")
        {
            if let Some(id) = channel.attribute("Id") {
                out.insert(id.to_owned(), channel);
            }
        }
    }
    out
}

fn parse_pixel_size(root: roxmltree::Node<'_, '_>) -> Value {
    let mut pixel_size = json!({ "x": null, "y": null, "z": null });
    if let Some(scaling) = find_path(root, &["Metadata", "Scaling"]) {
        let items = child(scaling, "Items").unwrap_or(scaling);
        for distance in items
            .children()
            .filter(|child| child.is_element() && child.tag_name().name() == "Distance")
        {
            let value_m = child_text(distance, "Value").and_then(|value| value.parse::<f64>().ok());
            let axis = distance
                .attribute("Id")
                .map(|value| value.trim().to_ascii_lowercase());
            if let (Some(axis), Some(value_m)) = (axis.as_deref(), value_m) {
                if axis == "x" || axis == "y" || axis == "z" {
                    pixel_size[axis] = json!(value_m * 1_000_000.0);
                }
            }
        }
    }
    pixel_size
}

fn parse_objective(
    instrument_node: Option<roxmltree::Node<'_, '_>>,
    image_node: Option<roxmltree::Node<'_, '_>>,
) -> Value {
    let objective = instrument_node
        .and_then(|node| child(node, "Objectives"))
        .and_then(|node| child(node, "Objective"));
    let microscope_settings = image_node.and_then(|node| child(node, "ObjectiveSettings"));
    json!({
        "name": objective.and_then(|node| node.attribute("Name")),
        "magnification": objective
            .and_then(|node| child_text(node, "NominalMagnification"))
            .and_then(|value| value.parse::<f64>().ok()),
        "numerical_aperture": objective
            .and_then(|node| child_text(node, "LensNA"))
            .and_then(|value| value.parse::<f64>().ok()),
        "immersion": objective
            .and_then(|node| child_text(node, "Immersion"))
            .or_else(|| microscope_settings.and_then(|node| child_text(node, "Medium"))),
        "refractive_index": objective
            .and_then(|node| child_text(node, "ImmersionRefractiveIndex"))
            .and_then(|value| value.parse::<f64>().ok())
            .or_else(|| {
                microscope_settings
                    .and_then(|node| child_text(node, "RefractiveIndex"))
                    .and_then(|value| value.parse::<f64>().ok())
            }),
    })
}

fn parse_acquisition(
    image_node: Option<roxmltree::Node<'_, '_>>,
    document_node: Option<roxmltree::Node<'_, '_>>,
    application_node: Option<roxmltree::Node<'_, '_>>,
    instrument_node: Option<roxmltree::Node<'_, '_>>,
    root: roxmltree::Node<'_, '_>,
    size_t: usize,
    channel_count: usize,
) -> Value {
    let microscope = instrument_node
        .and_then(|node| child(node, "Microscopes"))
        .and_then(|node| child(node, "Microscope"));
    let primary_channel = image_node
        .and_then(|node| find_path(node, &["Dimensions", "Channels"]))
        .and_then(|node| {
            node.children()
                .find(|child| child.is_element() && child.tag_name().name() == "Channel")
        })
        .and_then(|channel| channel.attribute("Name").map(|value| value.to_owned()));

    json!({
        "datetime": image_node.and_then(|node| child_text(node, "AcquisitionDateAndTime")),
        "creation_datetime": document_node.and_then(|node| child_text(node, "CreationDate")),
        "software": application_node.and_then(|node| child_text(node, "Name")),
        "software_version": application_node.and_then(|node| child_text(node, "Version")),
        "microscope": microscope.and_then(|node| node.attribute("Name")),
        "microscope_system": microscope.and_then(|node| child_text(node, "System")),
        "time_increment_configured_s": time_increment_configured_s(root, size_t),
        "time_increment_s": image_node.and_then(|node| time_increment_s(node, size_t)),
        "channel_count": channel_count,
        "primary_channel": primary_channel,
    })
}

fn time_increment_configured_s(root: roxmltree::Node<'_, '_>, size_t: usize) -> Option<f64> {
    if size_t <= 1 {
        return None;
    }

    for block in acquisition_blocks(root) {
        let setups = child(block, "SubDimensionSetups")?;
        let time_series = child(setups, "TimeSeriesSetup")?;
        if !is_activated(time_series) {
            continue;
        }
        let interval = child(time_series, "Interval")?;
        let timespan = child(interval, "TimeSpan")?;
        let value = child_text(timespan, "Value")?;
        let seconds = timespan_to_seconds(
            &value,
            child_text(timespan, "DefaultUnitFormat").as_deref(),
        )?;
        if seconds > 0.0 {
            return Some(seconds);
        }
    }

    None
}

fn time_increment_s(image_node: roxmltree::Node<'_, '_>, size_t: usize) -> Option<f64> {
    if size_t <= 1 {
        return None;
    }

    let t_dimension = child(child(image_node, "Dimensions")?, "T")?;
    let positions = child(t_dimension, "Positions")?;
    let interval = child(positions, "Interval")?;
    let increment = child_text(interval, "Increment")?.parse::<f64>().ok()?;
    if increment > 0.0 {
        Some(increment)
    } else {
        None
    }
}

fn acquisition_blocks<'a>(root: roxmltree::Node<'a, 'a>) -> Vec<roxmltree::Node<'a, 'a>> {
    let Some(blocks) = find_path(root, &["Metadata", "Experiment", "ExperimentBlocks"]) else {
        return Vec::new();
    };

    if let Some(block) = child(blocks, "AcquisitionBlock") {
        return vec![block];
    }

    blocks
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "AcquisitionBlock")
        .collect()
}

fn scaling_to_um(value: Option<f64>, unit: Option<&str>) -> Option<f64> {
    let value = value?;
    match unit.map(str::to_ascii_lowercase).as_deref() {
        Some("m") | Some("meter") | Some("meters") => Some(value * 1_000_000.0),
        Some("um") | Some("µm") | Some("micrometer") | Some("micrometers") => Some(value),
        Some("mm") | Some("millimeter") | Some("millimeters") => Some(value * 1_000.0),
        Some("nm") | Some("nanometer") | Some("nanometers") => Some(value / 1_000.0),
        _ => Some(value * 1_000_000.0),
    }
}

fn timespan_to_seconds(value: &str, unit: Option<&str>) -> Option<f64> {
    let value = value.parse::<f64>().ok()?;
    match unit.unwrap_or("s").trim().to_ascii_lowercase().as_str() {
        "s" | "sec" | "second" | "seconds" => Some(value),
        "ms" | "millisecond" | "milliseconds" => Some(value / 1_000.0),
        "us" | "µs" | "microsecond" | "microseconds" => Some(value / 1_000_000.0),
        "min" | "minute" | "minutes" => Some(value * 60.0),
        "h" | "hour" | "hours" => Some(value * 3600.0),
        _ => Some(value),
    }
}

fn is_activated(node: roxmltree::Node<'_, '_>) -> bool {
    node.attribute("IsActivated")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn find_path<'a, 'input>(
    start: roxmltree::Node<'a, 'input>,
    path: &[&str],
) -> Option<roxmltree::Node<'a, 'input>> {
    let mut current = Some(start);
    for name in path {
        current = current.and_then(|node| child(node, name));
    }
    current
}

fn child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn child_text<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<String> {
    child(node, name)
        .and_then(|child| child.text())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

pub fn read_raw_metadata_xml(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let header = read_exact_at(&mut file, 0, 512)?;
    let metadata_position = u64::from_le_bytes(header[60..68].try_into().unwrap());
    if metadata_position == 0 {
        return Ok(String::new());
    }

    let fixed = read_exact_at(&mut file, metadata_position + 32, 256)?;
    let xml_size = u32::from_le_bytes(fixed[0..4].try_into().unwrap()) as usize;
    if xml_size == 0 {
        return Ok(String::new());
    }

    let xml_offset = metadata_position + 32 + 256;
    let xml_bytes = read_exact_at(&mut file, xml_offset, xml_size)?;
    String::from_utf8(xml_bytes).map_err(|err| {
        MdatError::InvalidInput(format!("CZI metadata XML is not valid UTF-8: {err}"))
    })
}

fn read_exact_at(file: &mut File, offset: u64, size: usize) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0u8; size];
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::{time_increment_configured_s, time_increment_s, timespan_to_seconds};
    use roxmltree::Document;

    #[test]
    fn configured_time_increment_from_xml() {
        let xml = r#"
        <ImageDocument>
          <Metadata>
            <Experiment>
              <ExperimentBlocks>
                <AcquisitionBlock IsActivated="true">
                  <SubDimensionSetups>
                    <TimeSeriesSetup IsActivated="true">
                      <Interval>
                        <TimeSpan>
                          <Value>5</Value>
                          <DefaultUnitFormat>s</DefaultUnitFormat>
                        </TimeSpan>
                      </Interval>
                    </TimeSeriesSetup>
                  </SubDimensionSetups>
                </AcquisitionBlock>
              </ExperimentBlocks>
            </Experiment>
          </Metadata>
        </ImageDocument>
        "#;
        let doc = Document::parse(xml).unwrap();
        assert_eq!(time_increment_configured_s(doc.root_element(), 250), Some(5.0));
    }

    #[test]
    fn measured_time_increment_from_xml() {
        let xml = r#"
        <Image>
          <Dimensions>
            <T>
              <Positions>
                <Interval>
                  <Increment>5.000024096385542</Increment>
                </Interval>
              </Positions>
            </T>
          </Dimensions>
        </Image>
        "#;
        let doc = Document::parse(xml).unwrap();
        assert_eq!(
            time_increment_s(doc.root_element(), 250),
            Some(5.000024096385542)
        );
    }

    #[test]
    fn timespan_units() {
        assert_eq!(timespan_to_seconds("5", Some("s")), Some(5.0));
        assert_eq!(timespan_to_seconds("5000", Some("ms")), Some(5.0));
    }
}
