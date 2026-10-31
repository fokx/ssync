//! Protocol 31 handshake and negotiation.

use crate::varint::{read_int, read_varint, write_int, write_varint};
use std::io::{Read, Write};

pub const PROTOCOL_VERSION: i32 = 31;
pub const MIN_PROTOCOL_VERSION: i32 = 28;
pub const MAX_PROTOCOL_VERSION: i32 = 40;

pub const CF_INC_RECURSE: i32 = 1 << 0;
pub const CF_SYMLINK_TIMES: i32 = 1 << 1;
pub const CF_SYMLINK_ICONV: i32 = 1 << 2;
pub const CF_SAFE_FLIST: i32 = 1 << 3;
pub const CF_AVOID_XATTR_OPTIM: i32 = 1 << 4;
pub const CF_CHKSUM_SEED_FIX: i32 = 1 << 5;
pub const CF_INPLACE_PARTIAL_DIR: i32 = 1 << 6;
pub const CF_VARINT_FLIST_FLAGS: i32 = 1 << 7;
pub const CF_ID0_NAMES: i32 = 1 << 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResult {
    pub protocol_version: i32,
    pub compat_flags: i32,
    pub checksum_seed: i32,
}

/// Performs the client-side protocol 31 handshake.
pub fn client_handshake<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> std::io::Result<HandshakeResult> {
    // 1. Send protocol version
    write_int(writer, PROTOCOL_VERSION)?;
    writer.flush()?;

    // 2. Read remote protocol version
    let remote_proto = read_int(reader)?;
    if !(MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION).contains(&remote_proto) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unsupported remote protocol version: {}", remote_proto),
        ));
    }
    let negotiated_proto = PROTOCOL_VERSION.min(remote_proto);

    // 3. Read compat_flags from server (if proto >= 30)
    let compat_flags = if negotiated_proto >= 30 {
        read_varint(reader)?
    } else {
        0
    };

    // 4. Read checksum seed from server
    let checksum_seed = read_int(reader)?;

    Ok(HandshakeResult {
        protocol_version: negotiated_proto,
        compat_flags,
        checksum_seed,
    })
}

/// Performs the server-side protocol 31 handshake.
pub fn server_handshake<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    compat_flags: i32,
    seed: i32,
) -> std::io::Result<HandshakeResult> {
    // 1. Send protocol version
    write_int(writer, PROTOCOL_VERSION)?;
    writer.flush()?;

    // 2. Read remote protocol version
    let remote_proto = read_int(reader)?;
    if !(MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION).contains(&remote_proto) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Unsupported remote protocol version: {}", remote_proto),
        ));
    }
    let negotiated_proto = PROTOCOL_VERSION.min(remote_proto);

    // 3. Write compat_flags to client (if proto >= 30)
    if negotiated_proto >= 30 {
        write_varint(writer, compat_flags)?;
    }

    // 4. Send checksum seed
    write_int(writer, seed)?;
    writer.flush()?;

    Ok(HandshakeResult {
        protocol_version: negotiated_proto,
        compat_flags,
        checksum_seed: seed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_exchange() {
        let mut client_to_server = Vec::new();
        let mut server_to_client = Vec::new();

        let seed = 123456789;
        let flags = CF_SAFE_FLIST | CF_CHKSUM_SEED_FIX;

        // Server writes first into server_to_client
        write_int(&mut server_to_client, PROTOCOL_VERSION).unwrap();
        write_varint(&mut server_to_client, flags).unwrap();
        write_int(&mut server_to_client, seed).unwrap();

        // Client writes into client_to_server and reads from server_to_client
        let mut s2c_cursor = std::io::Cursor::new(server_to_client);
        let client_res = client_handshake(&mut s2c_cursor, &mut client_to_server).unwrap();

        assert_eq!(client_res.protocol_version, 31);
        assert_eq!(client_res.compat_flags, flags);
        assert_eq!(client_res.checksum_seed, seed);
    }
}
