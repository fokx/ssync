//! Multiplexed I/O framing constants, reader, and writer.

use std::io::{self, Read, Write};

/// The base tag offset used in rsync multiplex framing.
pub const MPLEX_BASE: u8 = 7;

/// Maximum payload length in a single multiplex frame (24 bits).
pub const MAX_MPLEX_LEN: usize = 0x00FF_FFFF;

/// Default chunk size when streaming data packets.
pub const DEFAULT_CHUNK_SIZE: usize = 32768;

/// Multiplexed message codes matching upstream `enum msgcode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MsgCode {
    Data = 0,
    ErrorXfer = 1,
    Info = 2,
    Error = 3,
    Warning = 4,
    Redo = 9,
    Stats = 10,
    BlockStats = 11,
    IoError = 22,
    IoTimeout = 33,
    Noop = 42,
    ErrorExit = 86,
    Success = 100,
    Deleted = 101,
    NoSend = 102,
}

impl MsgCode {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(MsgCode::Data),
            1 => Some(MsgCode::ErrorXfer),
            2 => Some(MsgCode::Info),
            3 => Some(MsgCode::Error),
            4 => Some(MsgCode::Warning),
            9 => Some(MsgCode::Redo),
            10 => Some(MsgCode::Stats),
            11 => Some(MsgCode::BlockStats),
            22 => Some(MsgCode::IoError),
            33 => Some(MsgCode::IoTimeout),
            42 => Some(MsgCode::Noop),
            86 => Some(MsgCode::ErrorExit),
            100 => Some(MsgCode::Success),
            101 => Some(MsgCode::Deleted),
            102 => Some(MsgCode::NoSend),
            _ => None,
        }
    }
}

/// A writer that frames outgoing data into rsync multiplexed packets.
pub struct MplexWriter<W: Write> {
    inner: W,
    chunk_size: usize,
}

impl<W: Write> MplexWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    /// Sets the maximum chunk size for MSG_DATA frames.
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size.min(MAX_MPLEX_LEN);
        self
    }

    /// Sends a discrete message frame with a specific message code.
    pub fn send_msg(&mut self, code: MsgCode, payload: &[u8]) -> io::Result<()> {
        if payload.len() > MAX_MPLEX_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Payload exceeds maximum multiplex frame length",
            ));
        }
        let tag = (((MPLEX_BASE + (code as u8)) as u32) << 24) | (payload.len() as u32);
        self.inner.write_all(&tag.to_le_bytes())?;
        self.inner.write_all(payload)?;
        self.inner.flush()?;
        Ok(())
    }

    /// Unwraps the inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for MplexWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let chunk_len = buf.len().min(self.chunk_size);
        let chunk = &buf[..chunk_len];
        let tag = (((MPLEX_BASE + (MsgCode::Data as u8)) as u32) << 24) | (chunk_len as u32);
        self.inner.write_all(&tag.to_le_bytes())?;
        self.inner.write_all(chunk)?;
        Ok(chunk_len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A reader that unpacks incoming rsync multiplexed packets and filters out control messages.
pub struct MplexReader<R: Read> {
    inner: R,
    data_buf: Vec<u8>,
    pos: usize,
}

impl<R: Read> MplexReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            data_buf: Vec::new(),
            pos: 0,
        }
    }

    /// Reads until a MSG_DATA chunk is available or EOF. Non-data frames are handled/printed.
    fn fetch_next_data(&mut self) -> io::Result<bool> {
        loop {
            let mut tag_buf = [0u8; 4];
            match self.inner.read_exact(&mut tag_buf) {
                Ok(()) => {}
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(e) => return Err(e),
            }

            let tag = u32::from_le_bytes(tag_buf);
            let len = (tag & 0x00FF_FFFF) as usize;
            let code_byte = ((tag >> 24) as i32) - (MPLEX_BASE as i32);

            if code_byte < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid multiplex tag: {:#x}", tag),
                ));
            }

            let mut payload = vec![0u8; len];
            self.inner.read_exact(&mut payload)?;

            match MsgCode::from_u8(code_byte as u8) {
                Some(MsgCode::Data) => {
                    self.data_buf = payload;
                    self.pos = 0;
                    return Ok(true);
                }
                Some(MsgCode::Info) => {
                    let msg = String::from_utf8_lossy(&payload);
                    print!("{}", msg);
                    let _ = io::stdout().flush();
                }
                Some(MsgCode::Error) | Some(MsgCode::ErrorXfer) | Some(MsgCode::Warning) => {
                    let msg = String::from_utf8_lossy(&payload);
                    eprint!("{}", msg);
                    let _ = io::stderr().flush();
                }
                Some(MsgCode::ErrorExit) => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "Remote end signaled ErrorExit",
                    ));
                }
                Some(MsgCode::Noop) | Some(MsgCode::Stats) | Some(MsgCode::Success) => {
                    // Ignored or handled internally
                }
                _ => {
                    // Unhandled message code
                }
            }
        }
    }
}

impl<R: Read> Read for MplexReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.data_buf.len() && !self.fetch_next_data()? {
            return Ok(0);
        }

        let available = self.data_buf.len() - self.pos;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.data_buf[self.pos..self.pos + to_copy]);
        self.pos += to_copy;
        Ok(to_copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mplex_write_and_read() {
        let mut pipe = Vec::new();
        {
            let mut writer = MplexWriter::new(&mut pipe);
            writer.write_all(b"hello world").unwrap();
            writer.send_msg(MsgCode::Noop, b"ping").unwrap();
            writer.write_all(b"second data").unwrap();
        }

        let mut reader = MplexReader::new(std::io::Cursor::new(pipe));
        let mut out = String::new();
        reader.read_to_string(&mut out).unwrap();
        assert_eq!(out, "hello worldsecond data");
    }
}
