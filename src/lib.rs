pub mod archive;
pub mod document;
pub mod layer;
pub mod tile;
pub mod export;

pub use document::ProcreateDocument;
pub use layer::{Layer, BlendMode};
pub use export::ExportOptions;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcreateError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Plist error: {0}")]
    Plist(#[from] plist::Error),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("LZ4 decompression error: {0}")]
    Lz4(#[from] lz4_flex::frame::Error),

    #[error("Invalid document: {0}")]
    InvalidDocument(String),

    #[error("Missing field: {0}")]
    MissingField(String),
}

pub type Result<T> = std::result::Result<T, ProcreateError>;
