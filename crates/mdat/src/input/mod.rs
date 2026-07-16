pub use mdat_input::types::{axis_size, ImageInfo, MetadataPayload, ReaderAdapter, ReaderSession};
pub use mdat_input::session::{inspect_input, open_reader};
pub use mdat_input::registry::{ensure_input_exists, location_suffix, open_input, resolve_reader_adapter};

pub mod registry {
    pub use mdat_input::registry::{ensure_input_exists, location_suffix, open_input, resolve_reader_adapter};
}