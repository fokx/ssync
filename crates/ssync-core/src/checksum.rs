//! Checksum implementations: weak rolling Checksum1 (Adler32-variant) and strong digests.

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

    /// Returns the combined 32-bit checksum value.
    pub const fn value(&self) -> u32 {
        (self.s1 & 0xFFFF) | ((self.s2 & 0xFFFF) << 16)
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
        }
    }
}
