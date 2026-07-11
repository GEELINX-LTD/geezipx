//! Minimal inline SHA-1 implementation (FIPS 180-4).
//!
//! We avoid pulling in a full crypto crate for a single 20-byte hash
//! since the WIM format uses SHA-1 for stream integrity (not security).

/// Compute the SHA-1 hash of `data` and return the 20-byte digest.
pub(crate) fn compute_sha1(data: &[u8]) -> [u8; 20] {
    // Initialize hash state
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;

    let msg_len_bits: u64 = (data.len() as u64) * 8;

    // Process 512-bit (64-byte) blocks
    let num_blocks = (data.len() + 9).div_ceil(64);

    // We'll build a mutable copy with padding for the last block
    let padded_len = num_blocks * 64;
    let mut padded = Vec::with_capacity(padded_len);
    padded.extend_from_slice(data);
    padded.push(0x80);

    // Pad with zeros to make (len(data) + 1 + 8) % 64 == 0
    while padded.len() % 64 != 56 {
        padded.push(0x00);
    }

    // Append message length in bits as a 64-bit big-endian integer
    padded.extend_from_slice(&msg_len_bits.to_be_bytes());

    // Process each 512-bit block
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 80];

        // Prepare message schedule (first 16 words from the block)
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        // Extend to 80 words
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        // Main loop
        #[allow(clippy::needless_range_loop)]
        for i in 0..80 {
            let (f, k): (u32, u32) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    // Produce final digest
    let mut digest = [0u8; 20];
    digest[0..4].copy_from_slice(&h0.to_be_bytes());
    digest[4..8].copy_from_slice(&h1.to_be_bytes());
    digest[8..12].copy_from_slice(&h2.to_be_bytes());
    digest[12..16].copy_from_slice(&h3.to_be_bytes());
    digest[16..20].copy_from_slice(&h4.to_be_bytes());
    digest
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
