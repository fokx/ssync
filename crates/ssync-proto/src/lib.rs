//! ssync-proto: Rsync wire protocol 30/31 framing, varints, and multiplexing.

#![forbid(unsafe_code)]

pub mod flist;
pub mod handshake;
pub mod mplex;
pub mod transfer;
pub mod varint;

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("I/O error during protocol exchange: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocol error: {0}")]
    ProtocolViolation(String),
}
