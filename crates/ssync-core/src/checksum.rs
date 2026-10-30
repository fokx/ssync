//! Checksum implementations: weak rolling Checksum1 (Adler32-variant) and strong digests.

use md4::Digest as _;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Weak rolling checksum (Checksum1) based on the Adler32-variant used by rsync.
///
/// In rsync:
/// `s1 = sum(buf[i]) mod 65536`
/// `s2 = sum((n - i) * buf[i]) mod 65536`
/// `checksum = (s1 & 0xffff) + (s2 << 16)`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RollingChecksum {
    s1: u32,
    s2: u32,
    count: usize,
}

impl RollingChecksum {
    /// Creates a new, empty rolling checksum.
    pub const fn new() -> Self {
        Self {
            s1: 0,
            s2: 0,
            count: 0,
        }
    }

    /// Computes the initial weak checksum for a given byte slice.
    pub fn calculate(buf: &[u8]) -> Self {
        let mut s1: u32 = 0;
        let mut s2: u32 = 0;
        let len = buf.len();

        for (i, &byte) in buf.iter().enumerate() {
            let b = byte as u32;
            s1 = s1.wrapping_add(b);
            s2 = s2.wrapping_add((len - i) as u32 * b);
        }

        Self {
            s1: s1 & 0xFFFF,
            s2: s2 & 0xFFFF,
            count: len,
        }
    }

    /// Rolls the window forward by 1 byte: discards `out_byte` and adds `in_byte`.
    pub fn roll(&mut self, out_byte: u8, in_byte: u8) {
        let out_val = out_byte as u32;
        let in_val = in_byte as u32;
        let k = self.count as u32;

        self.s1 = self.s1.wrapping_sub(out_val).wrapping_add(in_val) & 0xFFFF;
        self.s2 = self
            .s2
            .wrapping_sub(k.wrapping_mul(out_val))
            .wrapping_add(self.s1)
            & 0xFFFF;
    }

    /// Returns the combined 32-bit checksum value: `(s1 & 0xffff) | (s2 << 16)`.
    pub const fn value(&self) -> u32 {
        (self.s1 & 0xFFFF) | ((self.s2 & 0xFFFF) << 16)
    }

    pub const fn s1(&self) -> u16 {
        (self.s1 & 0xFFFF) as u16
    }

    pub const fn s2(&self) -> u16 {
        (self.s2 & 0xFFFF) as u16
    }

    /// Traditional rsync SUM2HASH lookup key: `(s1 + s2) & 0xFFFF`.
    pub const fn hash16(&self) -> u16 {
        ((self.s1 & 0xFFFF) + (self.s2 & 0xFFFF)) as u16
    }
}

/// Computes the 16-bit hash key from a 32-bit `sum1` value: `SUM2HASH(sum1)`.
pub const fn sum2hash(sum1: u32) -> u16 {
    let s1 = sum1 & 0xFFFF;
    let s2 = sum1 >> 16;
    ((s1 + s2) & 0xFFFF) as u16
}

/// Supported strong digest algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChecksumAlgorithm {
    #[default]
    Md5,
    Md4,
    Sha1,
    Xxh64,
}

impl ChecksumAlgorithm {
    pub const fn digest_len(self) -> usize {
        match self {
            Self::Md5 => 16,
            Self::Md4 => 16,
            Self::Sha1 => 20,
            Self::Xxh64 => 8,
        }
    }
}

/// Computes a strong digest of `data` using the specified algorithm and optional 32-bit seed.
pub fn calculate_strong_checksum(
    algo: ChecksumAlgorithm,
    data: &[u8],
    seed: Option<u32>,
) -> Vec<u8> {
    let seed_bytes = seed.map(|s| s.to_le_bytes());

    match algo {
        ChecksumAlgorithm::Md5 => {
            let mut hasher = md5::Md5::new();
            if let Some(sb) = seed_bytes {
                hasher.update(sb);
            }
            hasher.update(data);
            hasher.finalize().to_vec()
        }
        ChecksumAlgorithm::Md4 => {
            let mut hasher = md4::Md4::new();
            if let Some(sb) = seed_bytes {
                hasher.update(sb);
            }
            hasher.update(data);
            hasher.finalize().to_vec()
        }
        ChecksumAlgorithm::Sha1 => {
            let mut hasher = sha1::Sha1::new();
            if let Some(sb) = seed_bytes {
                hasher.update(sb);
            }
            hasher.update(data);
            hasher.finalize().to_vec()
        }
        ChecksumAlgorithm::Xxh64 => {
            let s = seed.unwrap_or(0) as u64;
            let mut hasher = xxhash_rust::xxh64::Xxh64::new(s);
            hasher.update(data);
            hasher.digest().to_be_bytes().to_vec()
        }
    }
}

/// Computes the full file checksum for quick-check (-c) or whole-file verification.
pub fn calculate_file_checksum(
    algo: ChecksumAlgorithm,
    path: &Path,
    seed: Option<u32>,
) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 65536];
    let seed_bytes = seed.map(|s| s.to_le_bytes());

    match algo {
        ChecksumAlgorithm::Md5 => {
            let mut hasher = md5::Md5::new();
            if let Some(sb) = seed_bytes {
                hasher.update(sb);
            }
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(hasher.finalize().to_vec())
        }
        ChecksumAlgorithm::Md4 => {
            let mut hasher = md4::Md4::new();
            if let Some(sb) = seed_bytes {
                hasher.update(sb);
            }
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(hasher.finalize().to_vec())
        }
        ChecksumAlgorithm::Sha1 => {
            let mut hasher = sha1::Sha1::new();
            if let Some(sb) = seed_bytes {
                hasher.update(sb);
            }
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(hasher.finalize().to_vec())
        }
        ChecksumAlgorithm::Xxh64 => {
            let s = seed.unwrap_or(0) as u64;
            let mut hasher = xxhash_rust::xxh64::Xxh64::new(s);
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            Ok(hasher.digest().to_be_bytes().to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_checksum_matches_full_calc() {
        let data = b"Hello, world! This is a test string for rolling checksum.";
        let window_size = 16;

        for start in 0..data.len() - window_size {
            let direct = RollingChecksum::calculate(&data[start..start + window_size]);

            if start == 0 {
                continue;
            }

            let prev_slice = &data[start - 1..start - 1 + window_size];
            let mut rolling = RollingChecksum::calculate(prev_slice);
            rolling.roll(data[start - 1], data[start + window_size - 1]);

            assert_eq!(
                direct.value(),
                rolling.value(),
                "Mismatch at offset {start}"
            );
            assert_eq!(sum2hash(direct.value()), direct.hash16());
        }
    }

    #[test]
    fn test_strong_checksums() {
        let data = b"The quick brown fox jumps over the lazy dog";
        let md5_sum = calculate_strong_checksum(ChecksumAlgorithm::Md5, data, None);
        assert_eq!(md5_sum.len(), 16);

        let sha1_sum = calculate_strong_checksum(ChecksumAlgorithm::Sha1, data, None);
        assert_eq!(sha1_sum.len(), 20);

        let xxh64_sum = calculate_strong_checksum(ChecksumAlgorithm::Xxh64, data, None);
        assert_eq!(xxh64_sum.len(), 8);
    }
}
