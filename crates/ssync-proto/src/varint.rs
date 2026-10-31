//! Variable-length integer encoding (varint / varlong) matching rsync wire format.

use std::io::{Read, Write};

/// Byte extra lookup table based on `(ch / 4)` from rsync `io.c`.
#[rustfmt::skip]
const INT_BYTE_EXTRA: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x00 - 0x3F
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x40 - 0x7F
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 0x80 - 0xBF
    2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 5, 6, // 0xC0 - 0xFF
];

/// Writes a 32-bit integer in rsync varint format.
pub fn write_varint<W: Write>(writer: &mut W, val: i32) -> std::io::Result<usize> {
    let mut b = [0u8; 5];
    let val_bytes = val.to_le_bytes();
    b[1..5].copy_from_slice(&val_bytes);

    let mut cnt = 4usize;
    while cnt > 1 && b[cnt] == 0 {
        cnt -= 1;
    }

    let bit = 1u8 << (8 - cnt);
    if b[cnt] >= bit {
        cnt += 1;
        b[0] = !(bit.wrapping_sub(1));
    } else if cnt > 1 {
        b[0] = b[cnt] | !(bit.wrapping_mul(2).wrapping_sub(1));
    } else {
        b[0] = b[1];
    }

    writer.write_all(&b[..cnt])?;
    Ok(cnt)
}

/// Reads a 32-bit integer in rsync varint format.
pub fn read_varint<R: Read>(reader: &mut R) -> std::io::Result<i32> {
    let mut ch = [0u8; 1];
    reader.read_exact(&mut ch)?;
    let extra = INT_BYTE_EXTRA[(ch[0] / 4) as usize] as usize;

    let mut buf = [0u8; 4];
    if extra > 0 {
        if extra > 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Varint overflow in read_varint",
            ));
        }
        let bit = 1u8 << (8 - extra);
        reader.read_exact(&mut buf[..extra])?;
        if extra < 4 {
            buf[extra] = ch[0] & (bit.wrapping_sub(1));
        }
    } else {
        buf[0] = ch[0];
    }

    Ok(i32::from_le_bytes(buf))
}

/// Writes a 64-bit integer in rsync varlong format.
pub fn write_varlong<W: Write>(writer: &mut W, val: i64, min_bytes: u8) -> std::io::Result<usize> {
    let mut b = [0u8; 9];
    let val_bytes = val.to_le_bytes();
    b[1..9].copy_from_slice(&val_bytes);

    let min_b = min_bytes as usize;
    let mut cnt = 8usize;
    while cnt > min_b && b[cnt] == 0 {
        cnt -= 1;
    }

    let shift = (7 + min_b).saturating_sub(cnt);
    let bit = 1u8 << shift;
    if b[cnt] >= bit {
        cnt += 1;
        b[0] = !(bit.wrapping_sub(1));
    } else if cnt > min_b {
        b[0] = b[cnt] | !(bit.wrapping_mul(2).wrapping_sub(1));
    } else {
        b[0] = b[cnt];
    }

    writer.write_all(&b[..cnt])?;
    Ok(cnt)
}

/// Reads a 64-bit integer in rsync varlong format.
pub fn read_varlong<R: Read>(reader: &mut R, min_bytes: u8) -> std::io::Result<i64> {
    let min_b = min_bytes as usize;
    let mut b2 = vec![0u8; min_b];
    reader.read_exact(&mut b2)?;

    let mut buf = [0u8; 8];
    if min_b > 1 {
        buf[..min_b - 1].copy_from_slice(&b2[1..min_b]);
    }

    let extra = INT_BYTE_EXTRA[(b2[0] / 4) as usize] as usize;
    if extra > 0 {
        if min_b + extra > 9 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Varlong overflow in read_varlong",
            ));
        }
        let bit = 1u8 << (8 - extra);
        reader.read_exact(&mut buf[min_b - 1..min_b - 1 + extra])?;
        if min_b + extra - 1 < 8 {
            buf[min_b + extra - 1] = b2[0] & (bit.wrapping_sub(1));
        }
    } else if min_b - 1 < 8 {
        buf[min_b - 1] = b2[0];
    }

    Ok(i64::from_le_bytes(buf))
}

/// Writes a 32-bit integer (little endian).
pub fn write_int<W: Write>(writer: &mut W, val: i32) -> std::io::Result<()> {
    writer.write_all(&val.to_le_bytes())
}

/// Reads a 32-bit integer (little endian).
pub fn read_int<R: Read>(reader: &mut R) -> std::io::Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

/// Writes a 16-bit integer (little endian).
pub fn write_short<W: Write>(writer: &mut W, val: i16) -> std::io::Result<()> {
    writer.write_all(&val.to_le_bytes())
}

/// Reads a 16-bit integer (little endian).
pub fn read_short<R: Read>(reader: &mut R) -> std::io::Result<i16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(i16::from_le_bytes(buf))
}

/// Writes a single byte.
pub fn write_byte<W: Write>(writer: &mut W, val: u8) -> std::io::Result<()> {
    writer.write_all(&[val])
}

/// Reads a single byte.
pub fn read_byte<R: Read>(reader: &mut R) -> std::io::Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        let test_values = [
            0,
            1,
            2,
            42,
            127,
            128,
            200,
            255,
            256,
            300,
            1000,
            16383,
            16384,
            65535,
            65536,
            1_000_000,
            100_000_000,
            i32::MAX,
        ];
        for &v in &test_values {
            let mut buf = Vec::new();
            write_varint(&mut buf, v).unwrap();
            let mut cursor = std::io::Cursor::new(buf);
            let read_back = read_varint(&mut cursor).unwrap();
            assert_eq!(v, read_back, "Failed for value {}", v);
        }
    }

    #[test]
    fn test_varlong_roundtrip() {
        let test_values = [
            0i64,
            1,
            42,
            127,
            128,
            300,
            65536,
            1_000_000_000,
            10_000_000_000,
            i64::MAX / 2,
        ];
        for &min_bytes in &[3u8, 4] {
            for &v in &test_values {
                let mut buf = Vec::new();
                write_varlong(&mut buf, v, min_bytes).unwrap();
                let mut cursor = std::io::Cursor::new(buf);
                let read_back = read_varlong(&mut cursor, min_bytes).unwrap();
                assert_eq!(
                    v, read_back,
                    "Failed varlong for {} with min_bytes {}",
                    v, min_bytes
                );
            }
        }
    }
}
