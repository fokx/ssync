//! ssync-fs: Local filesystem operations, metadata preservation, and path handling.

pub mod metadata;
pub mod path;

#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path error: {0}")]
    PathError(String),
    #[error("Metadata preservation error: {0}")]
    MetadataError(String),
}
