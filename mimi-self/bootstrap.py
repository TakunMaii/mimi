#!/usr/bin/env python3
"""Build and test stage1/stage2 once the self-hosted compiler source exists."""

from pathlib import Path
import argparse
import hashlib
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


if __name__ == "__main__":
    main()
