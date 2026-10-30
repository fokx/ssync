//! Delta transfer token generation and block matching data structures.

use crate::CoreError;
use crate::checksum::{ChecksumAlgorithm, RollingChecksum, calculate_strong_checksum, sum2hash};

pub const DEFAULT_BLOCK_SIZE: usize = 700;
pub const MAX_BLOCK_SIZE: usize = 131_072; // 128 KiB
pub const BLOCKSUM_BIAS: i32 = 10;
pub const MAX_CHAIN_LEN: usize = 1024;
pub const TRADITIONAL_TABLESIZE: usize = 65536;

/// A token transmitted from sender to receiver during delta transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaToken {
    /// Literal bytes that did not match any block in the basis file.
    Literal(Vec<u8>),
    /// Block reference index into the basis file (0-indexed).
    BlockRef(u32),
}

/// Header describing the block checksum signature of a basis file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumStruct {
    pub file_len: u64,
    pub count: usize,
    pub block_len: usize,
    pub strong_sum_len: usize,
    pub remainder: usize,
}

impl SumStruct {
    /// Calculates block size and checksum lengths according to rsync heuristics.
    pub fn calculate(
        file_len: u64,
        requested_block_size: Option<usize>,
        algo: ChecksumAlgorithm,
    ) -> Self {
        let max_s2len = algo.digest_len();
        let blength = if let Some(bs) = requested_block_size {
            if bs == 0 { DEFAULT_BLOCK_SIZE } else { bs }
        } else if file_len <= (DEFAULT_BLOCK_SIZE * DEFAULT_BLOCK_SIZE) as u64 {
            DEFAULT_BLOCK_SIZE
        } else {
            let mut c = 1i64;
            let mut l = file_len as i64;
            while l >> 2 != 0 {
                l >>= 2;
                c <<= 1;
            }
            if c < 0 || c >= MAX_BLOCK_SIZE as i64 {
                MAX_BLOCK_SIZE
            } else {
                let mut bl = 0i64;
                loop {
                    bl |= c;
                    if (file_len as i64) < bl * bl {
                        bl &= !c;
                    }
                    c >>= 1;
                    if c < 8 {
                        break;
                    }
                }
                std::cmp::max(bl as usize, DEFAULT_BLOCK_SIZE)
            }
        };

        // Donovan Baarda heuristic for s2length
        let mut b = BLOCKSUM_BIAS;
        let mut l = file_len as i64;
        while l >> 1 != 0 {
            l >>= 1;
            b += 2;
        }
        let mut c = blength as i64;
        while c >> 1 != 0 && b != 0 {
            c >>= 1;
            b -= 1;
        }
        let mut s2len = ((b + 1 - 32 + 7) / 8) as usize;
        s2len = std::cmp::max(s2len, 2);
        s2len = std::cmp::min(s2len, max_s2len);

        let remainder = (file_len as usize) % blength;
        let count = (file_len as usize / blength) + if remainder != 0 { 1 } else { 0 };

        Self {
            file_len,
            count,
            block_len: blength,
            strong_sum_len: s2len,
            remainder,
        }
    }
}

/// A block signature for matching basis blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSignature {
    pub index: u32,
    pub offset: u64,
    pub len: usize,
    pub sum1: u32,
    pub sum2: Vec<u8>,
}

/// Generates block signatures from basis data.
pub fn generate_basis_signatures(
    basis: &[u8],
    sums: &SumStruct,
    algo: ChecksumAlgorithm,
    seed: Option<u32>,
) -> Vec<BlockSignature> {
    let mut sigs = Vec::with_capacity(sums.count);
    let mut offset = 0;

    for i in 0..sums.count {
        let len = std::cmp::min(sums.block_len, basis.len() - offset);
        let chunk = &basis[offset..offset + len];
        let sum1 = RollingChecksum::calculate(chunk).value();
        let full_sum2 = calculate_strong_checksum(algo, chunk, seed);
        let s2len = std::cmp::min(sums.strong_sum_len, full_sum2.len());
        let sum2 = full_sum2[..s2len].to_vec();

        sigs.push(BlockSignature {
            index: i as u32,
            offset: offset as u64,
            len,
            sum1,
            sum2,
        });

        offset += len;
    }

    sigs
}

/// Statistics collected during delta generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchStats {
    pub hash_hits: usize,
    pub false_alarms: usize,
    pub matches: usize,
    pub literal_bytes: u64,
    pub matched_bytes: u64,
}

/// Generates delta tokens by matching source data against basis signatures.
pub fn generate_delta(
    source: &[u8],
    basis_sigs: &[BlockSignature],
    sums: &SumStruct,
    algo: ChecksumAlgorithm,
    seed: Option<u32>,
    _updating_basis_file: bool,
) -> (Vec<DeltaToken>, MatchStats) {
    let mut tokens = Vec::new();
    let mut stats = MatchStats::default();

    if basis_sigs.is_empty() {
        if !source.is_empty() {
            stats.literal_bytes = source.len() as u64;
            tokens.push(DeltaToken::Literal(source.to_vec()));
        }
        return (tokens, stats);
    }

    // Build 16-bit hash table
    let mut hash_table = vec![-1i32; TRADITIONAL_TABLESIZE];
    let mut chain = vec![-1i32; basis_sigs.len()];

    for (i, sig) in basis_sigs.iter().enumerate() {
        let t = sum2hash(sig.sum1) as usize;
        chain[i] = hash_table[t];
        hash_table[t] = i as i32;
    }

    let source_len = source.len();
    let mut offset = 0;
    let mut last_match = 0;
    let mut consecutive_false_offsets = 0;

    let end = if sums.remainder != 0 {
        source_len.saturating_sub(sums.remainder) + 1
    } else {
        source_len.saturating_sub(sums.block_len) + 1
    };

    if source_len < sums.block_len && source_len < sums.remainder {
        stats.literal_bytes = source_len as u64;
        tokens.push(DeltaToken::Literal(source.to_vec()));
        return (tokens, stats);
    }

    let mut k = std::cmp::min(sums.block_len, source_len);
    let mut rolling = RollingChecksum::calculate(&source[0..k]);

    while offset < end && offset < source_len {
        let s1 = rolling.s1();
        let s2 = rolling.s2();
        let sum = rolling.value();
        let hash_entry = ((s1 as usize) + (s2 as usize)) & 0xFFFF;

        let mut i = hash_table[hash_entry];
        let mut matched_block = false;
        let mut done_csum2 = false;
        let mut sum2 = Vec::new();
        let mut chain_len = 0;
        let mut last_false_sum2: Option<Vec<u8>> = None;
        let mut prev_idx: Option<usize> = None;

        if i >= 0 {
            stats.hash_hits += 1;

            while i >= 0 {
                let sig = &basis_sigs[i as usize];
                let l = std::cmp::min(sums.block_len, source_len - offset);

                if sum != sig.sum1 || l != sig.len {
                    prev_idx = Some(i as usize);
                    i = chain[i as usize];
                    continue;
                }

                // If this entry shares the same strong sum as a previous false alarm,
                // unlink it from the chain so long duplicate runs do not exhaust MAX_CHAIN_LEN.
                if let Some(ref lfs2) = last_false_sum2 {
                    if sig.sum2 == *lfs2 {
                        let next = chain[i as usize];
                        if let Some(p) = prev_idx {
                            chain[p] = next;
                        } else {
                            hash_table[hash_entry] = next;
                        }
                        i = next;
                        continue;
                    }
                }

                chain_len += 1;
                if chain_len > MAX_CHAIN_LEN {
                    break;
                }

                if !done_csum2 {
                    let full_sum2 =
                        calculate_strong_checksum(algo, &source[offset..offset + l], seed);
                    let s2len = std::cmp::min(sums.strong_sum_len, full_sum2.len());
                    sum2 = full_sum2[..s2len].to_vec();
                    done_csum2 = true;
                }

                if sum2 != sig.sum2 {
                    stats.false_alarms += 1;
                    last_false_sum2 = Some(sig.sum2.clone());
                    prev_idx = Some(i as usize);
                    i = chain[i as usize];
                    continue;
                }

                // Match found!
                matched_block = true;
                stats.matches += 1;
                consecutive_false_offsets = 0;

                // Emit prior literal bytes
                if offset > last_match {
                    let lit_chunk = source[last_match..offset].to_vec();
                    stats.literal_bytes += lit_chunk.len() as u64;
                    tokens.push(DeltaToken::Literal(lit_chunk));
                }

                // Emit block ref
                stats.matched_bytes += sig.len as u64;
                tokens.push(DeltaToken::BlockRef(sig.index));

                offset += sig.len;
                last_match = offset;

                if offset < source_len {
                    k = std::cmp::min(sums.block_len, source_len - offset);
                    rolling = RollingChecksum::calculate(&source[offset..offset + k]);
                }
                break;
            }
        }

        if matched_block {
            continue;
        }

        // Collision skip heuristic:
        // When 4 consecutive false offsets occur on strong checksum comparison (or chain > MAX_CHAIN_LEN),
        // skip forward min(k - 1, end - 1 - offset) bytes before the 1-byte advance.
        if done_csum2 || chain_len > MAX_CHAIN_LEN {
            consecutive_false_offsets += 1;
            if consecutive_false_offsets >= 4 && k > 1 {
                let skip = std::cmp::min(k - 1, end.saturating_sub(1 + offset));
                if skip > 0 {
                    offset += skip;
                    k = std::cmp::min(sums.block_len, source_len - offset);
                    rolling = RollingChecksum::calculate(&source[offset..offset + k]);
                }
                consecutive_false_offsets = 3;
            }
        } else {
            consecutive_false_offsets = 0;
        }

        // Roll window by 1 byte
        let out_byte = source[offset];
        offset += 1;

        if offset < source_len {
            if offset + k <= source_len {
                let in_byte = source[offset + k - 1];
                rolling.roll(out_byte, in_byte);
            } else {
                k = source_len - offset;
                rolling = RollingChecksum::calculate(&source[offset..offset + k]);
            }
        }
    }

    // Emit remaining literal bytes
    if last_match < source_len {
        let remainder_chunk = source[last_match..source_len].to_vec();
        stats.literal_bytes += remainder_chunk.len() as u64;
        tokens.push(DeltaToken::Literal(remainder_chunk));
    }

    (tokens, stats)
}

/// Reconstructs a file using basis bytes and delta tokens.
pub fn reconstruct_file(
    basis: &[u8],
    tokens: &[DeltaToken],
    block_len: usize,
) -> Result<Vec<u8>, CoreError> {
    let mut reconstructed = Vec::new();

    for token in tokens {
        match token {
            DeltaToken::Literal(bytes) => {
                reconstructed.extend_from_slice(bytes);
            }
            DeltaToken::BlockRef(idx) => {
                let offset = (*idx as usize) * block_len;
                if offset >= basis.len() {
                    return Err(CoreError::BlockMatchError(format!(
                        "Block reference index {} exceeds basis length {}",
                        idx,
                        basis.len()
                    )));
                }
                let len = std::cmp::min(block_len, basis.len() - offset);
                reconstructed.extend_from_slice(&basis[offset..offset + len]);
            }
        }
    }

    Ok(reconstructed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_delta_roundtrip_simple() {
        let basis = b"The quick brown fox jumps over the lazy dog. 1234567890.";
        let source = b"The fast brown fox leaps over the lazy dog. 1234567890. Tail addition!";

        let sums = SumStruct::calculate(basis.len() as u64, Some(16), ChecksumAlgorithm::Md5);
        let sigs = generate_basis_signatures(basis, &sums, ChecksumAlgorithm::Md5, None);

        let (tokens, stats) =
            generate_delta(source, &sigs, &sums, ChecksumAlgorithm::Md5, None, false);

        let reconstructed = reconstruct_file(basis, &tokens, sums.block_len).unwrap();
        assert_eq!(reconstructed, source);
        assert!(stats.matches > 0);
    }

    proptest! {
        #[test]
        fn test_arbitrary_basis_delta_roundtrip(
            basis in proptest::collection::vec(any::<u8>(), 0..4096),
            source in proptest::collection::vec(any::<u8>(), 0..4096),
            block_size in 16usize..512usize,
        ) {
            let sums = SumStruct::calculate(
                basis.len() as u64,
                Some(block_size),
                ChecksumAlgorithm::Md5,
            );
            let sigs = generate_basis_signatures(
                &basis,
                &sums,
                ChecksumAlgorithm::Md5,
                None,
            );

            let (tokens, _stats) = generate_delta(
                &source,
                &sigs,
                &sums,
                ChecksumAlgorithm::Md5,
                None,
                false,
            );

            let reconstructed = reconstruct_file(&basis, &tokens, sums.block_len).unwrap();
            prop_assert_eq!(reconstructed, source);
        }
    }
}
