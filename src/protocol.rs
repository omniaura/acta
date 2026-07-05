//! Framing protocol spoken over each session's unix domain socket.
//!
//! Every frame is `[type: u8][len: u32 BE][payload: len bytes]`.

use anyhow::{bail, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Client -> daemon: raw bytes destined for the agent's PTY stdin.
pub const C2D_STDIN: u8 = 0x01;
/// Client -> daemon: terminal resize; payload is `[cols: u16 BE][rows: u16 BE]`.
pub const C2D_RESIZE: u8 = 0x02;
/// Client -> daemon: client is detaching; the session keeps running.
pub const C2D_DETACH: u8 = 0x03;
/// Client -> daemon: terminate the agent process.
pub const C2D_KILL: u8 = 0x04;

/// Daemon -> client: raw PTY output bytes.
pub const D2C_OUTPUT: u8 = 0x11;
/// Daemon -> client: the agent exited; payload is the exit code as a decimal string.
pub const D2C_EXIT: u8 = 0x12;

/// Largest frame we will accept; PTY reads are far smaller than this.
const MAX_FRAME: u32 = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct Frame {
    pub kind: u8,
    pub payload: Vec<u8>,
}

pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, kind: u8, payload: &[u8]) -> Result<()> {
    let mut header = [0u8; 5];
    header[0] = kind;
    header[1..5].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    w.write_all(&header).await?;
    w.write_all(payload).await?;
    w.flush().await?;
    Ok(())
}

/// Read one frame; returns `None` on clean EOF at a frame boundary.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<Frame>> {
    let mut header = [0u8; 5];
    match r.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(header[1..5].try_into().unwrap());
    if len > MAX_FRAME {
        bail!("frame too large: {len} bytes");
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload).await?;
    Ok(Some(Frame {
        kind: header[0],
        payload,
    }))
}

pub fn encode_resize(cols: u16, rows: u16) -> [u8; 4] {
    let mut buf = [0u8; 4];
    buf[0..2].copy_from_slice(&cols.to_be_bytes());
    buf[2..4].copy_from_slice(&rows.to_be_bytes());
    buf
}

pub fn decode_resize(payload: &[u8]) -> Option<(u16, u16)> {
    if payload.len() != 4 {
        return None;
    }
    let cols = u16::from_be_bytes([payload[0], payload[1]]);
    let rows = u16::from_be_bytes([payload[2], payload[3]]);
    Some((cols, rows))
}
