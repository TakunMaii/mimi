#!/usr/bin/env python3
"""Compile and run the Mimi self-hosting test programs with stage0."""

from pathlib import Path
import argparse
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
SELF = ROOT / "mimi-self"


def run(
    command: list[str], *, cwd: Path = ROOT, expected_status: int = 0
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True)
    if result.returncode != expected_status:
        raise RuntimeError(
            f"unexpected status {result.returncode}, expected {expected_status}: "
            f"{' '.join(command)}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def test_compile_diagnostics(compiler: Path) -> None:
    cases = [
        (
            "parse-error",
            "main :: fn () i32 { 1 + }",
            "error[parse]",
        ),
        (
            "type-error",
            "main :: fn () i32 { missing_name }",
            "undefined identifier 'missing_name'",
        ),
    ]
    with tempfile.TemporaryDirectory(prefix="mimi-self-errors-") as temporary:
        directory = Path(temporary)
        for name, source, expected_diagnostic in cases:
            input_path = directory / f"{name}.mi"
            output_path = directory / name
            input_path.write_text(source, encoding="utf-8")
            result = run(
                [str(compiler), "build", str(input_path), "-o", str(output_path)],
                expected_status=1,
            )
            if expected_diagnostic not in result.stderr:
                raise AssertionError(
                    f"{name}: expected {expected_diagnostic!r} in stderr, "
                    f"got:\n{result.stderr}"
                )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--compiler",
        type=Path,
        help="compiler executable to test (defaults to the Rust stage0 compiler)",
    )
    arguments = parser.parse_args()
    if arguments.compiler is None:
        run(["cargo", "build", "--quiet"])
        compiler = ROOT / "target" / "debug" / "mimi"
    else:
        compiler = arguments.compiler.resolve()
        if not compiler.is_file():
            parser.error(f"compiler does not exist: {compiler}")
    fixture = SELF / "tests" / "lexer.mi"
    with tempfile.TemporaryDirectory(prefix="mimi-self-") as temporary:
        executable = Path(temporary) / "lexer-tests"
        run([str(compiler), "build", str(fixture), "-o", str(executable)])
        run([str(executable)])
    test_compile_diagnostics(compiler)
    print(f"mimi-self: tests passed with {compiler}")


if __name__ == "__main__":
    main()
