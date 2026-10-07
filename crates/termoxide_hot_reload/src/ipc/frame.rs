use std::io::{Read, Write};

use super::error::{IpcError, Result};

/// Size, in bytes, of a frame's length prefix.
pub(super) const HEADER_LEN: usize = std::mem::size_of::<usize>();

/// Largest payload, in bytes, a frame may carry.
///
/// Chosen so a whole frame (header + payload) never exceeds 16 MiB. Guards
/// against a corrupt length prefix making the reader allocate unbounded memory.
pub(super) const MAX_PAYLOAD_LEN: usize = (16 * 1024 * 1024) - HEADER_LEN;

/// Writes `payload` to `writer` as one frame: its length, then its bytes.
///
/// The frame is assembled in a freshly allocated buffer and sent with a single
/// `write_all`. Fails with `PayloadTooLarge` if `payload` exceeds
/// [`MAX_PAYLOAD_LEN`], and with `Closed` if the peer has closed its end.
pub(super) fn write_frame(writer: &mut impl Write, payload: &[u8]) -> Result<()> {
    let len = payload.len();
    if len > MAX_PAYLOAD_LEN {
        return Err(IpcError::PayloadTooLarge { len, max: MAX_PAYLOAD_LEN });
    }

    let mut frame = Vec::with_capacity(HEADER_LEN + len);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(payload);

    writer.write_all(&frame)?;
    writer.flush()?;
    Ok(())
}

/// Reads one whole frame from `reader` and returns its payload.
///
/// Fails with `Closed` if the stream ends before the frame is complete, and
/// with `PayloadTooLarge` if the announced length exceeds [`MAX_PAYLOAD_LEN`].
pub(super) fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>> {
    let len = {
        let mut header = [0u8; HEADER_LEN];
        reader.read_exact(&mut header)?;
        usize::from_le_bytes(header)
    };

    // Checked before allocating, so a corrupt header can't trigger a huge allocation.
    if len > MAX_PAYLOAD_LEN {
        return Err(IpcError::PayloadTooLarge { len, max: MAX_PAYLOAD_LEN });
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    Ok(payload)
}
