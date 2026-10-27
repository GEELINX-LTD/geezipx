//! SHA-1 resource digests required by the WIM format.
//!
//! Uses the shared incremental implementation without copying complete resources.

use ::sha1::{Digest, Sha1};
use std::io::{self, Write};

pub(crate) struct DigestWriter<'a> {
    inner: &'a mut dyn Write,
    digest: Sha1,
}

impl<'a> DigestWriter<'a> {
    pub(crate) fn new(inner: &'a mut dyn Write) -> Self {
        Self {
            inner,
            digest: Sha1::new(),
        }
    }

    pub(crate) fn finish(self) -> [u8; 20] {
        self.digest.finalize().into()
    }
}

impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(bytes)?;
        self.digest.update(&bytes[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Compute a WIM resource digest (format integrity, not password security).
pub(crate) fn compute_sha1(data: &[u8]) -> [u8; 20] {
    Sha1::digest(data).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_test_vector_abc() {
        let hash = compute_sha1(b"abc");
        let expected = [
            0xA9, 0x99, 0x3E, 0x36, 0x47, 0x06, 0x81, 0x6A, 0xBA, 0x3E, 0x25, 0x71, 0x78, 0x50,
            0xC2, 0x6C, 0x9C, 0xD0, 0xD8, 0x9D,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn sha1_test_vector_empty() {
        let hash = compute_sha1(b"");
        let expected = [
            0xDA, 0x39, 0xA3, 0xEE, 0x5E, 0x6B, 0x4B, 0x0D, 0x32, 0x55, 0xBF, 0xEF, 0x95, 0x60,
            0x18, 0x90, 0xAF, 0xD8, 0x07, 0x09,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn sha1_test_vector_quick_brown_fox() {
        let input = b"The quick brown fox jumps over the lazy dog";
        let hash = compute_sha1(input);
        let expected = [
            0x2F, 0xD4, 0xE1, 0xC6, 0x7A, 0x2D, 0x28, 0xFC, 0xED, 0x84, 0x9E, 0xE1, 0xBB, 0x76,
            0xE7, 0x39, 0x1B, 0x93, 0xEB, 0x12,
        ];
        assert_eq!(hash, expected);
    }
}
