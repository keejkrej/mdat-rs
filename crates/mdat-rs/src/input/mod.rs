//! Input seam: ND2/CZI/TIFF-series readers and session helpers.

mod czi;
mod nd2;
pub mod registry;
pub mod session;
mod tiff_series;
pub mod types;

pub use registry::{ensure_input_exists, location_suffix, open_input, resolve_reader_adapter};
pub use session::{inspect_input, open_reader};
pub use types::{axis_size, ImageInfo, MetadataPayload, ReaderAdapter, ReaderSession};
