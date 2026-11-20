"""Build known-content archives and verify the Windows executable against each one."""
import argparse
import bz2
import gzip
import io
import json
import lzma
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import zipfile


def run(binary, *args, expected=0, cwd=None):
    result = subprocess.run([binary, *map(str, args)], capture_output=True, cwd=cwd)
    if result.returncode != expected:
        raise AssertionError((args, result.returncode, result.stderr.decode(errors="replace")))
    return result.stdout


def verify(binary, archive, output, expected, *options):
    listed = json.loads(run(binary, "l", archive, "--json", *options))
    files = {Path(e["path"]).as_posix() for e in listed["entries"] if not e["directory"]}
    assert files == set(expected), (archive, files, expected.keys())
    report = json.loads(run(binary, "t", archive, "--json", *options))
    assert report == {"ok": True, "files": len(files), "bytes": sum(map(len, expected.values()))}
    run(binary, "x", archive, "--here", "-o", output, *options)
    written = {p.relative_to(output).as_posix(): p.read_bytes() for p in output.rglob("*") if p.is_file()}
    assert written == expected, archive


def payloads(i):
    return {"docs/note.txt": f"Working archive {i}\n".encode(), "nested/data.bin": bytes(range(256)) * i, "empty.txt": b""}


def generate(folder):
    for i in range(6):
        data = payloads(i)
        for suffix, method in [("store", zipfile.ZIP_STORED), ("deflate", zipfile.ZIP_DEFLATED),
                               ("bz2", zipfile.ZIP_BZIP2), ("lzma", zipfile.ZIP_LZMA)]:
            arc = folder / f"zip-{i}-{suffix}.zip"
            with zipfile.ZipFile(arc, "w", method) as z:
                for name, content in data.items():
                    z.writestr(name, content)
            yield arc, data
        for suffix, mode in [("tar", "w"), ("tar.gz", "w:gz"), ("tar.bz2", "w:bz2"), ("tar.xz", "w:xz")]:
            arc = folder / f"case-{i}.{suffix}"
            with tarfile.open(arc, mode) as tar:
                for name, content in data.items():
                    info = tarfile.TarInfo(name)
                    info.size = len(content)
                    info.mtime = 1_700_000_000
                    tar.addfile(info, io.BytesIO(content))
            yield arc, data
        for suffix, compress in [("gz", gzip.compress), ("xz", lzma.compress), ("bz2", bz2.compress)]:
            arc = folder / f"stream-{i}.{suffix}"
            content = data["nested/data.bin"]
            arc.write_bytes(compress(content))
            yield arc, {arc.stem: content}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--sevenzip", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    count = 0
    with tempfile.TemporaryDirectory(prefix="tear-corpus-") as tmp:
        base = Path(tmp)
        for arc, files in generate(base):
            verify(binary, arc, base / f"out-{count}", files)
            count += 1
        arc = base / "single-root.zip"
        with zipfile.ZipFile(arc, "w") as z:
            z.writestr("manual/readme.txt", "Windows smoke test\n")
        zone = b"[ZoneTransfer]\r\nZoneId=3\r\n"
        Path(str(arc) + ":Zone.Identifier").write_bytes(zone)
        for dirname in ["manual", "manual (2)"]:
            run(binary, "x", arc, "-o", base / "smart", "--smart")
            note = base / "smart" / dirname / "readme.txt"
            assert note.read_bytes() == b"Windows smoke test\n"
            assert Path(str(note) + ":Zone.Identifier").read_bytes() == zone
        # Existing user data must survive a collision when extracting directly.
        note = base / "smart/manual/readme.txt"
        note.write_text("user data")
        run(binary, "x", arc, "-o", base / "smart", "--here", expected=2)
        assert note.read_text() == "user data"
        count += 1
        if args.sevenzip:
            seven = args.sevenzip.resolve()
            data = {"hello.txt": b"native archiver verification\n"}
            (base / "hello.txt").write_bytes(data["hello.txt"])
            for ext, switches in [("zip", ["-mem=AES256"]), ("7z", ["-mhe=on"])]:
                arc = base / f"encrypted.{ext}"
                run(seven, "a", *switches, "-pExampleOnly", arc, "hello.txt", cwd=base)
                verify(binary, arc, base / ("decrypted-"+ext), data, "-p", "ExampleOnly")
                run(binary, "t", arc, "-p", "wrong", expected=3)
                # A missing or wrong password must not leave an empty folder behind.
                for extra in ([], ["-p", "wrong"]):
                    dest = base / f"no-leftovers-{ext}-{len(extra)}"
                    run(binary, "x", arc, "-o", dest, *extra, expected=3)
                    assert not dest.exists() or not any(dest.iterdir()), (arc, extra, list(dest.iterdir()))
                count += 1
            # Windows' bsdtar, not another tar that happens to be on PATH.
            bsdtar = Path(os.environ["SystemRoot"]) / "System32" / "tar.exe"
            arc = base / "hello.txt.zst"
            run(bsdtar, "--zstd", "--format=raw", "-cf", arc, "hello.txt", cwd=base)
            verify(binary, arc, base / "zstd-out", data)
            count += 1
            arc = base / "bundle.tar.zst"
            run(bsdtar, "--zstd", "-cf", arc, "hello.txt", cwd=base)
            verify(binary, arc, base / "tar-zstd-out", data)
            count += 1
        arc = base / "damaged.zip"
        with zipfile.ZipFile(arc, "w", zipfile.ZIP_STORED) as z:
            z.writestr("first.txt", b"unique-content-marker")
            z.writestr("second.txt", b"healthy")
        arc.write_bytes(arc.read_bytes().replace(b"unique-content-marker", b"unique-content-MARKER", 1))
        dest = base / "recovered"
        run(binary, "x", arc, "--here", "-o", dest, expected=1)
        assert not (dest/"first.txt").exists() and (dest/"second.txt").read_bytes()==b"healthy"
        count += 1
    print(f"{count} archive fixtures passed")


if __name__ == "__main__":
    main()
