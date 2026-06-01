use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::io::Cursor;

use geezipx_core::archive::gzip::{gzip_compress_with_level, gzip_decompress};

/// Generate deterministic semi-compressible data at the requested size.
fn generate_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push((i ^ 0xAB).wrapping_mul(7) as u8);
    }
    data
}

// ---------------------------------------------------------------------------
// Gzip compress
// ---------------------------------------------------------------------------

fn bench_gzip_compress(c: &mut Criterion) {
    let sizes: [usize; 2] = [1024, 1024 * 1024]; // 1 KiB, 1 MiB
    let levels: [Option<u32>; 4] = [None, Some(0), Some(6), Some(9)];

    for size in &sizes {
        let data = generate_data(*size);

        for level in &levels {
            let level_label = match level {
                None => "default",
                Some(0) => "level_0",
                Some(6) => "level_6",
                Some(9) => "level_9",
                _ => unreachable!(),
            };
            let bench_id = format!("{level_label}_{size}");

            let mut group = c.benchmark_group("gzip_compress");
            group.throughput(Throughput::Bytes(*size as u64));
            group.bench_function(&bench_id, |b| {
                b.iter(|| {
                    let mut reader = Cursor::new(data.as_slice());
                    let writer = Vec::new();
                    black_box(gzip_compress_with_level(&mut reader, writer, *level).unwrap());
                });
            });
            group.finish();
        }
    }
}

// ---------------------------------------------------------------------------
// Gzip decompress
// ---------------------------------------------------------------------------

fn bench_gzip_decompress(c: &mut Criterion) {
    let sizes: [usize; 2] = [1024, 1024 * 1024];
    let levels: [Option<u32>; 4] = [None, Some(0), Some(6), Some(9)];

    // Pre-compress data outside the measured loop.
    for size in &sizes {
        let data = generate_data(*size);

        for level in &levels {
            let mut buf = Vec::new();
            {
                let mut reader = Cursor::new(data.as_slice());
                gzip_compress_with_level(&mut reader, &mut buf, *level).unwrap();
            }
            let compressed = buf;

            let level_label = match level {
                None => "default",
                Some(0) => "level_0",
                Some(6) => "level_6",
                Some(9) => "level_9",
                _ => unreachable!(),
            };
            let bench_id = format!("{level_label}_{size}");

            let mut group = c.benchmark_group("gzip_decompress");
            group.throughput(Throughput::Bytes(*size as u64));
            group.bench_function(&bench_id, |b| {
                b.iter(|| {
                    let mut reader = Cursor::new(compressed.as_slice());
                    let mut writer = Vec::new();
                    black_box(gzip_decompress(&mut reader, &mut writer).unwrap());
                });
            });
            group.finish();
        }
    }
}

criterion_group!(benches, bench_gzip_compress, bench_gzip_decompress);
criterion_main!(benches);
