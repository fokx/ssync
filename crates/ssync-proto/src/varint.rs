//! Variable-length integer encoding (varint / varlong).

use std::io::{Read, Write};

/// Writes a 32-bit integer in rsync varint format.
pub fn write_varint<W: Write>(writer: &mut W, val: i32) -> std::io::Result<usize> {
    if (0..=127).contains(&val) {
        writer.write_all(&[val as u8])?;
        Ok(1)
    } else {
        let bytes = val.to_le_bytes();
        let mut len = 4;
        while len > 1 && bytes[len - 1] == 0 {
            len -= 1;
        }
        let tag = 0x80 | (len as u8);
        writer.write_all(&[tag])?;
        writer.write_all(&bytes[..len])?;
        Ok(1 + len)
    }
}

/// Reads a 32-bit integer in rsync varint format.
pub fn read_varint<R: Read>(reader: &mut R) -> std::io::Result<i32> {
    let mut first = [0u8; 1];
    reader.read_exact(&mut first)?;
    if first[0] & 0x80 == 0 {
        return Ok(first[0] as i32);
    }
    let len = (first[0] & 0x07) as usize;
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf[..len])?;
    Ok(i32::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        for v in [0, 1, 42, 127] {
            let mut buf = Vec::new();
            write_varint(&mut buf, v).unwrap();
            let mut cursor = std::io::Cursor::new(buf);
            let read_back = read_varint(&mut cursor).unwrap();
            assert_eq!(v, read_back);
        }
    }
}
