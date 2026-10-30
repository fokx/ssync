//! Multiplexed I/O framing constants and types.

/// The base tag offset used in rsync multiplex framing.
pub const MPLEX_BASE: u8 = 7;

/// Multiplexed message codes matching upstream `enum msgcode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MsgCode {
    Data = 0,
    ErrorXfer = 1,
    Info = 2,
    Error = 3,
    Warning = 4,
    Redo = 9,
    Stats = 10,
    IoError = 22,
    Noop = 42,
    Success = 100,
    Deleted = 101,
    NoSend = 102,
}
