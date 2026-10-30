//! ssync-core: Core algorithms, checksums, and delta-transfer math for ssync.

#![forbid(unsafe_code)]

pub mod checksum;
pub mod delta;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error in core engine: {0}")]
    Io(#[from] std::io::Error),
    #[error("Checksum mismatch: expected {expected}, computed {computed}")]
    ChecksumMismatch { expected: String, computed: String },
    #[error("Block matching failed: {0}")]
    BlockMatchError(String),
}
