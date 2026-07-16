use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{json, Value};
use tiff::decoder::{Decoder, DecodingResult};

use crate::error::{MdatError, Result};
use crate::types::{ImageInfo, MetadataPayload, ReaderAdapter, ReaderSession};

const EXPECTED_PATTERN: &str = "img_channel{c:03}_position{p:03}_time{t:09}_z{z:03}.tif";
pub(crate) const TIFF_SERIES_EXPECTED_PATTERN: &str = EXPECTED_PATTERN;
const DEFAULT_PALETTE: &[&str] = &[
    "#ff0000",
    "#00ff00",
    "#0000ff",
    "#ffff00",
    "#ff00ff",
    "#00ffff",
    "#ff8000",
    "#ff0080",
    "#80ff00",
    "#0080ff",
];

pub struct TiffSeriesReaderAdapter;

impl TiffSeriesReaderAdapter {
    pub(crate) fn matches_directory(dir: &Path) -> bool {
        has_mdat_named_tiffs(dir) || looks_like_out_root(dir)
    }
}

impl ReaderAdapter for TiffSeriesReaderAdapter {
    fn name(&self) -> &'static str {
        "tiff-series"
    }

    fn suffixes(&self) -> &'static [&'static str] {
        &[".tif"]
    }

    fn inspect(&self, input_path: &Path) -> Result<ImageInfo> {
        let resolved = resolve_layout(input_path)?;
        let index = SeriesIndex::scan(&resolved)?;
        index.image_info()
    }

    fn inspect_metadata(&self, input_path: &Path) -> Result<MetadataPayload> {
        let resolved = resolve_layout(input_path)?;
        let index = SeriesIndex::scan(&resolved)?;
        let time_map = read_time_map(&resolved.pos_dir(0));
        let normalized = normalized_metadata(&index, &time_map);
        Ok(MetadataPayload {
            normalized,
            raw: None,
            raw_format: None,
        })
    }

    fn open(&self, input_path: &Path) -> Result<ReaderSession> {
        let resolved = resolve_layout(input_path)?;
        let index = SeriesIndex::scan(&resolved)?;
        let info = index.image_info()?;
        let (width, height) = index.first_frame_dims()?;

        let frames_by_addr: HashMap<(usize, usize, usize, usize), PathBuf> = index
            .frames
            .iter()
            .map(|f| ((f.pos, f.time, f.chan, f.z), f.path.clone()))
            .collect();
        let mut cache: HashMap<(usize, usize, usize, usize), Vec<u16>> = HashMap::new();

        Ok(ReaderSession::new(
            info,
            width,
            height,
            move |p, t, c, z| {
                if let Some(existing) = cache.get(&(p, t, c, z)) {
                    return Ok(existing.clone());
                }
                let path = frames_by_addr
                    .get(&(p, t, c, z))
                    .ok_or_else(|| MdatError::InvalidInput(format!("no frame for (p={p},t={t},c={c},z={z})")))?;
                let file = File::open(path)?;
                let mut decoder = Decoder::new(file)?;
                let result = decoder.read_image()?;
                let pixels = match result {
                    DecodingResult::U16(v) => v,
                    other => {
                        return Err(MdatError::InvalidInput(format!(
                            "expected u16 TIFF plane, got {other:?}"
                        )))
                    }
                };
                cache.insert((p, t, c, z), pixels.clone());
                Ok(pixels)
            },
            move || Ok(()),
        ))
    }
}

#[derive(Clone, Copy, Debug)]
enum ResolvedKind {
    OutRoot,
    PosDir,
    SingleTif,
}

#[derive(Clone, Debug)]
struct ResolvedLayout {
    root: PathBuf,
    kind: ResolvedKind,
}

impl ResolvedLayout {
    fn pos_dir(&self, pos_idx: usize) -> PathBuf {
        match self.kind {
            ResolvedKind::OutRoot => self.root.join(format!("Pos{pos_idx}")),
            ResolvedKind::PosDir | ResolvedKind::SingleTif => self.root.clone(),
        }
    }

    fn enumerate_pos(&self) -> Vec<(usize, PathBuf)> {
        match self.kind {
            ResolvedKind::OutRoot => {
                let mut positions: Vec<(usize, PathBuf)> = std::fs::read_dir(&self.root)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| {
                        let name = entry.file_name();
                        let name = name.to_str()?;
                        if !entry.file_type().ok()?.is_dir() {
                            return None;
                        }
                        let pos_idx = name.strip_prefix("Pos")?;
                        let pos_idx: usize = pos_idx.parse().ok()?;
                        Some((pos_idx, entry.path()))
                    })
                    .collect();
                positions.sort_by_key(|(idx, _)| *idx);
                positions
            }
            ResolvedKind::PosDir | ResolvedKind::SingleTif => vec![(0, self.root.clone())],
        }
    }
}

fn resolve_layout(input_path: &Path) -> Result<ResolvedLayout> {
    if input_path.is_dir() {
        if has_mdat_named_tiffs(input_path) {
            return Ok(ResolvedLayout {
                root: input_path.to_path_buf(),
                kind: ResolvedKind::PosDir,
            });
        }
        if looks_like_out_root(input_path) {
            return Ok(ResolvedLayout {
                root: input_path.to_path_buf(),
                kind: ResolvedKind::OutRoot,
            });
        }
        return Err(MdatError::InvalidInput(format!(
            "directory does not match the mdat convert TIFF-series layout (expected {EXPECTED_PATTERN} under PosN/ children)"
        )));
    }
    let is_tif = input_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("tif"))
        .unwrap_or(false);
    if is_tif {
        let root = input_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| input_path.to_path_buf());
        return Ok(ResolvedLayout {
            root,
            kind: ResolvedKind::SingleTif,
        });
    }
    Err(MdatError::UnsupportedFormat {
        suffix: crate::registry::location_suffix(input_path),
    })
}

fn looks_like_out_root(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            if let Some(rest) = name.strip_prefix("Pos") {
                if rest.parse::<usize>().is_ok() && has_mdat_named_tiffs(&entry.path()) {
                    return true;
                }
            }
        }
    }
    false
}

fn has_mdat_named_tiffs(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let re = filename_regex();
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .map(|name| re.is_match(name))
            .unwrap_or(false)
    })
}

#[derive(Clone, Debug)]
struct SeriesIndex {
    n_pos: usize,
    n_time: usize,
    n_chan: usize,
    n_z: usize,
    frames: Vec<FrameEntry>,
}

#[derive(Clone, Debug)]
struct FrameEntry {
    pos: usize,
    time: usize,
    chan: usize,
    z: usize,
    path: PathBuf,
}

impl SeriesIndex {
    fn scan(resolved: &ResolvedLayout) -> Result<Self> {
        let re = filename_regex();
        let mut frames = Vec::new();
        let mut max_pos = 0usize;
        let mut max_time = 0usize;
        let mut max_chan = 0usize;
        let mut max_z = 0usize;
        let mut saw_any = false;

        for (pos_idx, pos_dir) in resolved.enumerate_pos() {
            let Ok(entries) = std::fs::read_dir(&pos_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = match entry.file_name().to_str() {
                    Some(s) => s.to_owned(),
                    None => continue,
                };
                let Some(caps) = re.captures(&name) else {
                    continue;
                };
                saw_any = true;
                let c: usize = caps.name("c").unwrap().as_str().parse().unwrap();
                let p: usize = caps.name("p").unwrap().as_str().parse().unwrap();
                let t: usize = caps.name("t").unwrap().as_str().parse().unwrap();
                let z: usize = caps.name("z").unwrap().as_str().parse().unwrap();
                max_pos = max_pos.max(p);
                max_time = max_time.max(t);
                max_chan = max_chan.max(c);
                max_z = max_z.max(z);
                frames.push(FrameEntry {
                    pos: p,
                    time: t,
                    chan: c,
                    z,
                    path: entry.path(),
                });
            }
            let _ = pos_idx;
        }

        if !saw_any {
            return Err(MdatError::UnsupportedFormat {
                suffix: EXPECTED_PATTERN.to_string(),
            });
        }

        Ok(SeriesIndex {
            n_pos: max_pos + 1,
            n_time: max_time + 1,
            n_chan: max_chan + 1,
            n_z: max_z + 1,
            frames,
        })
    }

    fn image_info(&self) -> Result<ImageInfo> {
        Ok(ImageInfo {
            n_pos: self.n_pos,
            n_time: self.n_time,
            n_chan: self.n_chan,
            n_z: self.n_z,
        })
    }

    fn first_frame_dims(&self) -> Result<(u32, u32)> {
        let first = self
            .frames
            .first()
            .ok_or_else(|| MdatError::InvalidInput("no frames found".to_string()))?;
        let file = File::open(&first.path)?;
        let mut decoder = Decoder::new(file)?;
        let (w, h) = decoder.dimensions()?;
        Ok((w, h))
    }
}

fn read_time_map(pos_dir: &Path) -> Vec<(usize, usize)> {
    let path = pos_dir.join("time_map.csv");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if i == 0 {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split(',');
        let t: usize = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        let t_orig: usize = parts
            .next()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        out.push((t, t_orig));
    }
    out
}

fn normalized_metadata(index: &SeriesIndex, time_map: &[(usize, usize)]) -> Value {
    let channels: Vec<Value> = (0..index.n_chan)
        .map(|c| {
            let color = DEFAULT_PALETTE.get(c).copied().unwrap_or("#ffffff");
            json!({
                "index": c,
                "id": null,
                "name": format!("Channel {c}"),
                "color": color,
                "fluor": null,
                "excitation_nm": null,
                "emission_nm": null,
                "detection_range_nm": null,
                "pixel_type": "Gray16",
                "acquisition_mode": null,
                "illumination_type": null,
            })
        })
        .collect();

    let time_map_json: Vec<Value> = time_map
        .iter()
        .map(|(t, t_orig)| json!({ "t": t, "t_orig": t_orig }))
        .collect();

    json!({
        "channels": channels,
        "pixel_size_um": { "x": null, "y": null, "z": null },
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
            "software": "mdat convert",
            "software_version": null,
            "microscope": null,
            "microscope_system": null,
            "time_increment_configured_s": null,
            "time_increment_s": null,
            "channel_count": channels.len(),
            "primary_channel": channels.first().and_then(|c| c.get("name")).cloned(),
        },
        "dimensions": {
            "size_x": null,
            "size_y": null,
            "size_z": index.n_z,
            "size_c": index.n_chan,
            "size_t": index.n_time,
            "size_p": index.n_pos,
            "pixel_type": "Gray16",
        },
        "time_map": time_map_json,
    })
}

fn filename_regex() -> Regex {
    Regex::new(
        r"^img_channel(?P<c>\d{3})_position(?P<p>\d{3})_time(?P<t>\d{9})_z(?P<z>\d{3})\.tif$",
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{ensure_input_exists, resolve_reader_adapter};
    use crate::ReaderAdapter;
    use std::fs::File;
    use std::path::Path;
    use std::fs;
    use tiff::encoder::{colortype, TiffEncoder};
    use tempfile::tempdir;

    fn write_tiff(path: &Path, pixels: &[u16], width: u32, height: u32) {
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
        let file = File::create(path).unwrap();
        let mut encoder = TiffEncoder::new(file).unwrap();
        let image = encoder.new_image::<colortype::Gray16>(width, height).unwrap();
        image.write_data(pixels).unwrap();
    }

    const W: u32 = 4;
    const H: u32 = 3;

    fn make_frame(seed: u16) -> Vec<u16> {
        (0..(W * H)).map(|i| (seed + i as u16) % 1000).collect()
    }

    fn write_mdat_tree(root: &Path, n_pos: usize, n_time: usize, n_chan: usize, n_z: usize) {
        for p in 0..n_pos {
            let pos_dir = root.join(format!("Pos{p}"));
            fs::create_dir_all(&pos_dir).unwrap();
            let mut time_map = String::from("t,t_real\n");
            for t in 0..n_time {
                time_map.push_str(&format!("{t},{t}\n"));
            }
            fs::write(pos_dir.join("time_map.csv"), time_map).unwrap();
            for t in 0..n_time {
                for c in 0..n_chan {
                    for z in 0..n_z {
                        let frame = make_frame(((p * 100 + t * 10 + c * 3 + z) as u16) * 7);
                        let filename = format!(
                            "img_channel{c:03}_position{p:03}_time{t:09}_z{z:03}.tif"
                        );
                        write_tiff(&pos_dir.join(filename), &frame, W, H);
                    }
                }
            }
        }
    }

    #[test]
    fn inspect_reports_dims_for_out_root() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 2, 3, 2, 2);
        let info = TiffSeriesReaderAdapter
            .inspect(dir.path())
            .expect("inspect ok");
        assert_eq!(info.n_pos, 2);
        assert_eq!(info.n_time, 3);
        assert_eq!(info.n_chan, 2);
        assert_eq!(info.n_z, 2);
    }

    #[test]
    fn inspect_reports_dims_for_pos_dir() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 1, 2, 1, 2);
        let pos_dir = dir.path().join("Pos0");
        let info = TiffSeriesReaderAdapter.inspect(&pos_dir).expect("inspect ok");
        assert_eq!(info.n_pos, 1);
        assert_eq!(info.n_time, 2);
        assert_eq!(info.n_chan, 1);
        assert_eq!(info.n_z, 2);
    }

    #[test]
    fn inspect_reports_dims_for_single_tif() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 1, 1, 1, 1);
        let file = dir
            .path()
            .join("Pos0")
            .join("img_channel000_position000_time000000000_z000.tif");
        let info = TiffSeriesReaderAdapter.inspect(&file).expect("inspect ok");
        assert_eq!(info.n_pos, 1);
        assert_eq!(info.n_time, 1);
        assert_eq!(info.n_chan, 1);
        assert_eq!(info.n_z, 1);
    }

    #[test]
    fn read_frame_returns_correct_pixels() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 1, 1, 1, 1);
        let expected = make_frame(0);
        let mut session = TiffSeriesReaderAdapter
            .open(dir.path())
            .expect("open ok");
        assert_eq!(session.width, W);
        assert_eq!(session.height, H);
        let frame = session.read_frame(0, 0, 0, 0).expect("read ok");
        assert_eq!(frame.len(), (W * H) as usize);
        assert_eq!(frame, expected);
    }

    #[test]
    fn read_frame_repeats_without_reopen_error() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 1, 2, 1, 1);
        let mut session = TiffSeriesReaderAdapter
            .open(dir.path())
            .expect("open ok");
        let a = session.read_frame(0, 0, 0, 0).expect("read t0");
        let b = session.read_frame(0, 1, 0, 0).expect("read t1");
        let a2 = session.read_frame(0, 0, 0, 0).expect("read t0 again");
        assert_eq!(a, a2);
        assert_ne!(a, b);
    }

    #[test]
    fn read_frame_out_of_range_is_error() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 1, 1, 1, 1);
        let mut session = TiffSeriesReaderAdapter
            .open(dir.path())
            .expect("open ok");
        let err = session.read_frame(0, 99, 0, 0).unwrap_err();
        assert!(err.to_string().contains("no frame"));
    }

    #[test]
    fn read_frame_multi_position_isolation() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 2, 1, 1, 1);
        let mut session = TiffSeriesReaderAdapter
            .open(dir.path())
            .expect("open ok");
        let p0 = session.read_frame(0, 0, 0, 0).expect("read p0");
        let p1 = session.read_frame(1, 0, 0, 0).expect("read p1");
        assert_ne!(p0, p1);
        let p0_again = session.read_frame(0, 0, 0, 0).expect("read p0 again");
        assert_eq!(p0, p0_again);
    }

    #[test]
    fn empty_directory_is_unsupported() {
        let dir = tempdir().unwrap();
        let err = TiffSeriesReaderAdapter.inspect(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(EXPECTED_PATTERN),
            "error should name the expected pattern, got: {msg}"
        );
    }

    #[test]
    fn foreign_directory_is_unsupported() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("random.tif"), b"not a tiff").unwrap();
        let err = TiffSeriesReaderAdapter.inspect(dir.path()).unwrap_err();
        assert!(err.to_string().contains("mdat convert"));
    }

    #[test]
    fn metadata_exposes_time_map_and_default_colors() {
        let dir = tempdir().unwrap();
        write_mdat_tree(dir.path(), 1, 2, 2, 1);
        let payload = TiffSeriesReaderAdapter
            .inspect_metadata(dir.path())
            .expect("metadata ok");
        let channels = payload.normalized["channels"].as_array().unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0]["color"], json!("#ff0000"));
        assert_eq!(channels[1]["color"], json!("#00ff00"));
        let time_map = payload.normalized["time_map"].as_array().unwrap();
        assert_eq!(time_map.len(), 2);
        assert_eq!(time_map[0]["t"], json!(0));
        assert_eq!(time_map[0]["t_orig"], json!(0));
        assert_eq!(time_map[1]["t_orig"], json!(1));
    }

    mod registry {
        use super::*;

        #[test]
        fn out_root_resolves_to_tiff_series() {
            let dir = tempdir().unwrap();
            write_mdat_tree(dir.path(), 1, 1, 1, 1);
            let adapter = resolve_reader_adapter(dir.path()).expect("resolve ok");
            assert_eq!(adapter.name(), "tiff-series");
        }

        #[test]
        fn pos_dir_resolves_to_tiff_series() {
            let dir = tempdir().unwrap();
            write_mdat_tree(dir.path(), 1, 1, 1, 1);
            let pos_dir = dir.path().join("Pos0");
            let adapter = resolve_reader_adapter(&pos_dir).expect("resolve ok");
            assert_eq!(adapter.name(), "tiff-series");
        }

        #[test]
        fn single_tif_resolves_to_tiff_series() {
            let dir = tempdir().unwrap();
            write_mdat_tree(dir.path(), 1, 1, 1, 1);
            let file = dir
                .path()
                .join("Pos0")
                .join("img_channel000_position000_time000000000_z000.tif");
            let adapter = resolve_reader_adapter(&file).expect("resolve ok");
            assert_eq!(adapter.name(), "tiff-series");
        }

        #[test]
        fn nd2_suffix_still_resolves_to_nd2() {
            let dir = tempdir().unwrap();
            let file = dir.path().join("foo.nd2");
            fs::write(&file, b"x").unwrap();
            let adapter = resolve_reader_adapter(&file).expect("resolve ok");
            assert_eq!(adapter.name(), "nd2");
        }

        #[test]
        fn czi_suffix_still_resolves_to_czi() {
            let dir = tempdir().unwrap();
            let file = dir.path().join("foo.czi");
            fs::write(&file, b"x").unwrap();
            let adapter = resolve_reader_adapter(&file).expect("resolve ok");
            assert_eq!(adapter.name(), "czi");
        }

        #[test]
        fn unknown_directory_is_unsupported() {
            let dir = tempdir().unwrap();
            match resolve_reader_adapter(dir.path()) {
                Ok(adapter) => panic!("expected unsupported, got {}", adapter.name()),
                Err(err) => assert!(err.to_string().contains("mdat convert")),
            }
        }

        #[test]
        fn unknown_suffix_is_unsupported() {
            let dir = tempdir().unwrap();
            let file = dir.path().join("foo.bin");
            fs::write(&file, b"x").unwrap();
            match resolve_reader_adapter(&file) {
                Ok(adapter) => panic!("expected unsupported, got {}", adapter.name()),
                Err(err) => assert!(err.to_string().contains(".bin")),
            }
        }

        #[test]
        fn ensure_input_exists_accepts_directory() {
            let dir = tempdir().unwrap();
            write_mdat_tree(dir.path(), 1, 1, 1, 1);
            ensure_input_exists(dir.path()).expect("dir accepted");
        }

        #[test]
        fn ensure_input_exists_accepts_file() {
            let dir = tempdir().unwrap();
            let file = dir.path().join("foo.nd2");
            fs::write(&file, b"x").unwrap();
            ensure_input_exists(&file).expect("file accepted");
        }

        #[test]
        fn ensure_input_exists_rejects_missing() {
            let dir = tempdir().unwrap();
            let missing = dir.path().join("nope");
            let err = ensure_input_exists(&missing).unwrap_err();
            assert!(err.to_string().contains("not found"));
        }
    }
}