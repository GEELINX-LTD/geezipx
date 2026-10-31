#!/usr/bin/env python3
"""Generate independent NSIS 3 Unicode fixtures; inspect them without running them."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--workdir", type=Path)
args = parser.parse_args()
root = args.workdir or Path(tempfile.mkdtemp(prefix="geezipx-nsis-fixtures-"))
root = root.resolve()
root.mkdir(parents=True, exist_ok=True)
source = root / "source"
source.mkdir(exist_ok=True)
compiler = shutil.which("makensis")
reference = shutil.which("7z") or shutil.which("7zz")
if not compiler or not reference:
    raise SystemExit("Requires makensis and 7z/7zz on PATH")
files = {
    "hello.txt": b"GeeZipX independent NSIS fixture\n",
    "empty.bin": b"",
    "中文 Ω.txt": "Unicode 中文 Ω\n".encode() * 101,
    "mixed.bin": random.Random(73021).randbytes(196609) + b"\0" * 196609,
}
for name, data in files.items():
    (source / name).write_bytes(data)
manifest = {name: {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
            for name, data in files.items()}
manifest["compiler_version"] = subprocess.check_output([compiler, "-VERSION"], text=True).strip()
manifest["compiler_sha256"] = hashlib.sha256(Path(compiler).read_bytes()).hexdigest()
fixtures = root / "fixtures"
fixtures.mkdir(exist_ok=True)
for codec, solid in [(c, s) for c in ("zlib", "bzip2", "lzma") for s in (False, True)] + [("store", False)]:
    name = codec + ("-solid" if solid else "-nonsolid")
    output = fixtures / (name + ".exe")
    lines = ["Unicode true", 'Name "GeeZipX independent fixture"', f'OutFile "{output}"',
             "RequestExecutionLevel user", 'InstallDir "$TEMP\\GeeZipX fixture"',
             "SilentInstall silent", "CRCCheck force",
             "SetCompressor /FINAL " + ("/SOLID " if solid else "") + ("zlib" if codec == "store" else codec),
             "SetCompress " + ("off" if codec == "store" else "force"),
             "Section", '  SetOutPath "$INSTDIR"']
    for filename in ["hello.txt", "empty.bin", "mixed.bin"]:
        lines.append(f'  File "/oname={filename}" "{source / filename}"')
    lines += ['  SetOutPath "$INSTDIR\\nested"',
              f'  File "/oname=中文 Ω.txt" "{source / "中文 Ω.txt"}"',
              '  CreateDirectory "$INSTDIR\\empty"', "SectionEnd", ""]
    recipe = root / (name + ".nsi")
    recipe.write_text("\n".join(lines), encoding="utf-8")
    result = subprocess.run([compiler, "-V3", str(recipe)], capture_output=True, timeout=60)
    (root / (name + "-build.log")).write_bytes(result.stdout + result.stderr)
    result.check_returncode()
    listing = subprocess.run([reference, "l", "-slt", str(output)], capture_output=True, check=True)
    (root / (name + "-7z-list.txt")).write_bytes(listing.stdout)
    destination = root / "reference" / name
    destination.mkdir(parents=True, exist_ok=True)
    subprocess.run([reference, "x", "-y", str(output), f"-o{destination}"],
                   capture_output=True, check=True, timeout=30)
    for filename, data in files.items():
        relative = "nested/" + filename if filename == "中文 Ω.txt" else filename
        if (destination / relative).read_bytes() != data:
            raise AssertionError((name, relative))
    manifest[name] = {"bytes": output.stat().st_size,
                      "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                      "reference_empty_directory": (destination / "empty").is_dir()}
    shutil.copyfile(output, fixtures / (name + ".bin"))
(root / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"Independent fixtures and reference extraction: {root}")
