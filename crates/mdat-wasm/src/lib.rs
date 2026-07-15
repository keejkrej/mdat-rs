use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};

use czi_rs::CziFile;
use js_sys::{Object, Reflect, Uint16Array};
use nd2_rs::Nd2File;
use serde::Serialize;
use wasm_bindgen::prelude::*;
#[derive(Debug, Clone, Copy, Serialize)]
struct ImageSummary {
    n_pos: usize,
    n_time: usize,
    n_chan: usize,
    n_z: usize,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize)]
struct OpenResponse {
    handle: u32,
    format: &'static str,
    name: String,
    summary: ImageSummary,
}

enum MicroscopySession {
    Nd2(Nd2File),
    Czi(CziFile),
}

struct SessionRecord {
    format: &'static str,
    name: String,
    summary: ImageSummary,
    reader: MicroscopySession,
}


static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);

thread_local! {
    static SESSIONS: RefCell<HashMap<u32, SessionRecord>> = RefCell::new(HashMap::new());
}

fn with_sessions<T>(f: impl FnOnce(&mut HashMap<u32, SessionRecord>) -> T) -> T {
    SESSIONS.with(|sessions| f(&mut sessions.borrow_mut()))
}

fn axis_size(sizes: &std::collections::BTreeMap<String, usize>, axis: &str) -> usize {
    sizes.get(axis).copied().unwrap_or(1).max(1)
}

fn summary_from_nd2(reader: &mut Nd2File) -> Result<ImageSummary, String> {
    let summary = reader
        .summary()
        .map_err(|error| error.to_string())?;
    Ok(ImageSummary {
        n_pos: axis_size(&summary.sizes, "P"),
        n_time: axis_size(&summary.sizes, "T"),
        n_chan: axis_size(&summary.sizes, "C"),
        n_z: axis_size(&summary.sizes, "Z"),
        width: axis_size(&summary.sizes, "X") as u32,
        height: axis_size(&summary.sizes, "Y") as u32,
    })
}

fn summary_from_czi(reader: &mut CziFile) -> Result<ImageSummary, String> {
    let summary = reader
        .summary()
        .map_err(|error| error.to_string())?;
    let n_pos = {
        let scenes = axis_size(&summary.sizes, "S");
        if scenes > 1 { scenes } else { 1 }
    };
    Ok(ImageSummary {
        n_pos,
        n_time: axis_size(&summary.sizes, "T"),
        n_chan: axis_size(&summary.sizes, "C"),
        n_z: axis_size(&summary.sizes, "Z"),
        width: axis_size(&summary.sizes, "X") as u32,
        height: axis_size(&summary.sizes, "Y") as u32,
    })
}

fn detect_format(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".nd2") {
        Some("nd2")
    } else if lower.ends_with(".czi") {
        Some("czi")
    } else {
        None
    }
}

fn store_session(record: SessionRecord) -> u32 {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    with_sessions(|sessions| {
        sessions.insert(handle, record);
    });
    handle
}

fn with_session<T>(
    handle: u32,
    f: impl FnOnce(&mut SessionRecord) -> Result<T, String>,
) -> Result<T, String> {
    with_sessions(|sessions| {
        let session = sessions
            .get_mut(&handle)
            .ok_or_else(|| format!("invalid microscopy handle: {handle}"))?;
        f(session)
    })
}

fn remove_session(handle: u32) -> bool {
    with_sessions(|sessions| sessions.remove(&handle).is_some())
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn to_js_error(message: String) -> JsValue {
    JsValue::from_str(&message)
}

#[wasm_bindgen]
pub fn open_microscopy_file(name: &str, bytes: &[u8]) -> Result<JsValue, JsValue> {
    let format = detect_format(name).ok_or_else(|| {
        to_js_error(format!("unsupported microscopy file: {name}"))
    })?;

    let cursor = Cursor::new(bytes.to_vec());
    let (summary, reader) = match format {
        "nd2" => {
            let mut reader = Nd2File::open_reader(cursor).map_err(|error| to_js_error(error.to_string()))?;
            let summary = summary_from_nd2(&mut reader).map_err(to_js_error)?;
            (summary, MicroscopySession::Nd2(reader))
        }
        "czi" => {
            let mut reader = CziFile::open_reader(cursor).map_err(|error| to_js_error(error.to_string()))?;
            let summary = summary_from_czi(&mut reader).map_err(to_js_error)?;
            (summary, MicroscopySession::Czi(reader))
        }
        _ => return Err(to_js_error("unsupported microscopy format".to_owned())),
    };

    let handle = store_session(SessionRecord {
        format,
        name: name.to_owned(),
        summary,
        reader,
    });

    to_js_value(&OpenResponse {
        handle,
        format,
        name: name.to_owned(),
        summary,
    })
}

#[wasm_bindgen]
pub fn read_microscopy_frame(
    handle: u32,
    position: u32,
    time: u32,
    channel: u32,
    z: u32,
) -> Result<JsValue, JsValue> {
    with_session(handle, |session| {
        let pixels = match &mut session.reader {
            MicroscopySession::Nd2(reader) => reader
                .read_frame_2d(
                    position as usize,
                    time as usize,
                    channel as usize,
                    z as usize,
                )
                .map_err(|error| error.to_string())?,
            MicroscopySession::Czi(reader) => reader
                .read_frame_2d(
                    position as usize,
                    time as usize,
                    channel as usize,
                    z as usize,
                )
                .map_err(|error| error.to_string())?,
        };

        let width = session.summary.width as usize;
        let height = session.summary.height as usize;
        if width == 0 || height == 0 {
            return Err("missing image dimensions in microscopy summary".to_owned());
        }
        let expected = width
            .checked_mul(height)
            .ok_or_else(|| "frame pixel count overflow".to_owned())?;
        if pixels.len() != expected {
            return Err(format!(
                "expected {} pixels, got {}",
                expected,
                pixels.len()
            ));
        }

        let array = Uint16Array::new_with_length(pixels.len() as u32);
        array.copy_from(&pixels);

        let response = Object::new();
        Reflect::set(
            &response,
            &JsValue::from_str("width"),
            &JsValue::from_f64(width as f64),
        )
        .map_err(|error| format!("failed to set width: {error:?}"))?;
        Reflect::set(
            &response,
            &JsValue::from_str("height"),
            &JsValue::from_f64(height as f64),
        )
        .map_err(|error| format!("failed to set height: {error:?}"))?;
        Reflect::set(
            &response,
            &JsValue::from_str("pixels"),
            &array.into(),
        )
        .map_err(|error| format!("failed to set pixels: {error:?}"))?;

        Ok(response.into())
    })
    .map_err(to_js_error)
}

#[wasm_bindgen]
pub fn close_microscopy_file(handle: u32) -> bool {
    remove_session(handle)
}
