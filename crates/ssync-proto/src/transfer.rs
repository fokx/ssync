//! Streaming block checksums and delta tokens over the wire.

use crate::varint::{read_int, write_int};
use ssync_core::delta::{DeltaToken, SumStruct};
use std::io::{Read, Write};

/// Block checksum pair sent over the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireBlockSum {
    pub sum1: u32,
    pub sum2: Vec<u8>,
}

/// Transmits the block checksum signature of a basis file over the wire.
pub fn send_block_checksums<W: Write>(
    writer: &mut W,
    sum_struct: &SumStruct,
    sums: &[WireBlockSum],
) -> std::io::Result<()> {
    write_int(writer, sum_struct.count as i32)?;
    write_int(writer, sum_struct.block_len as i32)?;
    write_int(writer, sum_struct.strong_sum_len as i32)?;
    write_int(writer, sum_struct.remainder as i32)?;

    for sum in sums {
        write_int(writer, sum.sum1 as i32)?;
        writer.write_all(&sum.sum2)?;
    }

    writer.flush()?;
    Ok(())
}

/// Receives the block checksum signature of a basis file from the wire.
pub fn recv_block_checksums<R: Read>(
    reader: &mut R,
) -> std::io::Result<(SumStruct, Vec<WireBlockSum>)> {
    let count = read_int(reader)? as usize;
    let block_len = read_int(reader)? as usize;
    let strong_sum_len = read_int(reader)? as usize;
    let remainder = read_int(reader)? as usize;

    let mut sums = Vec::with_capacity(count);
    for _ in 0..count {
        let sum1 = read_int(reader)? as u32;
        let mut sum2 = vec![0u8; strong_sum_len];
        reader.read_exact(&mut sum2)?;
        sums.push(WireBlockSum { sum1, sum2 });
    }

    let file_len = if count == 0 {
        0
    } else {
        ((count - 1) * block_len + remainder) as u64
    };

    let sum_struct = SumStruct {
        file_len,
        count,
        block_len,
        strong_sum_len,
        remainder,
    };

    Ok((sum_struct, sums))
}

/// Streams delta tokens and the whole-file checksum over the wire.
pub fn send_delta_tokens<W: Write>(
    writer: &mut W,
    tokens: &[DeltaToken],
    whole_file_md5: &[u8],
) -> std::io::Result<()> {
    for token in tokens {
        match token {
            DeltaToken::Literal(bytes) => {
                let mut offset = 0;
                while offset < bytes.len() {
                    let chunk_len = (bytes.len() - offset).min(32768);
                    write_int(writer, chunk_len as i32)?;
                    writer.write_all(&bytes[offset..offset + chunk_len])?;
                    offset += chunk_len;
                }
            }
            DeltaToken::BlockRef(block_idx) => {
                let token_val = -((*block_idx as i32) + 1);
                write_int(writer, token_val)?;
            }
        }
    }

    // Terminal token 0 indicates end of tokens for this file
    write_int(writer, 0)?;

    // Send whole-file checksum (16 bytes for MD5)
    writer.write_all(whole_file_md5)?;
    writer.flush()?;

    Ok(())
}

/// Reads delta tokens from the wire until the terminal token 0, followed by the whole-file digest.
pub fn recv_delta_tokens<R: Read>(
    reader: &mut R,
    digest_len: usize,
) -> std::io::Result<(Vec<DeltaToken>, Vec<u8>)> {
    let mut tokens = Vec::new();

    loop {
        let token_val = read_int(reader)?;
        if token_val == 0 {
            break;
        } else if token_val > 0 {
            let chunk_len = token_val as usize;
            let mut chunk = vec![0u8; chunk_len];
            reader.read_exact(&mut chunk)?;
            tokens.push(DeltaToken::Literal(chunk));
        } else {
            let block_idx = (-token_val - 1) as u32;
            tokens.push(DeltaToken::BlockRef(block_idx));
        }
    }

    let mut digest = vec![0u8; digest_len];
    reader.read_exact(&mut digest)?;

    Ok((tokens, digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_checksums_roundtrip() {
        let sum_struct = SumStruct {
            file_len: 1500,
            count: 3,
            block_len: 700,
            strong_sum_len: 16,
            remainder: 100,
        };
        let sums = vec![
            WireBlockSum {
                sum1: 0x11223344,
                sum2: vec![1; 16],
            },
            WireBlockSum {
                sum1: 0x55667788,
                sum2: vec![2; 16],
            },
            WireBlockSum {
                sum1: 0x99AABBCC,
                sum2: vec![3; 16],
            },
        ];

        let mut buf = Vec::new();
        send_block_checksums(&mut buf, &sum_struct, &sums).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let (read_sum_struct, read_sums) = recv_block_checksums(&mut cursor).unwrap();

        assert_eq!(sum_struct, read_sum_struct);
        assert_eq!(sums, read_sums);
    }

    #[test]
    fn test_delta_tokens_roundtrip() {
        let tokens = vec![
            DeltaToken::Literal(b"hello world".to_vec()),
            DeltaToken::BlockRef(0),
            DeltaToken::Literal(b"foo bar".to_vec()),
            DeltaToken::BlockRef(2),
        ];
        let md5 = vec![42u8; 16];

        let mut buf = Vec::new();
        send_delta_tokens(&mut buf, &tokens, &md5).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let (read_tokens, read_digest) = recv_delta_tokens(&mut cursor, 16).unwrap();

        assert_eq!(tokens, read_tokens);
        assert_eq!(md5, read_digest);
    }
}
