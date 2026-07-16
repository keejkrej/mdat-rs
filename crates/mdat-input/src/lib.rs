//! Input seam shared by `mdat` and `mdat-view`.
//!
//! Holds the `ReaderAdapter` / `ReaderSession` contract, the ND2/CZI/TIFF-series
//! adapters, `resolve_reader_adapter` / `ensure_input_exists`, and the `MdatError`
//! enum (moved here in full and re-exported by `mdat` so existing call sites keep
//! compiling). This crate exists to break the `mdat` <-> `mdat-view` dependency
//! cycle: both depend on `mdat-input`, neither depends on the other for the seam.

mod czi;
pub mod error;
mod nd2;
pub mod registry;
pub mod session;
mod tiff_series;
pub mod types;

pub use error::{MdatError, Result};
pub use registry::{ensure_input_exists, location_suffix, open_input, resolve_reader_adapter};
pub use session::{inspect_input, open_reader};
pub use types::{ImageInfo, MetadataPayload, ReaderAdapter, ReaderSession};