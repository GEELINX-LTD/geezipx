use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::io::Cursor;
use std::path::PathBuf;

use geezipx_core::archive::targz::{TarGzReader, TarGzWriter};
use geezipx_core::archive::zip::{ZipReader, ZipWriter};
use geezipx_core::archive::{ArchiveReader, ArchiveWriter};

/// Generate deterministic semi-compressible data at the requested size.
fn generate_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push((i ^ 0xAB).wrapping_mul(7) as u8);
    }
    data
}

/// Helper: a file (name + data) for archive construction.
struct ArchiveFile {
    name: String,
    data: Vec<u8>,
}

/// Dataset: 10 files x 1 KiB each.
fn dataset_10x1k() -> Vec<ArchiveFile> {
    (0..10)
        .map(|i| ArchiveFile {
            name: format!("file_{i}.txt"),
            data: generate_data(1024),
        })
        .collect()
}

/// Dataset: 1 file x 1 MiB.
fn dataset_1x1m() -> Vec<ArchiveFile> {
    vec![ArchiveFile {
        name: "large.bin".into(),
        data: generate_data(1024 * 1024),
    }]
}

fn total_size(files: &[ArchiveFile]) -> u64 {
    files.iter().map(|f| f.data.len() as u64).sum()
}

// ---------------------------------------------------------------------------
// Helper: pre-create archive bytes (used by decompress benchmarks)
// ---------------------------------------------------------------------------

fn prepare_targz(files: &[ArchiveFile]) -> Vec<u8> {
    let mut writer = TarGzWriter::new(Vec::new());
    for f in files {
        writer
            .add_entry_from_reader(&PathBuf::from(&f.name), &mut Cursor::new(&f.data))
            .unwrap();
    }
    let (_, data) = writer.finalize().unwrap();
    data
}

fn prepare_zip(files: &[ArchiveFile]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    for f in files {
        writer
            .add_entry_from_reader(&PathBuf::from(&f.name), &mut Cursor::new(&f.data))
            .unwrap();
    }
    let (_, cursor) = writer.finalize().unwrap();
    cursor.into_inner()
}

// ---------------------------------------------------------------------------
// TarGz compress
// ---------------------------------------------------------------------------

fn bench_targz_compress(c: &mut Criterion) {
    let d10 = dataset_10x1k();
    let d1m = dataset_1x1m();
    let mut group = c.benchmark_group("targz_compress");

    group.throughput(Throughput::Bytes(total_size(&d10)));
    group.bench_function("10x1k", |b| {
        b.iter(|| {
            let mut writer = TarGzWriter::new(Vec::new());
            for f in &d10 {
                writer
                    .add_entry_from_reader(&PathBuf::from(&f.name), &mut Cursor::new(&f.data))
                    .unwrap();
            }
            black_box(writer.finalize().unwrap());
        });
    });

    group.throughput(Throughput::Bytes(total_size(&d1m)));
    group.bench_function("1x1m", |b| {
        b.iter(|| {
            let mut writer = TarGzWriter::new(Vec::new());
            for f in &d1m {
                writer
                    .add_entry_from_reader(&PathBuf::from(&f.name), &mut Cursor::new(&f.data))
                    .unwrap();
            }
            black_box(writer.finalize().unwrap());
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// TarGz decompress
// ---------------------------------------------------------------------------

fn bench_targz_decompress(c: &mut Criterion) {
    let d10 = dataset_10x1k();
    let d1m = dataset_1x1m();

    // Pre-create archive bytes outside the measured loop.
    let archive_10x1k = prepare_targz(&d10);
    let archive_1x1m = prepare_targz(&d1m);

    let mut group = c.benchmark_group("targz_decompress");

    group.throughput(Throughput::Bytes(total_size(&d10)));
    group.bench_function("10x1k", |b| {
        b.iter(|| {
            let mut reader = TarGzReader::new(Cursor::new(archive_10x1k.as_slice()));
            let entries = reader.entries().unwrap();
            for entry in &entries {
                let mut output = Vec::new();
                black_box(reader.extract(entry, &mut output).unwrap());
            }
        });
    });

    group.throughput(Throughput::Bytes(total_size(&d1m)));
    group.bench_function("1x1m", |b| {
        b.iter(|| {
            let mut reader = TarGzReader::new(Cursor::new(archive_1x1m.as_slice()));
            let entries = reader.entries().unwrap();
            for entry in &entries {
                let mut output = Vec::new();
                black_box(reader.extract(entry, &mut output).unwrap());
            }
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// ZIP compress
// ---------------------------------------------------------------------------

fn bench_zip_compress(c: &mut Criterion) {
    let d10 = dataset_10x1k();
    let d1m = dataset_1x1m();
    let mut group = c.benchmark_group("zip_compress");

    group.throughput(Throughput::Bytes(total_size(&d10)));
    group.bench_function("10x1k", |b| {
        b.iter(|| {
            let cursor = Cursor::new(Vec::new());
            let mut writer = ZipWriter::new(cursor);
            for f in &d10 {
                writer
                    .add_entry_from_reader(&PathBuf::from(&f.name), &mut Cursor::new(&f.data))
                    .unwrap();
            }
            black_box(writer.finalize().unwrap());
        });
    });

    group.throughput(Throughput::Bytes(total_size(&d1m)));
    group.bench_function("1x1m", |b| {
        b.iter(|| {
            let cursor = Cursor::new(Vec::new());
            let mut writer = ZipWriter::new(cursor);
            for f in &d1m {
                writer
                    .add_entry_from_reader(&PathBuf::from(&f.name), &mut Cursor::new(&f.data))
                    .unwrap();
            }
            black_box(writer.finalize().unwrap());
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// ZIP decompress
// ---------------------------------------------------------------------------

fn bench_zip_decompress(c: &mut Criterion) {
    let d10 = dataset_10x1k();
    let d1m = dataset_1x1m();

    // Pre-create archive bytes outside the measured loop.
    let archive_10x1k = prepare_zip(&d10);
    let archive_1x1m = prepare_zip(&d1m);

    let mut group = c.benchmark_group("zip_decompress");

    group.throughput(Throughput::Bytes(total_size(&d10)));
    group.bench_function("10x1k", |b| {
        b.iter(|| {
            let mut reader = ZipReader::new(Cursor::new(archive_10x1k.as_slice())).unwrap();
            let entries = reader.entries().unwrap();
            for entry in &entries {
                let mut output = Vec::new();
                black_box(reader.extract(entry, &mut output).unwrap());
            }
        });
    });

    group.throughput(Throughput::Bytes(total_size(&d1m)));
    group.bench_function("1x1m", |b| {
        b.iter(|| {
            let mut reader = ZipReader::new(Cursor::new(archive_1x1m.as_slice())).unwrap();
            let entries = reader.entries().unwrap();
            for entry in &entries {
                let mut output = Vec::new();
                black_box(reader.extract(entry, &mut output).unwrap());
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_targz_compress,
    bench_targz_decompress,
    bench_zip_compress,
    bench_zip_decompress
);
criterion_main!(benches);
