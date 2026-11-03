#!/usr/bin/env python3
"""Build and test stage1/stage2 once the self-hosted compiler source exists."""

from pathlib import Path
import argparse
import hashlib
import struct
import subprocess


ROOT = Path(__file__).resolve().parent.parent
SELF = ROOT / "mimi-self"
RUNTIME = SELF / "runtime.c"
TEST_RUNNER = SELF / "test.py"


def run(command: list[str], *, cwd: Path = ROOT) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def artifact_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


LC_UUID = 0x1B
LC_CODE_SIGNATURE = 0x1D


def macho_metadata_ranges(path: Path) -> list[tuple[int, int]]:
    """Byte ranges the Mach-O linker fills with output-dependent metadata."""
    data = path.read_bytes()
    if len(data) < 32 or struct.unpack_from("<I", data, 0)[0] != 0xFEEDFACF:
        return []
    command_count = struct.unpack_from("<8I", data, 0)[4]
    offset = 32
    ranges: list[tuple[int, int]] = []
    for _ in range(command_count):
        if offset + 8 > len(data):
            break
        command, size = struct.unpack_from("<II", data, offset)
        if command == LC_UUID:
            ranges.append((offset + 8, offset + 24))
        elif command == LC_CODE_SIGNATURE:
            data_offset, data_size = struct.unpack_from("<II", data, offset + 8)
            ranges.append((data_offset, data_offset + data_size))
        offset += size
    return ranges


def normalized_artifact(path: Path) -> bytes:
    data = bytearray(path.read_bytes())
    for start, end in macho_metadata_ranges(path):
        end = min(end, len(data))
        data[start:end] = bytes(end - start)
    return bytes(data)


def artifact_record(path: Path) -> str:
    digest = artifact_digest(path)
    return f"{path}: {path.stat().st_size} bytes, sha256 {digest}"


def build(compiler: Path, source: Path, output: Path) -> None:
    run(
        [
            str(compiler), "build", str(source), "-o", str(output),
            "--c-source", str(RUNTIME), "--no-cache",
        ]
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler_source", type=Path)
    parser.add_argument("--stage0", type=Path, default=ROOT / "target/debug/mimi")
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments = parser.parse_args()
    stage0 = arguments.stage0.resolve()
    source = arguments.compiler_source.resolve()
    output_dir = arguments.output_dir.resolve()
    if not stage0.is_file():
        parser.error(f"stage0 compiler does not exist: {stage0}")
    if not source.is_file():
        parser.error(f"compiler source does not exist: {source}")
    output_dir.mkdir(parents=True, exist_ok=True)

    stage1 = output_dir / "mimi-stage1"
    stage2 = output_dir / "mimi-stage2"
    stage3 = output_dir / "mimi-stage3"
    build(stage0, source, stage1)
    print("Stage1 artifact:", artifact_record(stage1), flush=True)
    run(["python3", str(TEST_RUNNER), "--compiler", str(stage0)])
    try:
        build(stage1, source, stage2)
    except subprocess.CalledProcessError as failure:
        print(f"Stage2 build stopped with exit status {failure.returncode}; see compiler diagnostic above.", flush=True)
        raise SystemExit(failure.returncode) from None
    build(stage2, source, stage3)
    run(["python3", str(TEST_RUNNER), "--compiler", str(stage1), "--reference-compiler", str(stage0), "--self-hosted"])
    run(["python3", str(TEST_RUNNER), "--compiler", str(stage2), "--reference-compiler", str(stage0), "--self-hosted"])
    run(["python3", str(TEST_RUNNER), "--compiler", str(stage3), "--reference-compiler", str(stage0), "--self-hosted"])
    print("Stage artifacts:")
    print(artifact_record(stage1))
    print(artifact_record(stage2))
    print(artifact_record(stage3))
    same_path = output_dir / "mimi-repro"
    build(stage1, source, same_path)
    first_digest = artifact_digest(same_path)
    build(stage2, source, same_path)
    if first_digest == artifact_digest(same_path):
        print("Stage1 and stage2 produce byte-identical output at the same path.")
    else:
        print("Stage1 and stage2 output differs at the same path; review reproducibility.")

    ignored = len(macho_metadata_ranges(stage2)) + len(macho_metadata_ranges(stage3))
    if normalized_artifact(stage2) == normalized_artifact(stage3):
        print(
            "Stage2 and stage3 output is identical apart from the Mach-O load-command "
            f"UUID and ad-hoc signature ({ignored} metadata regions)."
        )
    else:
        print("Stage2 and stage3 output differs beyond linker metadata; review reproducibility.")


if __name__ == "__main__":
    main()
