//! Abstraction over the USB bulk endpoints so the server can be tested without hardware.

pub mod usb;

use async_trait::async_trait;

/// Errors that can occur while reading or writing bulk data.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("usb error: {0}")]
    Usb(String),
    #[error("short read: expected {expected} bytes, got {got}")]
    ShortRead { expected: usize, got: usize },
}

/// A bidirectional bulk endpoint pair.
#[async_trait]
pub trait Transport: Send {
    /// Read exactly `len` bytes.
    async fn read(&mut self, len: usize) -> Result<Vec<u8>, TransportError>;
    /// Write all of `data`.
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError>;
}

/// Round `n` up to the next multiple of `m` (USB IN transfers must request a
/// multiple of the endpoint's max packet size). Never returns 0 for a nonzero `m`.
pub(crate) fn round_up(n: usize, m: usize) -> usize {
    if m == 0 {
        return n;
    }
    n.div_ceil(m).max(1) * m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_up_to_packet_size() {
        assert_eq!(round_up(0, 512), 512);
        assert_eq!(round_up(1, 512), 512);
        assert_eq!(round_up(512, 512), 512);
        assert_eq!(round_up(513, 512), 1024);
        assert_eq!(round_up(16, 512), 512);
    }
}
