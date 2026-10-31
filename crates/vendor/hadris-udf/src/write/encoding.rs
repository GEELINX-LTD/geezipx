//! OSTA CS0 serialization used by both writer entry points.
use crate::UdfRevision;
use alloc::vec::Vec;

pub(super) fn filename(name: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    if name.is_ascii() {
        bytes.push(8);
        bytes.extend_from_slice(name.as_bytes());
    } else {
        bytes.push(16);
        for unit in name.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
    }
    bytes
}

pub(super) fn charspec(buffer: &mut [u8]) {
    buffer.fill(0);
    buffer[1..24].copy_from_slice(b"OSTA Compressed Unicode");
}

pub(super) fn dstring(buffer: &mut [u8], text: &str) {
    if buffer.len() < 2 || text.is_empty() {
        return;
    }
    let encoded = filename(text);
    let mut n = encoded.len().min(buffer.len() - 1);
    if encoded[0] == 16 {
        n = 1 + (n - 1) / 2 * 2;
        // Avoid ending with a surrogate whose matching low unit was truncated.
        if n >= 3
            && (0xD800..=0xDBFF).contains(&u16::from_be_bytes([encoded[n - 2], encoded[n - 1]]))
        {
            n -= 2;
        }
    }
    buffer[..n].copy_from_slice(&encoded[..n]);
    let last = buffer.len() - 1;
    buffer[last] = n as u8;
}

pub(super) fn entity(buffer: &mut [u8], id: &[u8], revision: UdfRevision) {
    let n = id.len().min(23);
    buffer[1..1 + n].copy_from_slice(&id[..n]);
    if id.starts_with(b"*OSTA UDF") {
        // regid suffix starts at byte 24 (flags + 23 identifier bytes).
        buffer[24..26].copy_from_slice(&revision.to_raw().to_le_bytes());
    }
}
