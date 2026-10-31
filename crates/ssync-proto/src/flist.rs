//! Wire serialization for file lists (`flist`) in Protocol 31.

use crate::varint::{
    read_byte, read_int, read_varint, read_varlong, write_byte, write_int, write_varint,
    write_varlong,
};
use std::io::{Read, Write};

pub const XMIT_TOP_DIR: u16 = 1 << 0;
pub const XMIT_SAME_MODE: u16 = 1 << 1;
pub const XMIT_EXTENDED_FLAGS: u16 = 1 << 2;
pub const XMIT_SAME_UID: u16 = 1 << 3;
pub const XMIT_SAME_GID: u16 = 1 << 4;
pub const XMIT_SAME_NAME: u16 = 1 << 5;
pub const XMIT_LONG_NAME: u16 = 1 << 6;
pub const XMIT_SAME_TIME: u16 = 1 << 7;
pub const XMIT_MOD_NSEC: u16 = 1 << 13;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireFileEntry {
    pub name: String,
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub symlink_target: Option<String>,
}

/// Serializes a slice of file entries into the rsync wire stream.
pub fn send_wire_file_list<W: Write>(
    writer: &mut W,
    entries: &[WireFileEntry],
) -> std::io::Result<()> {
    let mut prev_name = String::new();
    let mut prev_mode = 0u32;
    let mut prev_time = 0i64;
    let mut prev_uid = 0u32;
    let mut prev_gid = 0u32;

    for entry in entries {
        let mut xflags: u16 = 0;

        // Calculate prefix overlap l1 with previous name
        let l1 = common_prefix_len(&prev_name, &entry.name);
        let l2 = entry.name.len() - l1;

        if l1 > 0 {
            xflags |= XMIT_SAME_NAME;
        }
        if l2 > 255 {
            xflags |= XMIT_LONG_NAME;
        }
        if entry.mode == prev_mode {
            xflags |= XMIT_SAME_MODE;
        }
        if entry.mtime_secs == prev_time {
            xflags |= XMIT_SAME_TIME;
        }
        if entry.mtime_nanos > 0 {
            xflags |= XMIT_MOD_NSEC;
        }
        if entry.uid == prev_uid {
            xflags |= XMIT_SAME_UID;
        }
        if entry.gid == prev_gid {
            xflags |= XMIT_SAME_GID;
        }

        // Avoid writing a 0 byte for the first flag byte (0 signals EOF)
        if (entry.mode & 0o170000) == 0o040000 || xflags == 0 {
            xflags |= XMIT_TOP_DIR;
        }

        // Write xflags
        if (xflags & 0xFF00) != 0 {
            let full_flags = xflags | XMIT_EXTENDED_FLAGS;
            write_byte(writer, (full_flags & 0xFF) as u8)?;
            write_byte(writer, ((full_flags >> 8) & 0xFF) as u8)?;
        } else {
            write_byte(writer, (xflags & 0xFF) as u8)?;
        }

        // Write name components
        if (xflags & XMIT_SAME_NAME) != 0 {
            write_byte(writer, l1 as u8)?;
        }
        if (xflags & XMIT_LONG_NAME) != 0 {
            write_varint(writer, l2 as i32)?;
        } else {
            write_byte(writer, l2 as u8)?;
        }
        writer.write_all(&entry.name.as_bytes()[l1..])?;

        // Write size (3-byte minimum varlong)
        write_varlong(writer, entry.size as i64, 3)?;

        // Write mtime
        if (xflags & XMIT_SAME_TIME) == 0 {
            write_varlong(writer, entry.mtime_secs, 4)?;
        }

        // Write nanoseconds
        if (xflags & XMIT_MOD_NSEC) != 0 {
            write_varint(writer, entry.mtime_nanos as i32)?;
        }

        // Write mode
        if (xflags & XMIT_SAME_MODE) == 0 {
            write_int(writer, entry.mode as i32)?;
        }

        // Write uid / gid
        if (xflags & XMIT_SAME_UID) == 0 {
            write_varint(writer, entry.uid as i32)?;
        }
        if (xflags & XMIT_SAME_GID) == 0 {
            write_varint(writer, entry.gid as i32)?;
        }

        // Write symlink target
        if let Some(ref target) = entry.symlink_target {
            let target_bytes = target.as_bytes();
            write_varint(writer, target_bytes.len() as i32)?;
            writer.write_all(target_bytes)?;
        }

        prev_name = entry.name.clone();
        prev_mode = entry.mode;
        prev_time = entry.mtime_secs;
        prev_uid = entry.uid;
        prev_gid = entry.gid;
    }

    // Terminal 0 signals end of file list
    write_byte(writer, 0)?;

    // Trailing io_error word in protocol >= 29
    write_int(writer, 0)?;
    writer.flush()?;

    Ok(())
}

/// Deserializes a file list from the rsync wire stream.
pub fn recv_wire_file_list<R: Read>(reader: &mut R) -> std::io::Result<Vec<WireFileEntry>> {
    let mut entries = Vec::new();

    let mut prev_name = String::new();
    let mut prev_mode = 0u32;
    let mut prev_time = 0i64;
    let mut prev_uid = 0u32;
    let mut prev_gid = 0u32;

    loop {
        let b1 = read_byte(reader)?;
        if b1 == 0 {
            break;
        }

        let mut xflags = b1 as u16;
        if (b1 & (XMIT_EXTENDED_FLAGS as u8)) != 0 {
            let b2 = read_byte(reader)?;
            xflags = (b1 as u16) | ((b2 as u16) << 8);
        }

        let l1 = if (xflags & XMIT_SAME_NAME) != 0 {
            read_byte(reader)? as usize
        } else {
            0
        };

        let l2 = if (xflags & XMIT_LONG_NAME) != 0 {
            read_varint(reader)? as usize
        } else {
            read_byte(reader)? as usize
        };

        let mut suffix = vec![0u8; l2];
        reader.read_exact(&mut suffix)?;
        let suffix_str = String::from_utf8_lossy(&suffix);

        let name = if l1 > 0 {
            format!("{}{}", &prev_name[..l1.min(prev_name.len())], suffix_str)
        } else {
            suffix_str.to_string()
        };

        let size = read_varlong(reader, 3)? as u64;

        let mtime_secs = if (xflags & XMIT_SAME_TIME) != 0 {
            prev_time
        } else {
            read_varlong(reader, 4)?
        };

        let mtime_nanos = if (xflags & XMIT_MOD_NSEC) != 0 {
            read_varint(reader)? as u32
        } else {
            0
        };

        let mode = if (xflags & XMIT_SAME_MODE) != 0 {
            prev_mode
        } else {
            read_int(reader)? as u32
        };

        let uid = if (xflags & XMIT_SAME_UID) != 0 {
            prev_uid
        } else {
            read_varint(reader)? as u32
        };

        let gid = if (xflags & XMIT_SAME_GID) != 0 {
            prev_gid
        } else {
            read_varint(reader)? as u32
        };

        let symlink_target = if (mode & 0o170000) == 0o120000 {
            let target_len = read_varint(reader)? as usize;
            let mut target_buf = vec![0u8; target_len];
            reader.read_exact(&mut target_buf)?;
            Some(String::from_utf8_lossy(&target_buf).to_string())
        } else {
            None
        };

        prev_name = name.clone();
        prev_mode = mode;
        prev_time = mtime_secs;
        prev_uid = uid;
        prev_gid = gid;

        entries.push(WireFileEntry {
            name,
            size,
            mtime_secs,
            mtime_nanos,
            mode,
            uid,
            gid,
            symlink_target,
        });
    }

    // Read trailing io_error
    let _io_error = read_int(reader)?;

    Ok(entries)
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    let mut len = 0;
    for (ca, cb) in a.bytes().zip(b.bytes()) {
        if ca == cb {
            len += 1;
        } else {
            break;
        }
    }
    len.min(255)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_file_list_roundtrip() {
        let entries = vec![
            WireFileEntry {
                name: "dir1".to_string(),
                size: 4096,
                mtime_secs: 1700000000,
                mtime_nanos: 123456,
                mode: 0o040755,
                uid: 1000,
                gid: 1000,
                symlink_target: None,
            },
            WireFileEntry {
                name: "dir1/file1.txt".to_string(),
                size: 1024,
                mtime_secs: 1700000010,
                mtime_nanos: 0,
                mode: 0o100644,
                uid: 1000,
                gid: 1000,
                symlink_target: None,
            },
            WireFileEntry {
                name: "dir1/file2.txt".to_string(),
                size: 2048,
                mtime_secs: 1700000010,
                mtime_nanos: 0,
                mode: 0o100644,
                uid: 1000,
                gid: 1000,
                symlink_target: None,
            },
            WireFileEntry {
                name: "dir1/link_to_file1".to_string(),
                size: 9,
                mtime_secs: 1700000020,
                mtime_nanos: 500,
                mode: 0o120777,
                uid: 1000,
                gid: 1000,
                symlink_target: Some("file1.txt".to_string()),
            },
        ];

        let mut buf = Vec::new();
        send_wire_file_list(&mut buf, &entries).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let read_back = recv_wire_file_list(&mut cursor).unwrap();

        assert_eq!(entries.len(), read_back.len());
        for (expected, actual) in entries.iter().zip(read_back.iter()) {
            assert_eq!(expected, actual);
        }
    }
}
