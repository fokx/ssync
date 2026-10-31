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
        let mut offset = 0;

        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") && buf.len() >= 32 {
                // SAFETY: We checked that the CPU supports AVX2 via runtime CPUID detection,
                // and calculate_avx2 accesses only in-bounds 32-byte chunks of buf.
                let (consumed, s1_acc, s2_acc) = unsafe { calculate_avx2(buf, s1, s2) };
                offset = consumed;
                s1 = s1_acc;
                s2 = s2_acc;
            }
        }

        // Unrolled 4-byte chunk loop for remainder or when SIMD is unavailable.
        let rem = &buf[offset..];
        let chunks_4 = rem.len() / 4;
        for i in 0..chunks_4 {
            let base = i * 4;
            let b0 = rem[base] as u32;
            let b1 = rem[base + 1] as u32;
            let b2 = rem[base + 2] as u32;
            let b3 = rem[base + 3] as u32;

            s2 = s2
                .wrapping_add(4u32.wrapping_mul(s1))
                .wrapping_add(4u32.wrapping_mul(b0))
                .wrapping_add(3u32.wrapping_mul(b1))
                .wrapping_add(2u32.wrapping_mul(b2))
                .wrapping_add(b3);
            s1 = s1.wrapping_add(b0 + b1 + b2 + b3);
        }

        let tail = &rem[chunks_4 * 4..];
        for &b in tail {
            let val = b as u32;
            s1 = s1.wrapping_add(val);
            s2 = s2.wrapping_add(s1);
        }

        Self {
            s1: s1 & 0xFFFF,
            s2: s2 & 0xFFFF,
            count: buf.len(),
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

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must ensure that the target CPU supports AVX2, and `buf` is a valid slice.
unsafe fn calculate_avx2(buf: &[u8], mut s1: u32, mut s2: u32) -> (usize, u32, u32) {
    use std::arch::x86_64::*;

    // SAFETY: Intrinsics calls are valid under target_feature(enable = "avx2").
    unsafe {
        let ones_8 = _mm256_set1_epi8(1);
        let ones_16 = _mm256_set1_epi16(1);
        #[rustfmt::skip]
        let weights = _mm256_setr_epi8(
            32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17,
            16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1,
        );

        let chunks = buf.len() / 32;
        let ptr = buf.as_ptr();

        for i in 0..chunks {
            // SAFETY: i * 32 + 32 <= buf.len(), so ptr.add(i * 32) points to 32 readable bytes.
            let v = _mm256_loadu_si256(ptr.add(i * 32) as *const __m256i);

            // T1 = sum(b_j)
            let pairs_t1 = _mm256_maddubs_epi16(v, ones_8);
            let quads_t1 = _mm256_madd_epi16(pairs_t1, ones_16);

            // T2 = sum((32 - j) * b_j)
            let pairs_t2 = _mm256_maddubs_epi16(v, weights);
            let quads_t2 = _mm256_madd_epi16(pairs_t2, ones_16);

            // Horizontal sum of quads_t1
            let lo_t1 = _mm256_castsi256_si128(quads_t1);
            let hi_t1 = _mm256_extracti128_si256(quads_t1, 1);
            let sum_t1_128 = _mm_add_epi32(lo_t1, hi_t1);
            let sum_t1_64 = _mm_add_epi32(sum_t1_128, _mm_shuffle_epi32(sum_t1_128, 0xEE));
            let sum_t1_32 = _mm_add_epi32(sum_t1_64, _mm_shuffle_epi32(sum_t1_64, 0x55));
            let t1 = _mm_cvtsi128_si32(sum_t1_32) as u32;

            // Horizontal sum of quads_t2
            let lo_t2 = _mm256_castsi256_si128(quads_t2);
            let hi_t2 = _mm256_extracti128_si256(quads_t2, 1);
            let sum_t2_128 = _mm_add_epi32(lo_t2, hi_t2);
            let sum_t2_64 = _mm_add_epi32(sum_t2_128, _mm_shuffle_epi32(sum_t2_128, 0xEE));
            let sum_t2_32 = _mm_add_epi32(sum_t2_64, _mm_shuffle_epi32(sum_t2_64, 0x55));
            let t2 = _mm_cvtsi128_si32(sum_t2_32) as u32;

            s2 = s2.wrapping_add(32u32.wrapping_mul(s1)).wrapping_add(t2);
            s1 = s1.wrapping_add(t1);
        }

        (chunks * 32, s1, s2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn scalar_reference(buf: &[u8]) -> (u16, u16) {
        let mut s1: u32 = 0;
        let mut s2: u32 = 0;
        let len = buf.len();
        for (i, &byte) in buf.iter().enumerate() {
            let b = byte as u32;
            s1 = s1.wrapping_add(b);
            s2 = s2.wrapping_add((len - i) as u32 * b);
        }
        ((s1 & 0xFFFF) as u16, (s2 & 0xFFFF) as u16)
    }

    proptest! {
        #[test]
        fn test_simd_matches_scalar_reference(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let (ref_s1, ref_s2) = scalar_reference(&data);
            let cs = RollingChecksum::calculate(&data);
            prop_assert_eq!(cs.s1(), ref_s1);
            prop_assert_eq!(cs.s2(), ref_s2);
        }

        #[test]
        fn test_simd_unaligned_slices(data in proptest::collection::vec(any::<u8>(), 33..256), offset in 1usize..16) {
            let slice = &data[offset..];
            let (ref_s1, ref_s2) = scalar_reference(slice);
            let cs = RollingChecksum::calculate(slice);
            prop_assert_eq!(cs.s1(), ref_s1);
            prop_assert_eq!(cs.s2(), ref_s2);
        }
    }

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
