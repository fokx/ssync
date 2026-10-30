//! ssync-fs: Local filesystem operations, metadata preservation, and path handling.

pub mod filter;
pub mod metadata;
pub mod path;
pub mod sync;
pub mod walker;

#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path error: {0}")]
    PathError(String),
    #[error("Metadata preservation error: {0}")]
    MetadataError(String),
    #[error("Deletion limit exceeded: {0}")]
    DeleteLimitReached(usize),
}
