//! Stored segment checksums are verified before returning path-selected data.
use super::{
    FfiReader, FfiWriter, Result, SharedVecWriter, SinkWriter, ZpaqError, clear_last_error,
    err_from_last, sys,
};
use std::io::Cursor;

struct Decoder {
    raw: *mut sys::Decompresser,
    sha1: *mut sys::SHA1,
}
impl Drop for Decoder {
    fn drop(&mut self) {
        // Decoder borrows the digest; destroy it first. Both constructors
        // return uniquely owned handles and the C++ deletes accept null.
        unsafe {
            sys::zpaq_decompresser_free(self.raw);
            sys::zpaq_sha1_free(self.sha1);
        }
    }
}
fn check(rc: i32) -> Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(err_from_last())
    }
}

pub(super) fn read_named_stream(archive: &[u8], path: &str) -> Result<Option<Vec<u8>>> {
    clear_last_error();
    let input = FfiReader::new(Cursor::new(archive))?;
    // All callback handles outlive the synchronous calls using them. No raw
    // handle escapes this function, and guards free it on every error path.
    let decoder = Decoder {
        raw: unsafe { sys::zpaq_decompresser_new() },
        sha1: unsafe { sys::zpaq_sha1_new() },
    };
    if decoder.raw.is_null() || decoder.sha1.is_null() {
        return Err(err_from_last());
    }
    unsafe {
        check(sys::zpaq_decompresser_set_input(decoder.raw, input.raw))?;
    }
    let mut found = None;
    let mut memory = 0.0;
    loop {
        match unsafe { sys::zpaq_decompresser_find_block(decoder.raw, &mut memory) } {
            0 => break,
            1 => (),
            _ => return Err(err_from_last()),
        }
        loop {
            let filename = SharedVecWriter::new();
            let filename_writer = FfiWriter::new(filename.clone())?;
            match unsafe { sys::zpaq_decompresser_find_filename(decoder.raw, filename_writer.raw) }
            {
                0 => break,
                1 => (),
                _ => return Err(err_from_last()),
            }
            unsafe {
                check(sys::zpaq_writer_flush(filename_writer.raw))?;
            }
            let mut name = filename.bytes();
            while name.last() == Some(&0) {
                name.pop();
            }
            let target = String::from_utf8_lossy(&name) == path;
            let comment = FfiWriter::new(SinkWriter)?;
            unsafe {
                check(sys::zpaq_decompresser_read_comment(
                    decoder.raw,
                    comment.raw,
                ))?;
            }
            let bytes = SharedVecWriter::new();
            let sink: Box<dyn std::io::Write + Send> = if target {
                Box::new(bytes.clone())
            } else {
                Box::new(SinkWriter)
            };
            let output = FfiWriter::new(sink)?;
            unsafe {
                check(sys::zpaq_decompresser_set_output(decoder.raw, output.raw))?;
                check(sys::zpaq_decompresser_set_sha1(decoder.raw, decoder.sha1))?;
            }
            loop {
                match unsafe { sys::zpaq_decompresser_decompress(decoder.raw, 1 << 20) } {
                    0 => break,
                    1 => (),
                    _ => return Err(err_from_last()),
                }
            }
            let mut stored = [0u8; 21];
            let mut actual = [0u8; 20];
            unsafe {
                check(sys::zpaq_decompresser_read_segment_end(
                    decoder.raw,
                    stored.as_mut_ptr(),
                ))?;
                // result resets SHA1, including for legacy segments with no hash.
                check(sys::zpaq_sha1_result(decoder.sha1, actual.as_mut_ptr()))?;
                check(sys::zpaq_writer_flush(output.raw))?;
            }
            if stored[0] == 1 && stored[1..] != actual {
                return Err(ZpaqError::Ffi("ZPAQ segment checksum mismatch".into()));
            }
            if target {
                found = Some(bytes.bytes());
            }
        }
    }
    Ok(found)
}
