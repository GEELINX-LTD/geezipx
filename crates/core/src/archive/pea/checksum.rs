//! PEA hash identifiers and incremental checksums from the public PEA spec.
use crate::error::{GeeZipError, GeeZipResult as Result};
use sha2::digest::DynDigest;

pub(super) enum Checksum {
    None,
    Adler(adler2::Adler32),
    Crc32(crc32fast::Hasher),
    Crc64(u64),
    Digest(Box<dyn DynDigest + Send>),
}
impl Checksum {
    pub(super) fn new(kind: u8) -> Result<Self> {
        Ok(match kind {
            0 => Self::None,
            1 => Self::Adler(adler2::Adler32::new()),
            2 => Self::Crc32(crc32fast::Hasher::new()),
            3 => Self::Crc64(u64::MAX),
            0x10 => Self::Digest(Box::<md5::Md5>::default()),
            0x11 => Self::Digest(Box::<ripemd::Ripemd160>::default()),
            0x12 => Self::Digest(Box::<sha1::Sha1>::default()),
            0x13 => Self::Digest(Box::<sha2::Sha256>::default()),
            0x14 => Self::Digest(Box::<sha2::Sha512>::default()),
            0x15 => Self::Digest(Box::<whirlpool::Whirlpool>::default()),
            0x16 => Self::Digest(Box::<sha3::Sha3_256>::default()),
            0x17 => Self::Digest(Box::<sha3::Sha3_512>::default()),
            0x18 => Self::Digest(Box::<blake2::Blake2s256>::default()),
            0x19 => Self::Digest(Box::<blake2::Blake2b512>::default()),
            _ => return Err(bad(format!("unsupported checksum algorithm {kind}"))),
        })
    }
    pub(super) fn update(&mut self, data: &[u8]) {
        match self {
            Self::None => {}
            Self::Adler(state) => state.write_slice(data),
            Self::Crc32(state) => state.update(data),
            Self::Crc64(state) => {
                for &byte in data {
                    *state = (*state << 8) ^ CRC64[(((*state >> 56) as u8) ^ byte) as usize];
                }
            }
            Self::Digest(state) => state.update(data),
        }
    }
    /// Snapshot a digest without destroying the state. Shared stream/volume
    /// checksums then hash the same payload once, and continue with trailer bytes.
    pub(super) fn value(&self) -> Vec<u8> {
        match self {
            Self::None => Vec::new(),
            Self::Adler(state) => state.checksum().to_le_bytes().to_vec(),
            Self::Crc32(state) => state.clone().finalize().to_le_bytes().to_vec(),
            Self::Crc64(state) => (!*state).to_le_bytes().to_vec(),
            Self::Digest(state) => state.box_clone().finalize().to_vec(),
        }
    }
}

const CRC64: [u64; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = (i as u64) << 56;
        let mut bit = 0;
        while bit < 8 {
            value = if value & (1u64 << 63) != 0 {
                (value << 1) ^ 0x42F0_E1EB_A9EA_3693
            } else {
                value << 1
            };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
};

pub(super) fn bad(message: impl Into<String>) -> GeeZipError {
    GeeZipError::Format {
        format: crate::detect::ArchiveFormat::Pea,
        message: message.into(),
    }
}
