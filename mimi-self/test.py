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


def test_compile_diagnostics(compiler: Path, *, self_hosted: bool = False) -> None:
    cases = [
        (
            "parse-error",
            "main :: fn () i32 { 1 + }",
            "error[parse]",
            ("1:25", "^"),
        ),
        (
            "lex-error",
            "main :: fn () i32 { 1 | 2 }",
            "error[lex]",
            ("1:23", "^"),
        ),
        (
            "type-error",
            "main :: fn () i32 { missing_name }",
            "undefined identifier 'missing_name'",
            ("1:1", "^~~~"),
        ),
    ]
    if self_hosted:
        cases = [
            ("parse-error", "main :: fn () i32 { 1 + }", "error[E_PARSE]: expected an expression after operator", ("1:25", "^")),
            ("lex-error", "main :: fn () i32 { 1 | 2 }", "error[E_LEX]", ("1:23", "^")),
            ("type-error", "main :: fn () i32 { missing_name }", "error[E_NAME]: undefined identifier", ("1:21", "^~~~~~~~~~~~")),
        ]
    with tempfile.TemporaryDirectory(prefix="mimi-self-errors-") as temporary:
        directory = Path(temporary)
        for name, source, expected_diagnostic, expected_location in cases:
            input_path = directory / f"{name}.mi"
            output_path = directory / name
            input_path.write_text(source, encoding="utf-8")
            result = run(
                [
                    str(compiler), "build", str(input_path), "-o", str(output_path),
                    "--c-source", str(RUNTIME), "--no-cache",
                ],
                expected_status=1,
            )
            if expected_diagnostic not in result.stderr:
                raise AssertionError(
                    f"{name}: expected {expected_diagnostic!r} in stderr, "
                    f"got:\n{result.stderr}"
                )
            if "  = help:" not in result.stderr:
                raise AssertionError(f"{name}: missing actionable help:\n{result.stderr}")
            actual_location = diagnostic_location(result.stderr)
            if actual_location != expected_location:
                raise AssertionError(
                    f"{name}: expected location {expected_location!r}, "
                    f"got {actual_location!r}:\n{result.stderr}"
                )


def diagnostic_location(stderr: str) -> tuple[str, str] | None:
    lines = stderr.splitlines()
    location = next((line.strip() for line in lines if line.startswith(" --> ")), None)
    underline_index = next(
        (index for index, line in enumerate(lines) if line.startswith("  | ")),
        None,
    )
    if location is None or underline_index is None:
        return None
    location_suffix = location.rsplit(":", 2)
    if len(location_suffix) != 3:
        return None
    underline = lines[underline_index].split("|", 1)[1].strip()
    return f"{location_suffix[-2]}:{location_suffix[-1]}", underline


def diagnostic_category(stderr: str) -> str | None:
    if "E_PARSE" in stderr or "error[parse]" in stderr:
        return "parse"
    if "E_LEX" in stderr or "error[lex]" in stderr:
        return "lex"
    if "E_NAME" in stderr or "undefined identifier" in stderr:
        return "name"
    if "E_TYPE" in stderr or "error[type]" in stderr:
        return "type"
    return None


def test_compiler_parity(reference: Path, candidate: Path, *, self_hosted: bool = False) -> None:
    cases = [
        ("accepted-zero", "main :: fn () i32 { 0 }", 0, None),
        ("accepted-arithmetic", "main :: fn () i32 { value := 6 * 7 if value != 42 { return 1 } 0 }", 0, None),
        ("accepted-control", "main :: fn () i32 { value := 3 if value == 3 { return 0 } return 1 }", 0, None),
        ("accepted-cast", "main :: fn () i32 { value := 41 as i32 value + 1 - 42 }", 0, None),
        ("accepted-tuple-destructure", "main :: fn () i32 { first, second := (20, 22) first + second - 42 }", 0, None),
        ("accepted-struct-method", "point :: struct { x : i32, get() :: fn (self : Self) i32 { self.x } } main :: fn () i32 { value := point { x = 42 } value.get() - 42 }", 0, None),
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
                        "--c-source", str(RUNTIME), "--no-cache",
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
                location = None
                if accepted:
                    program = run([str(output_path)])
                    program_status = program.returncode
                else:
                    diagnostic = next(
                        (line for line in result.stderr.splitlines() if line.startswith("error[")),
                        None,
                    )
                    location = diagnostic_location(result.stderr)
                    if location is None:
                        raise AssertionError(
                            f"{label} {name}: missing primary diagnostic location:\n{result.stderr}"
                        )
                    if label == "stage0" and expected_diagnostic not in result.stderr:
                        raise AssertionError(
                            f"{label} {name}: expected diagnostic {expected_diagnostic!r}, "
                            f"got:\n{result.stderr}"
                        )
                    if label == "candidate" and self_hosted:
                        expected_candidate_location = {
                            "parse-error": ("1:25", "^"),
                            "lex-error": ("1:23", "^"),
                            "type-error": ("1:21", "^~~~~~~~~~~~"),
                        }.get(name)
                        if expected_candidate_location is not None and location != expected_candidate_location:
                            raise AssertionError(
                                f"candidate {name}: expected location {expected_candidate_location!r}, "
                                f"got {location!r}:\n{result.stderr}"
                            )
                observations.append((accepted, program_status, diagnostic_category(result.stderr) if not accepted else None))
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
                "--c-source", str(RUNTIME), "--no-cache",
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
    canonical := bytes.init()
    if path.canonicalize([{nul_path}], &canonical) {{ return 16 }}
    bytes.release(&canonical)
    joined := bytes.init()
    if !path.join("folder", "file", &joined) {{ return 11 }}
    if joined.len != 11 || joined.data[6] != 47 || joined.data[10] != 101 {{ return 12 }}
    bytes.release(&joined)
    joined_slash := bytes.init()
    if !path.join("folder/", "file", &joined_slash) {{ return 13 }}
    if joined_slash.len != 11 || joined_slash.data[6] != 47 || joined_slash.data[7] != 102 {{ return 14 }}
    bytes.release(&joined_slash)
    joined_empty := bytes.init()
    if !path.join("folder", "", &joined_empty) || joined_empty.len != 7 || joined_empty.data[6] != 47 {{ return 15 }}
    bytes.release(&joined_empty)
    0
}}
''',
            encoding="utf-8",
        )
        run(
            [
                str(compiler), "build", str(source_path), "-o", str(executable),
                "--c-source", str(RUNTIME), "--no-cache",
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
        semantic_path = directory / "semantic input.mi"
        semantic_path.write_text("main :: fn () i32 { missing }", encoding="utf-8")
        module_path = mimi_quote(SELF / "src" / "source.mi")
        source_path.write_text(
            f'''source :: @import({module_path})
extern mimi_slice_from_raw : fn(data : *u8, length : u64, output : *[]u8) void
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
    semantic_source := source.init()
    if !source.load({mimi_quote(semantic_path)}, &semantic_source) {{ return 12 }}
    semantic_issue := source.analyze_text("main :: fn () i32 {{ missing }}")
    if semantic_issue.code.len != 6 || semantic_issue.span.start != 20 || semantic_issue.span.end != 27 {{ return 13 }}
    source.render(semantic_issue, &semantic_source)
    source.release(&semantic_source)
    lexical_issue := source.analyze_text("main :: fn () i32 {{ 1 | 2 }}")
    if lexical_issue.code.len != 5 || lexical_issue.span.start != 22 {{ return 14 }}
    valid := source.analyze_text("main :: fn () i32 {{ 0 }}")
    if valid.code.len != 0 {{ return 15 }}
    shadowed := source.analyze_text("main :: fn (value : i32) i32 {{ value := 1 if true {{ value := 2 return value }} return value }}")
    if shadowed.code.len != 0 {{ return 16 }}
    duplicate := source.analyze_text("main :: fn () i32 {{ value := 1 value := 2 return value }}")
    if duplicate.code.len == 0 {{ return 17 }}
    self_file := source.init()
    if !source.load({mimi_quote(SELF / "src" / "lexer.mi")}, &self_file) {{ return 18 }}
    self_text : []u8 = ""
    mimi_slice_from_raw(self_file.contents.data, self_file.contents.len, &self_text)
    self_issue := source.analyze_text(self_text)
    if self_issue.code.len != 0 {{ source.render(self_issue, &self_file) }}
    source.release(&self_file)
    if self_issue.code.len != 0 {{ return 19 }}
    if !source.load({mimi_quote(SELF / "src" / "compiler.mi")}, &self_file) {{ return 20 }}
    mimi_slice_from_raw(self_file.contents.data, self_file.contents.len, &self_text)
    self_issue = source.analyze_text(self_text)
    if self_issue.code.len != 0 {{ source.render(self_issue, &self_file) }}
    source.release(&self_file)
    if self_issue.code.len != 0 {{ return 21 }}
    if !source.load({mimi_quote(SELF / "src" / "source.mi")}, &self_file) {{ return 22 }}
    mimi_slice_from_raw(self_file.contents.data, self_file.contents.len, &self_text)
    self_issue = source.analyze_text(self_text)
    if self_issue.code.len != 0 {{ source.render(self_issue, &self_file) }}
    source.release(&self_file)
    if self_issue.code.len != 0 {{ return 23 }}
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
                "--c-source", str(RUNTIME), "--no-cache",
            ]
        )
        result = run([str(executable)])
        expected_diagnostic = [
            "error[E_TEST]: sample",
            f" --> {input_path}:1:3",
            "1 | alpha",
            "  |   ^~~",
            "error[E_NAME]: undefined identifier",
            f" --> {semantic_path}:1:21",
            "  |                     ^~~~~~~",
        ]
        for fragment in expected_diagnostic:
            if fragment not in result.stderr:
                raise AssertionError(
                    f"diagnostic output missing {fragment!r}:\n{result.stderr}"
                )


def test_scalar_codegen(compiler: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="mimi-self-codegen-") as temporary:
        directory = Path(temporary)
        driver = directory / "emit.mi"
        emitter = directory / "emit"
        generated = directory / "program.c"
        executable = directory / "program"
        program = "main :: fn () i32 { value := 6 * 7 check := value == 42 check result := value as i32 result - 42 }"
        driver.write_text(
            f'''ir :: @import({mimi_quote(SELF / "src" / "lexer.mi")})
main :: fn () i32 {{
    source := "{program}"
    storage := ir.init()
    lowered := ir.lower_text(source, &storage)
    if lowered.issue.code != 0 {{ return 1 }}
    if !ir.emit_c_to_file(source, lowered.module, {mimi_quote(generated)}) {{ return 2 }}
    ir.release(&storage)
    0
}}
''',
            encoding="utf-8",
        )
        run(
            [
                str(compiler), "build", str(driver), "-o", str(emitter),
                "--c-source", str(RUNTIME), "--no-cache",
            ]
        )
        run([str(emitter)])
        run(["cc", "-std=c11", "-O2", str(generated), "-o", str(executable)])
        run([str(executable)])


def test_partial_compiler_bootstrap(stage0: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="mimi-self-stage1-") as temporary:
        directory = Path(temporary)
        stage1 = directory / "mimi-stage1"
        run(
            [
                str(stage0), "build", str(SELF / "src" / "compiler.mi"),
                "-o", str(stage1), "--c-source", str(RUNTIME), "--no-cache",
            ]
        )
        cases = [
            ("arithmetic", "main :: fn () i32 { value := 6 * 7 value }", 42),
            ("cast", "main :: fn () i32 { value := 41 as i32 value + 1 }", 42),
            ("boolean", "main :: fn () i32 { check := 2 < 3 check 1 }", 1),
            ("call", "main :: fn () i32 { add(20, 22) } add :: fn (left : i32, right : i32) i32 { left + right }", 42),
            ("typed-float", "helper :: fn (value : f64) f64 { value + 1 } main :: fn () i32 { result : f64 = helper(41) result as i32 }", 42),
            ("while", "main :: fn () i32 { value := 0 while value < 5 { value = value + 1 } if value == 5 { return 42 } return 1 }", 42),
            ("do", "main :: fn () i32 { value := 0 do { value = value + 1 } while value < 3 value + 39 }", 42),
            ("loop", "main :: fn () i32 { value := 40 loop { value = value + 1 if value == 42 { break } } value }", 42),
            ("conditional", "main :: fn () i32 { if true { return 42 } else { return 0 } 0 }", 42),
            ("range-for", "main :: fn () i32 { total := 22 for index in 2..7 { total = total + index } total }", 42),
            ("range-continue", "main :: fn () i32 { total := 34 for index in 0..5 { if index == 2 { continue } total = total + index } total }", 42),
            ("if-expression", "main :: fn () i32 { result := if true { 42 } else { 0 } result }", 42),
            ("if-expression-else", "main :: fn () i32 { result := if false { 0 } else { 42 } result }", 42),
            ("scalar-extern", "extern labs : fn(value : i64) i64 main :: fn () i32 { labs(-42) as i32 }", 42),
            ("pointer-extern", "extern malloc : fn(size : u64) *u8 extern free : fn(pointer : *u8) void main :: fn () i32 { memory := malloc(8) if memory == null { return 1 } free(memory) 42 }", 42),
            ("pointer-cast", "extern malloc : fn(size : u64) *u8 extern free : fn(pointer : *u8) void main :: fn () i32 { memory := malloc(8) converted := memory as *i32 free(converted as *u8) 42 }", 42),
            ("byte-slice", "main :: fn () i32 { text : []u8 = \"hello\\n\" if text.len == 6 { return 42 } 0 }", 42),
            ("slice-extern", "extern mimi_stderr_write : fn(data : *u8, length : u64) void main :: fn () i32 { text : []u8 = \"hello\\n\" mimi_stderr_write(text.ptr, text.len) 42 }", 42),
            ("slice-index", "extern mimi_slice_from_raw : fn(data : *u8, length : u64, output : *[]u8) void main :: fn () i32 { text := \"xy\" copied : []u8 = \"\" mimi_slice_from_raw(text.ptr, text.len, &copied) if copied[1] == 121 { return 42 } 0 }", 42),
            ("struct-literal", "Point :: struct { x : i32, y : i32 } main :: fn () i32 { point := Point { x = 20, y = 22 } point.x + point.y }", 42),
            ("struct-pointer", "Point :: struct { x : i32 } read :: fn (point : *Point) i32 { point.x } main :: fn () i32 { point := Point { x = 42 } read(&point) }", 42),
            ("struct-assign", "Point :: struct { x : i32 } main :: fn () i32 { point := Point { x = 20 } point.x = 42 point.x }", 42),
            ("tuple-return", "get :: fn () (i32, i32) { (20, 22) } main :: fn () i32 { pair := get() pair.0 + pair.1 }", 42),
            ("tuple-argument", "sum :: fn (pair : (i32, i32)) i32 { pair.0 + pair.1 } main :: fn () i32 { sum((20, 22)) }", 42),
            ("slice-bounds", "main :: fn () i32 { text := \"abcde\" part := text[1:4] if part.len == 3 && part[0] == 98 { return 42 } 0 }", 42),
            ("slice-open-end", "main :: fn () i32 { text := \"abcde\" part := text[2:] if part.len == 3 && part[2] == 101 { return 42 } 0 }", 42),
            ("struct-pointer-index", "Point :: struct { x : i32 } main :: fn () i32 { point := Point { x = 20 } pointer := &point pointer[0] = Point { x = 42 } pointer[0].x }", 42),
            ("integer-pointer", "main :: fn () i32 { value : u64 = 20 pointer := &value pointer[0] = 42 value as i32 }", 42),
            ("byte-array-literal", "main :: fn () i32 { values := [20, 22] values[0] + values[1] }", 42),
        ]
        for name, text, expected_exit in cases:
            program = directory / f"{name} program.mi"
            program.write_text(text, encoding="utf-8")
            reference = directory / f"{name}-stage0"
            candidate = directory / f"{name}-stage1"
            run(
                [
                    str(stage0), "build", str(program), "-o", str(reference),
                    "--c-source", str(RUNTIME), "--no-cache",
                ]
            )
            run(
                [
                    str(stage1), "build", str(program), "-o", str(candidate),
                    "--c-source", str(RUNTIME), "--no-cache",
                ]
            )
            reference_run = run([str(reference)], expected_status=expected_exit)
            candidate_run = run([str(candidate)], expected_status=expected_exit)
            if (candidate_run.stdout, candidate_run.stderr) != (reference_run.stdout, reference_run.stderr):
                raise AssertionError(f"stage1 {name}: runtime output differs from stage0")
            if candidate.with_name(candidate.name + ".mimi.c").exists():
                raise AssertionError("stage1 left temporary C source after --no-cache")
        left = directory / "left.mi"
        right = directory / "right.mi"
        middle = directory / "middle.mi"
        left.write_text("pub value :: fn () i32 { 20 }\nhidden :: fn () i32 { 0 }\n", encoding="utf-8")
        right.write_text("pub value :: fn () i32 { 22 }\n", encoding="utf-8")
        middle.write_text('left :: @import("left.mi")\npub result :: fn () i32 { left.value() + 22 }\n', encoding="utf-8")
        point_module = directory / "point.mi"
        point_module.write_text("pub Point :: struct { x : i32 } pub make :: fn () Point { Point { x = 42 } }\n", encoding="utf-8")
        flag_module = directory / "flag.mi"
        flag_module.write_text("pub check :: fn () bool { true }\n", encoding="utf-8")
        module_cases = [
            ("same-name", 'left :: @import("left.mi")\nright :: @import("right.mi")\nmain :: fn () i32 { left.value() + right.value() }'),
            ("transitive", 'middle :: @import("middle.mi")\nmain :: fn () i32 { middle.result() }'),
            ("imported-struct", 'point :: @import("point.mi")\nmain :: fn () i32 { value : point.Point = point.make() value.x }'),
            ("prefix-imported-call", 'flag :: @import("flag.mi")\nmain :: fn () i32 { if !flag.check() { return 0 } 42 }'),
        ]
        for name, text in module_cases:
            program = directory / f"{name}.mi"
            program.write_text(text, encoding="utf-8")
            for label, tool in (("stage0", stage0), ("stage1", stage1)):
                executable = directory / f"{name}-{label}"
                run([str(tool), "build", str(program), "-o", str(executable), "--c-source", str(RUNTIME), "--no-cache"])
                run([str(executable)], expected_status=42)
        argc_program = directory / "argc-main.mi"
        argc_program.write_text("main :: fn (argc : i32, argv : **u8) i32 { argc }", encoding="utf-8")
        for label, tool in (("stage0", stage0), ("stage1", stage1)):
            executable = directory / f"argc-{label}"
            run([str(tool), "build", str(argc_program), "-o", str(executable), "--c-source", str(RUNTIME), "--no-cache"])
            run([str(executable), "alpha", "beta"], expected_status=3)
        argv_program = directory / "argv-index.mi"
        argv_program.write_text("main :: fn (argc : i32, argv : **u8) i32 { if argc == 3 && argv[1][0] == 97 { return 42 } 0 }", encoding="utf-8")
        for label, tool in (("stage0", stage0), ("stage1", stage1)):
            executable = directory / f"argv-{label}"
            run([str(tool), "build", str(argv_program), "-o", str(executable), "--c-source", str(RUNTIME), "--no-cache"])
            run([str(executable), "alpha", "beta"], expected_status=42)
        private_program = directory / "private-call.mi"
        private_program.write_text('left :: @import("left.mi")\nmain :: fn () i32 { left.hidden() }', encoding="utf-8")
        private_result = run([str(stage1), "build", str(private_program), "-o", str(directory / "private-call")], expected_status=1)
        if "error[E_LOWER]" not in private_result.stderr or diagnostic_location(private_result.stderr) is None:
            raise AssertionError(f"stage1 private import: bad diagnostic:\n{private_result.stderr}")
        rejected = [
            ("name", "main :: fn () i32 { missing }", "error[E_NAME]"),
            ("type", "main :: fn () i32 { true }", "error[E_TYPE]"),
            ("lower", "main :: fn () i32 { defer { return 1 } 0 }", "error[E_LOWER]"),
            ("arity", "add :: fn (left : i32, right : i32) i32 { left + right } main :: fn () i32 { add(20) }", "error[E_LOWER]"),
            ("range-readonly", "main :: fn () i32 { for index in 0..3 { index = 2 } 0 }", "error[E_LOWER]"),
            ("integer-pointer-cast", "main :: fn () i32 { pointer := 0 as *u8 0 }", "error[E_LOWER]"),
        ]
        for name, text, expected in rejected:
            program = directory / f"{name}.mi"
            program.write_text(text, encoding="utf-8")
            result = run(
                [str(stage1), "build", str(program), "-o", str(directory / name)],
                expected_status=1,
            )
            if expected not in result.stderr or diagnostic_location(result.stderr) is None:
                raise AssertionError(f"stage1 {name}: bad diagnostic:\n{result.stderr}")
        stage2 = directory / "stage2"
        run(
            [str(stage1), "build", str(SELF / "src" / "compiler.mi"), "-o", str(stage2),
             "--c-source", str(RUNTIME), "--no-cache"],
        )
        stage2_program = directory / "stage2-tuple.mi"
        stage2_program.write_text(
            "main :: fn () i32 { pair := (20, 22) pair.0 + pair.1 }", encoding="utf-8",
        )
        stage2_executable = directory / "stage2-tuple"
        run(
            [str(stage2), "build", str(stage2_program), "-o", str(stage2_executable),
             "--no-cache"],
        )
        run([str(stage2_executable)], expected_status=42)


def test_module_loading(compiler: Path, *, self_hosted: bool = False) -> None:
    with tempfile.TemporaryDirectory(prefix="mimi-self-modules-") as temporary:
        directory = Path(temporary)
        program = directory / "module-load.mi"
        executable = directory / "module-load"
        module_path = mimi_quote(SELF / "src" / "source.mi")
        program.write_text(
            f'''source :: @import({module_path})
main :: fn () i32 {{
    if source.module_directory_length("/main.mi".ptr, 8) != 1 {{ return 3 }}
    if source.module_directory_length("/tmp/main.mi".ptr, 12) != 4 {{ return 4 }}
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
                "--c-source", str(RUNTIME), "--no-cache",
            ]
        )
        run([str(executable)])

        root_module = directory / "graph root.mi"
        left_module = directory / "left.mi"
        right_module = directory / "right.mi"
        shared_module = directory / "shared.mi"
        cycle_a = directory / "cycle-a.mi"
        cycle_b = directory / "cycle-b.mi"
        orphan_module = directory / "orphan.mi"
        (directory / "sub").mkdir()
        root_module.write_text(
            'left :: @import("left.mi")\nright :: @import("right.mi")\n'
            'shared :: @import("./shared.mi")\n', encoding="utf-8"
        )
        left_module.write_text('shared :: @import("shared.mi")\n', encoding="utf-8")
        right_module.write_text('shared :: @import("sub/../shared.mi")\n', encoding="utf-8")
        shared_module.write_text(
            "pub answer :: fn () i32 { 42 }\n"
            "pub Point :: struct { x : i32 }\n"
            "pub Choice :: enum { some, none }\n"
            "private :: fn () i32 { 0 }\n"
            "Hidden :: struct { x : i32 }\n",
            encoding="utf-8",
        )
        cycle_a.write_text('next :: @import("cycle-b.mi")\n', encoding="utf-8")
        cycle_b.write_text('next :: @import("./cycle-a.mi")\n', encoding="utf-8")
        orphan_module.write_text('@import("shared.mi")\n', encoding="utf-8")
        graph_program = directory / "graph traversal.mi"
        graph_executable = directory / "graph-traversal"
        graph_program.write_text(
            f'''source :: @import({module_path})
main :: fn () i32 {{
    graph := source.module_graph_init()
    if !source.load_graph({mimi_quote(root_module)}, &graph) {{ return 1 }}
    if graph.file_count != 4 || graph.files.contents_length == 0 || graph.files.contents[0] != 108 {{ return 5 }}
    if graph.files.import_count != 3 || graph.files.imports.alias_length != 4 || graph.files.imports.alias[0] != 108 {{ return 9 }}
    if graph.files.imports.path_length == 0 || graph.files.imports.next.alias_length != 5 || graph.files.imports.next.alias[0] != 114 {{ return 10 }}
    if graph.files.imports.next.next.alias_length != 6 || graph.files.imports.next.next.alias[0] != 115 {{ return 11 }}
    imported := source.module_file_lookup(&graph, graph.files.imports.path, graph.files.imports.path_length)
    if source.module_file_is_null(imported) || imported.import_count != 1 || imported.contents[0] != 115 {{ return 12 }}
    public_function := source.module_function_lookup(&graph, graph.files, "shared", "answer")
    if source.module_function_is_null(public_function) || public_function.file.contents[public_function.function.name_start] != 97 {{ return 14 }}
    transitive_function := source.module_function_lookup(&graph, imported, "shared", "answer")
    if source.module_function_is_null(transitive_function) || transitive_function.file != public_function.file {{ return 18 }}
    private_function := source.module_function_lookup(&graph, graph.files, "shared", "private")
    if !source.module_function_is_null(private_function) {{ return 15 }}
    public_type := source.module_type_lookup(&graph, graph.files, "shared", "Point")
    if source.module_symbol_is_null(public_type) || public_type.declaration.kind != 2 {{ return 22 }}
    enum_type := source.module_type_lookup(&graph, imported, "shared", "Choice")
    if source.module_symbol_is_null(enum_type) || enum_type.declaration.kind != 3 {{ return 23 }}
    private_type := source.module_type_lookup(&graph, graph.files, "shared", "Hidden")
    if !source.module_symbol_is_null(private_type) {{ return 24 }}
    wrong_kind := source.module_type_lookup(&graph, graph.files, "shared", "answer")
    if !source.module_symbol_is_null(wrong_kind) {{ return 25 }}
    record := graph.files
    index : u64 = 0
    seen_shared := false
    while index < graph.file_count {{
        if record.path_length == 0 || record.contents_length == 0 {{ return 6 }}
        if record.contents[0] == 112 {{ seen_shared = true }}
        record = record.next
        index = index + 1
    }}
    if !seen_shared {{ return 7 }}
    state : u64 = 0
    if !source.module_state(&graph, {mimi_quote(root_module.resolve())}, &state) || state != 2 {{ return 2 }}
    if !source.module_state(&graph, {mimi_quote(shared_module.resolve())}, &state) || state != 2 {{ return 3 }}
    source.module_graph_release(&graph)
    if graph.file_count != 0 {{ return 8 }}
    source.module_graph_release(&graph)
    cycle_graph := source.module_graph_init()
    if source.load_graph({mimi_quote(cycle_a)}, &cycle_graph) {{ return 4 }}
    source.module_graph_release(&cycle_graph)
    orphan_graph := source.module_graph_init()
    if source.load_graph({mimi_quote(orphan_module)}, &orphan_graph) {{ return 13 }}
    source.module_graph_release(&orphan_graph)
    self_graph := source.module_graph_init()
    if !source.load_graph({mimi_quote(SELF / "src" / "compiler.mi")}, &self_graph) {{ return 16 }}
    if self_graph.file_count < 8 {{ return 17 }}
    graph_type := source.module_type_lookup(&self_graph, self_graph.files, "source", "ModuleGraph")
    if source.module_symbol_is_null(graph_type) || graph_type.declaration.kind != 2 {{ return 26 }}
    parser_function := source.module_function_lookup(&self_graph, graph_type.file, "lexer", "parse_expression")
    if source.module_function_is_null(parser_function) {{ return 19 }}
    source.module_graph_release(&self_graph)
    0
}}
''', encoding="utf-8"
        )
        run(
            [
                str(compiler), "build", str(graph_program), "-o", str(graph_executable),
                "--c-source", str(RUNTIME), "--no-cache",
            ]
        )
        run([str(graph_executable)])

        if self_hosted:
            factory_module = directory / "result-factory.mi"
            factory_module.write_text(
                'pub Result :: enum <T, E> { ok(T), err(E) }\n'
                'pub make :: fn () Result<i32, i32> { ok(42) }\n',
                encoding="utf-8",
            )
            factory_program = directory / "result-factory-main.mi"
            factory_program.write_text(
                'factory :: @import("result-factory.mi")\n'
                'main :: fn () i32 { result := factory.make() value := match result { ok(item) => item, err(_) => 0 } value }\n',
                encoding="utf-8",
            )
            factory_executable = directory / "result-factory-main"
            run([str(compiler), "build", str(factory_program), "-o", str(factory_executable), "--c-source", str(RUNTIME), "--no-cache"])
            run([str(factory_executable)], expected_status=42)

        root_path_program = directory / "root-path.mi"
        root_path_executable = directory / "root-path"
        root_path_program.write_text(
            f'''path :: @import({mimi_quote(SELF / "src" / "path.mi")})
bytes :: @import({mimi_quote(SELF / "src" / "bytes.mi")})
main :: fn () i32 {{
    output := bytes.init()
    if !path.join_raw("/".ptr, 1, "child.mi".ptr, 8, &output) {{ return 1 }}
    if output.len != 9 || output.data[0] != 47 || output.data[8] != 105 {{ return 2 }}
    bytes.release(&output)
    0
}}
''', encoding="utf-8"
        )
        run(
            [
                str(compiler), "build", str(root_path_program), "-o", str(root_path_executable),
                "--c-source", str(RUNTIME), "--no-cache",
            ]
        )
        run([str(root_path_executable)])


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
    parser.add_argument("--self-hosted", action="store_true", help="expect self-hosted diagnostic codes and spans")
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
                    "--c-source", str(RUNTIME), "--no-cache",
                ]
            )
            run([str(executable)])
    test_compile_diagnostics(compiler, self_hosted=arguments.self_hosted)
    test_file_io(compiler)
    test_path_api(compiler)
    test_source_model(compiler)
    test_scalar_codegen(compiler)
    if arguments.compiler is None:
        test_partial_compiler_bootstrap(compiler)
    test_module_loading(compiler, self_hosted=arguments.self_hosted)
    if arguments.compiler is not None:
        reference = arguments.reference_compiler
        if reference is None:
            reference = ROOT / "target" / "debug" / "mimi"
        else:
            reference = reference.resolve()
        if not reference.is_file():
            parser.error(f"reference compiler does not exist: {reference}")
        test_compiler_parity(reference, compiler, self_hosted=arguments.self_hosted)
    print(f"mimi-self: tests passed with {compiler}")


if __name__ == "__main__":
    main()
