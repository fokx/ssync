//! Delta transfer token generation and block matching data structures.

/// A token transmitted from sender to receiver during delta transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaToken {
    /// Literal bytes that did not match any block in the basis file.
    Literal(Vec<u8>),
    /// Block reference index into the basis file.
    BlockRef(u32),
}

/// Header describing the block checksum signature of a basis file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumStruct {
    pub count: u32,
    pub block_len: u32,
    pub strong_sum_len: u32,
    pub remainder: u32,
}
