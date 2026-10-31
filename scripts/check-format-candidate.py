#!/usr/bin/env python3
"""Exercise an already-built native CLI using offline fixtures and real processes."""
import argparse
import hashlib
import io
import json
import lzma
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import tarfile
import tempfile
import zipfile
import zlib

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "crates/core/tests/fixtures"


def invoke(command, *, data=None, success=True):
    result = subprocess.run(
        [str(part) for part in command], input=data,
        stdin=subprocess.DEVNULL if data is None else None,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120,
    )
    if (result.returncode == 0) != success:
        raise RuntimeError(
            f"Unexpected exit {result.returncode}: {command}\n"
            f"{result.stdout.decode('utf-8', errors='replace')[-4000:]}\n"
            f"{result.stderr.decode('utf-8', errors='replace')[-4000:]}"
        )
    return result.stdout


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def manifest(directory):
    return {
        path.relative_to(directory).as_posix(): {
            "bytes": path.stat().st_size, "sha256": sha256(path),
        }
        for path in sorted(directory.rglob("*")) if path.is_file()
    }


def alz(data, packed):
    # Synthetic single-volume DEFLATE regression; not an independent fixture.
    return (
        b"ALZ\x01\x0a\0\0\0BLZ\x01"
        + bytes([5, 0, 0x20, 0, 0, 0, 0, 0x40, 0, 2, 0])
        + struct.pack("<III", zlib.crc32(data), len(packed), len(data))
        + b"a.txt" + packed + b"CLZ\x01\0\0\0\0CLZ\x02\0\0\0\0"
    )


def compound(data):
    # Minimal v3 CFB regular stream; MSI database semantics are not exercised.
    free, end, fat_sector = 0xFFFFFFFF, 0xFFFFFFFE, 0xFFFFFFFD
    data = data.ljust(4096, b"\0")
    header = bytearray(512)
    header[:8] = bytes.fromhex("d0cf11e0a1b11ae1")
    struct.pack_into("<HHHHH", header, 24, 0x3E, 3, 0xFFFE, 9, 6)
    struct.pack_into("<IIIIIIII", header, 44, 1, 0, 0, 4096, end, 0, end, 0)
    struct.pack_into("<109I", header, 76, 9, *([free] * 108))
    directory = bytearray(512)
    for offset, name, kind, child, start, size in [
        (0, "Root Entry", 5, 1, end, 0), (128, "payload.bin", 2, free, 1, len(data)),
    ]:
        encoded = (name + "\0").encode("utf-16le")
        directory[offset:offset + len(encoded)] = encoded
        struct.pack_into("<HBBIII", directory, offset + 64, len(encoded), kind, 1, free, free, child)
        struct.pack_into("<IQ", directory, offset + 116, start, size)
    fat = [end, *range(2, 9), end, fat_sector] + [free] * 118
    return bytes(header + directory) + data + struct.pack("<128I", *fat), data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--report", type=Path, default=Path("candidate-report.json"))
    parser.add_argument("--sfx", action="store_true", help="Execute a native SFX made by this CLI")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    report = {
        "source_sha": os.environ.get("GITHUB_SHA", "local"),
        "platform": platform.system(), "architecture": platform.machine(),
        "features": os.environ.get("GEEZIPX_CANDIDATE_FEATURES", "unspecified"),
        "binary_sha256": sha256(binary),
        "version": invoke([binary, "--version"]).decode().strip(),
        "checks": [], "ok": False,
    }

    def cli(*arguments, **kwargs):
        return invoke([binary, *arguments], **kwargs)

    def record(label, output=None):
        report["checks"].append({"name": label, **({"files": manifest(output)} if output else {})})
        print(f"PASS {label}", flush=True)

    with tempfile.TemporaryDirectory(prefix="geezipx-candidate-") as temporary:
        work = Path(temporary) / "中文 路径"
        work.mkdir()
        payload = "Native CLI candidate 中文\n".encode() + bytes(range(256)) * 32
        source = work / "源文件.txt"
        source.write_bytes(payload)

        def extract(label, archive, *, password=None, expected=None, crc=None, digest=None):
            options = ["--password", password] if password is not None else []
            test = json.loads(cli("test", archive, "--json", *options))
            if test.get("ok") is not True:
                raise AssertionError((label, test))
            listing = json.loads(cli("list", archive, "--json", *options))
            if not isinstance(listing, list) or not listing:
                raise AssertionError((label, listing))
            output = work / (label + "-out")
            cli("decompress", archive, "-o", output, *options)
            files = sorted(path for path in output.rglob("*") if path.is_file())
            if expected is not None:
                if len(files) != 1 or files[0].read_bytes() != expected:
                    raise AssertionError(f"{label}: extracted contents differ")
            if crc is not None:
                actual = sorted((p.stat().st_size, zlib.crc32(p.read_bytes())) for p in files)
                if actual != sorted(crc):
                    raise AssertionError((label, actual, crc))
            if digest is not None and (len(files) != 1 or sha256(files[0]) != digest):
                raise AssertionError(f"{label}: extracted digest differs")
            record(label, output)
            return output

        try:
            for extension, format_name in [
                ("zip", "zip"), ("zipx", "zipx"), ("7z", "7z"),
                ("tar", "tar"), ("tgz", "tgz"), ("lzh", "lzh"),
                ("iso", "iso"), ("gz", "gzip"), ("xz", "xz"),
                ("uu", "uu"), ("uue", "uue"), ("xxe", "xxe"),
            ]:
                archive = work / ("压缩样本." + extension)
                options = ["--level", "9"] if format_name == "lzh" else []
                cli("compress", source, "-o", archive, "-f", format_name, "--no-progress", *options)
                if extension == "zipx":
                    with zipfile.ZipFile(archive) as zipped:
                        if any(entry.compress_type != 95 for entry in zipped.infolist()):
                            raise AssertionError("ZIPX did not use XZ method 95")
                extract("roundtrip-" + extension, archive, expected=payload)

            for name, expected in [
                ("lzma5.egg", [(0, 0), (95, 0x6A94D1BC), (2060, 0xB54CCAB2), (4096, 0x1BB92DFC), (3150, 0x7A553A9D)]),
                ("posix.egg", [(54, 0x96E10D03), (25, 0x6C7B95E7), (19, 0xB3010A13)]),
                ("empty_noblock.egg", [(0, 0)]),
            ]:
                archive = work / name
                shutil.copyfile(FIXTURES / "egg" / name, archive)
                extract("fixture-" + name, archive, crc=expected)
            for name, expected in [("pm1.pma", [(32768, 0x41FE9947)]), ("pm2.pma", [(128, 0x906330C5)])]:
                archive = work / name
                shutil.copyfile(FIXTURES / name, archive)
                extract("fixture-" + name, archive, crc=expected)
            for name, expected in [
                ("ncompress.Z", bytes((i * 73 + i // 13) % 256 for i in range(16384))),
                ("lzip/gnu-1.25.lz", bytes(range(256)) * 1024 + b"GNU lzip independent interoperability\n"),
            ]:
                archive = work / Path(name).name
                shutil.copyfile(FIXTURES / name, archive)
                extract("fixture-" + archive.name, archive, expected=expected)
            archive = work / "ultraiso.isz"
            shutil.copyfile(FIXTURES / "isz/ultraiso.isz", archive)
            extract("fixture-isz", archive, digest="aa8ea632b219083c93ad46998bf3383d273d514f9807eb5127f6e8449e68c1b5")

            # The cooked ISO is also a BIN/IMG content-view fixture.
            iso = work / "原始镜像.iso"
            iso.write_bytes(cli("decompress", archive, "--stdout"))
            for suffix in ["img", "bin"]:
                view = iso.with_suffix("." + suffix)
                shutil.copyfile(iso, view)
                extract("view-" + suffix, view, digest="aa8ea632b219083c93ad46998bf3383d273d514f9807eb5127f6e8449e68c1b5")
            # TLZ uses Python's independent TAR/LZMA implementation as producer.
            tlz = work / "nested.tlz"
            tar_buffer = io.BytesIO()
            with tarfile.open(fileobj=tar_buffer, mode="w", format=tarfile.USTAR_FORMAT) as tar:
                member = tarfile.TarInfo("nested/payload.txt")
                member.size = len(payload)
                tar.addfile(member, io.BytesIO(payload))
            tlz.write_bytes(lzma.compress(tar_buffer.getvalue(), format=lzma.FORMAT_ALONE))
            extract("fixture-tlz", tlz, expected=payload)

            for version in range(3):
                for size in [0, 16, 17]:
                    name = f"v{version}-{size}.aes"
                    archive = work / name
                    shutil.copyfile(FIXTURES / "aescrypt" / name, archive)
                    expected = (b"0123456789ABCDEF0")[:size]
                    extract("fixture-" + name, archive, password="Hello", expected=expected)
            archive = work / "official-v3.aes"
            shutil.copyfile(FIXTURES / "aescrypt/official-v3.aes", archive)
            extract("fixture-aes-v3", archive, password="Unicode 密码 🔑", expected=b"Official AES Crypt v3 fixture\n")
            protected = work / "password-protected"
            protected.mkdir()
            (protected / "official-v3").write_bytes(b"existing destination")
            before = manifest(protected)
            cli("decompress", archive, "-o", protected, "--password", "wrong", success=False)
            if manifest(protected) != before:
                raise AssertionError("wrong password changed destination")
            record("wrong-password-protects-destination")

            encoder = zlib.compressobj(wbits=-15)
            packed = encoder.compress(payload) + encoder.flush()
            archive = work / "synthetic.alz"
            archive.write_bytes(alz(payload, packed))
            extract("synthetic-alz", archive, expected=payload)
            nonfinal = b"\0" + struct.pack("<HH", len(payload), len(payload) ^ 0xFFFF) + payload
            archive.write_bytes(alz(payload, nonfinal))
            cli("test", archive, "--json", success=False)
            record("nonfinal-alz-rejected")
            archive = work / "synthetic.msi"
            cfb, expected = compound(payload[:100])
            archive.write_bytes(cfb)
            extract("synthetic-cfb-stream", archive, expected=expected)

            broken = work / "damaged.lz"
            broken.write_bytes((FIXTURES / "lzip/gnu-1.25.lz").read_bytes()[:-1])
            cli("test", broken, "--json", success=False)
            output = work / "damaged-out"
            output.mkdir()
            (output / "damaged").write_bytes(b"existing destination")
            before = manifest(output)
            cli("decompress", broken, "-o", output, success=False)
            if manifest(output) != before:
                raise AssertionError("corrupt Lzip changed destination")
            record("corrupt-stream-protects-destination")

            if args.sfx:
                executable = work / "自解压.exe"
                cli("compress", source, "-o", executable, "-f", "zip", "--sfx", "--no-progress")
                if os.name != "nt":
                    executable.chmod(0o755)
                output = work / "sfx-native"
                invoke([executable, output])
                if (output / source.name).read_bytes() != payload:
                    raise AssertionError("native SFX contents differ")
                record("native-sfx", output)
            report["ok"] = True
        except Exception as error:
            report["error"] = str(error)
            raise
        finally:
            args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Verified {len(report['checks'])} native CLI checks; report: {args.report}")


if __name__ == "__main__":
    main()
