//! Archive processing for tear. This crate is independent of the command line and GUI.

mod archive;
pub mod format;
pub mod path;
#[cfg(feature = "rar")]
mod rar_stream;
mod readers;
mod windows_io;
pub use archive::{Archive, ArchiveOptions, Entry, ExtractMode, Report};
pub use readers::is_password_error;
