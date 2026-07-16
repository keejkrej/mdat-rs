pub mod convert;
pub mod error;
pub mod input;
pub mod io;
pub mod metadata;
pub mod output;
pub mod selection;
pub mod slices;

pub use convert::{preview_selection, run_convert, ConvertSelectionSummary};
pub use error::{MdatError, Result};
pub use input::{ImageInfo, MetadataPayload, ReaderAdapter, ReaderSession};
pub use metadata::run_metadata;
pub use output::{OutputFormat, ProgressCallback, ProgressEvent, ProgressPhase};