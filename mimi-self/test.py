#!/usr/bin/env python3
"""Compile and run the Mimi self-hosting test programs with stage0."""

from pathlib import Path
import argparse
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
SELF = ROOT / "mimi-self"
RUNTIME = SELF / "runtime.c"


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
                [
                    str(compiler), "build", str(input_path), "-o", str(output_path),
                    "--c-source", str(RUNTIME),
                ],
                expected_status=1,
            )
            if expected_diagnostic not in result.stderr:
                raise AssertionError(
                    f"{name}: expected {expected_diagnostic!r} in stderr, "
                    f"got:\n{result.stderr}"
                )


def test_compiler_parity(reference: Path, candidate: Path) -> None:
    cases = [
        ("accepted", "main :: fn () i32 { 0 }", 0, None),
        ("parse-error", "main :: fn () i32 { 1 + }", 1, "error[parse]"),
        ("lex-error", "main :: fn () i32 { 1 | 2 }", 1, "error[lex]"),
        (
            "type-error",
            "main :: fn () i32 { missing_name }",
            1,
            "error[type]",
        ),
    ]
    with tempfile.TemporaryDirectory(prefix="mimi-self-parity-") as temporary:
        directory = Path(temporary)
        for name, source, expected_status, expected_diagnostic in cases:
            source_path = directory / f"{name}.mi"
            source_path.write_text(source, encoding="utf-8")
            observations: list[tuple[bool, int | None, str | None]] = []
            for label, compiler in (("stage0", reference), ("candidate", candidate)):
                output_path = directory / f"{name}-{label}"
                result = subprocess.run(
                    [
                        str(compiler), "build", str(source_path), "-o", str(output_path),
                        "--c-source", str(RUNTIME),
                    ],
                    cwd=ROOT,
                    text=True,
                    capture_output=True,
                )
                accepted = result.returncode == 0
                if accepted != (expected_status == 0):
                    raise AssertionError(
                        f"{label} {name}: expected compile status {expected_status}, "
                        f"got {result.returncode}:\n{result.stderr}"
                    )
                program_status = None
                diagnostic = None
                if accepted:
                    program = run([str(output_path)])
                    program_status = program.returncode
                else:
                    diagnostic = next(
                        (line for line in result.stderr.splitlines() if line.startswith("error[")),
                        None,
                    )
                    if expected_diagnostic not in result.stderr:
                        raise AssertionError(
                            f"{label} {name}: expected diagnostic {expected_diagnostic!r}, "
                            f"got:\n{result.stderr}"
                        )
                observations.append((accepted, program_status, diagnostic))
            if observations[0] != observations[1]:
                raise AssertionError(
                    f"compiler behavior differs for {name}: "
                    f"stage0={observations[0]!r}, candidate={observations[1]!r}"
                )


def mimi_quote(value: Path) -> str:
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"') + '"'


def test_file_io(compiler: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="mimi-self-files-") as temporary:
        directory = Path(temporary)
        source_path = directory / "file-io.mi"
        input_path = directory / "input.bin"
        output_path = directory / "output.bin"
        executable = directory / "file-io"
        expected = b"Mimi\x00self-hosting\xff"
        input_path.write_bytes(expected)
        module_path = SELF / "src" / "bytes.mi"
        source_path.write_text(
            f"""bytes :: @import({mimi_quote(module_path)})
main :: fn () i32 {{
    buffer := bytes.init()
    if !bytes.read_file({mimi_quote(input_path)}, &buffer) {{ return 1 }}
    if buffer.len != {len(expected)} || buffer.data[0] != 77 || buffer.data[4] != 0 || buffer.data[17] != 255 {{ return 2 }}
    if !bytes.write_file({mimi_quote(output_path)}, &buffer) {{ return 3 }}
    bytes.release(&buffer)
    0
}}
""",
            encoding="utf-8",
        )
        run(
            [
                str(compiler), "build", str(source_path), "-o", str(executable),
                "--c-source", str(RUNTIME),
            ]
        )
        run([str(executable)])
        if output_path.read_bytes() != expected:
            raise AssertionError("Mimi file write did not preserve the input bytes")


def test_path_api(compiler: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="mimi-self-paths-") as temporary:
        directory = Path(temporary) / "created directory"
        file_path = directory / "data file.bin"
        missing_path = directory / "missing"
        directory_text = mimi_quote(directory)
        file_text = mimi_quote(file_path)
        missing_text = mimi_quote(missing_path)
        nul_path = ", ".join(str(byte) for byte in directory.as_posix().encode() + b"\x00ignored")
        source_path = Path(temporary) / "paths.mi"
        executable = Path(temporary) / "paths"
        module_path = SELF / "src" / "path.mi"
        source_path.write_text(
            f'''path :: @import({mimi_quote(module_path)})
bytes :: @import({mimi_quote(SELF / "src" / "bytes.mi")})
main :: fn () i32 {{
    if path.exists({directory_text}) {{ return 1 }}
    if !path.create_directory({directory_text}) {{ return 2 }}
    if !path.exists({directory_text}) || !path.is_directory({directory_text}) {{ return 3 }}
    if path.is_directory({missing_text}) {{ return 4 }}
    buffer := bytes.init()
    if !bytes.push(&buffer, 65) || !bytes.push(&buffer, 0) || !bytes.push(&buffer, 255) {{ return 5 }}
    if !bytes.write_file({file_text}, &buffer) {{ return 6 }}
    bytes.release(&buffer)
    if !path.exists({file_text}) || path.is_directory({file_text}) {{ return 7 }}
    size : u64 = 0
    if !path.file_size({file_text}, &size) || size != 3 {{ return 8 }}
    if path.file_size({missing_text}, &size) {{ return 9 }}
    if path.exists([{nul_path}]) {{ return 10 }}
    0
}}
''',
            encoding="utf-8",
        )
        run(
            [
                str(compiler), "build", str(source_path), "-o", str(executable),
                "--c-source", str(RUNTIME),
            ]
        )
        run([str(executable)])


def test_source_model(compiler: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="mimi-self-source-") as temporary:
        directory = Path(temporary)
        input_path = directory / "source input.mi"
        source_path = directory / "source-model.mi"
        executable = directory / "source-model"
        input_path.write_bytes(b"alpha\n\x00x\n")
        module_path = mimi_quote(SELF / "src" / "source.mi")
        source_path.write_text(
            f'''source :: @import({module_path})
main :: fn () i32 {{
    loaded := source.init()
    if !source.load({mimi_quote(input_path)}, &loaded) {{ return 1 }}
    if loaded.contents.len != 9 {{ return 2 }}
    if loaded.contents.data[0] != 97 || loaded.contents.data[8] != 10 {{ return 3 }}
    span := source.Span {{ start = 2, end = 5 }}
    if span.start != 2 || span.end != 5 {{ return 4 }}
    diagnostic := source.Diagnostic {{ code = "E_TEST", message = "sample", span = span }}
    if diagnostic.span.end - diagnostic.span.start != 3 {{ return 5 }}
    source.render(diagnostic, &loaded)
    source.release(&loaded)
    graph := source.module_graph_init()
    if !source.module_begin(&graph, "root.mi") {{ return 6 }}
    if source.module_begin(&graph, "root.mi") {{ return 7 }}
    state : u64 = 0
    if !source.module_state(&graph, "root.mi", &state) || state != 1 {{ return 8 }}
    if !source.module_finish(&graph, "root.mi") {{ return 9 }}
    if !source.module_state(&graph, "root.mi", &state) || state != 2 {{ return 10 }}
    if source.module_finish(&graph, "root.mi") {{ return 11 }}
    source.module_graph_release(&graph)
    0
}}
''',
            encoding="utf-8",
        )
        run(
            [
                str(compiler), "build", str(source_path), "-o", str(executable),
                "--c-source", str(RUNTIME),
            ]
        )
        result = run([str(executable)])
        expected_diagnostic = [
            "error[E_TEST]: sample",
            f" --> {input_path}:1:3",
            "1 | alpha",
            "  |   ^~~",
        ]
        for fragment in expected_diagnostic:
            if fragment not in result.stderr:
                raise AssertionError(
                    f"diagnostic output missing {fragment!r}:\n{result.stderr}"
                )


def test_module_loading(compiler: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="mimi-self-modules-") as temporary:
        directory = Path(temporary)
        program = directory / "module-load.mi"
        executable = directory / "module-load"
        module_path = mimi_quote(SELF / "src" / "source.mi")
        program.write_text(
            f'''source :: @import({module_path})
main :: fn () i32 {{
    graph := source.module_graph_init()
    if !source.module_begin(&graph, "root.mi") {{ return 1 }}
    if !source.module_begin(&graph, "child.mi") {{ return 2 }}
    if source.module_begin(&graph, "root.mi") {{ return 3 }}
    if !source.module_finish(&graph, "child.mi") {{ return 4 }}
    if !source.module_finish(&graph, "root.mi") {{ return 5 }}
    source.module_graph_release(&graph)
    0
}}
''',
            encoding="utf-8",
        )
        run(
            [
                str(compiler), "build", str(program), "-o", str(executable),
                "--c-source", str(RUNTIME),
            ]
        )
        run([str(executable)])


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--compiler",
        type=Path,
        help="compiler executable to test (defaults to the Rust stage0 compiler)",
    )
    parser.add_argument(
        "--reference-compiler",
        type=Path,
        help="Rust stage0 compiler used for parity checks with --compiler",
    )
    arguments = parser.parse_args()
    if arguments.compiler is None:
        run(["cargo", "build", "--quiet"])
        compiler = ROOT / "target" / "debug" / "mimi"
    else:
        compiler = arguments.compiler.resolve()
        if not compiler.is_file():
            parser.error(f"compiler does not exist: {compiler}")
    fixtures = sorted((SELF / "tests").glob("*.mi"))
    with tempfile.TemporaryDirectory(prefix="mimi-self-") as temporary:
        directory = Path(temporary)
        for fixture in fixtures:
            executable = directory / fixture.stem
            run(
                [
                    str(compiler), "build", str(fixture), "-o", str(executable),
                    "--c-source", str(RUNTIME),
                ]
            )
            run([str(executable)])
    test_compile_diagnostics(compiler)
    test_file_io(compiler)
    test_path_api(compiler)
    test_source_model(compiler)
    test_module_loading(compiler)
    if arguments.compiler is not None:
        reference = arguments.reference_compiler
        if reference is None:
            reference = ROOT / "target" / "debug" / "mimi"
        else:
            reference = reference.resolve()
        if not reference.is_file():
            parser.error(f"reference compiler does not exist: {reference}")
        test_compiler_parity(reference, compiler)
    print(f"mimi-self: tests passed with {compiler}")


if __name__ == "__main__":
    main()
