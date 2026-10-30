use super::{
    build_file, build_file_for_target, build_file_for_target_with_c_options, codegen, hir,
    lexer::tokenize, lir, mir, parser::parse,
};

#[test]
fn lowers_function_through_all_ir_layers() {
    let ast = parse(tokenize(
        "add :: fn (x : i32, y : i32) i32 { x + y }".chars(),
    ));
    let hir = hir::lower_ast(&ast);
    assert_eq!(hir.functions.len(), 1);
    assert_eq!(hir.functions[0].name, "add");

    let mir = mir::lower_hir(&hir);
    assert_eq!(mir.functions.len(), 1);
    assert!(
        mir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, mir::MirInstruction::Add { .. }))
    );

    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    assert_eq!(
        lir.target,
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget)
    );
    assert_eq!(
        lir.data_layout,
        <lir::HostTarget as lir::Target>::data_layout(&lir::HostTarget)
    );
    assert!(
        lir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, lir::LirInstruction::Add { .. }))
    );
}

#[test]
fn lowers_pointer_stride_from_target_data_layout() {
    let ast = parse(tokenize(
        "advance :: fn (value : **i32) **i32 { value + 1 }".chars(),
    ));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir_with_layout(&hir, lir::TargetDataLayout::new(4, 8));
    assert!(mir.functions[0].blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                mir::MirInstruction::IndexAddress { scale: 4, .. }
            )
        })
    }));
}

#[test]
fn builds_and_runs_tuples_through_functions_and_member_assignment() {
    let source = std::env::temp_dir().join(format!("mimi_tuple_source_{}.mi", std::process::id()));
    let executable =
        std::env::temp_dir().join(format!("mimi_tuple_executable_{}", std::process::id()));
    std::fs::write(
        &source,
        "make :: fn (value : i32) (i32, bool) { (value, true) } identity :: fn (pair : (i32, bool)) (i32, bool) { pair } main :: fn () i32 { pair := identity(make(7)) pair.0 = 9 nested := (pair.1, (pair.0, false)) nested.1.0 = 11 wide := (5000000000, true) negative := (-2147483648, true) items := [(1, true), (2, false)] items[1].0 = 3 result := if nested.0 && nested.1.0 == 11 && !nested.1.1 && wide.0 == 5000000000 && wide.1 && negative.0 == -2147483648 && negative.1 && items[1].0 == 3 && !items[1].1 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn lowers_capture_free_lambdas_to_indirectly_callable_functions() {
    let ast = parse(tokenize(
        "apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { square := fn (value : i32) i32 { value * value } apply(square, 7) + apply(fn (value : i32) i32 { value + 1 }, 2) }".chars(),
    ));
    let hir = hir::lower_ast(&ast);
    assert_eq!(hir.functions.len(), 4);
    assert!(
        hir.functions
            .iter()
            .any(|function| function.name == "__mimi_lambda_0")
    );
    assert!(
        hir.functions
            .iter()
            .any(|function| function.name == "__mimi_lambda_1")
    );

    let mir = mir::lower_hir(&hir);
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, mir::MirInstruction::CallIndirect { .. }))
        })
    }));
}

#[test]
fn lowers_f8_literals_to_e4m3fn_mir_constants() {
    let ast = parse(tokenize("value :: fn () f8 { 1.5 }".chars()));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir(&hir);
    assert!(mir.functions[0].blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                mir::MirInstruction::ConstFloat {
                    bits: 0x3c,
                    width: 8,
                    ..
                }
            )
        })
    }));
}

#[test]
fn builds_and_runs_capture_free_lambdas() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_lambda_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_lambda_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { square := fn (value : i32) i32 { value * value } apply(square, 7) + apply(fn (value : i32) i32 { value + 1 }, 2) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(52));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn imports_and_calls_c_variadic_printf() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_variadic_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_variadic_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "c :: @cimport(\"stdio.h\") main :: fn () i32 { c.printf(\"%d %f\\n\", 7, 1.5) }",
    )
    .unwrap();
    let host = <lir::HostTarget as lir::Target>::arch(&lir::HostTarget);
    for target in [lir::TargetArch::X86_64, lir::TargetArch::Aarch64] {
        let executable = path.with_extension(format!("{target:?}"));
        build_file_for_target(
            source.to_str().unwrap(),
            executable.to_str().unwrap(),
            &[],
            &[],
            &[],
            target,
        )
        .unwrap();
        if target == host {
            let status = std::process::Command::new(&executable).status().unwrap();
            assert_eq!(status.code(), Some(11));
        }
        std::fs::remove_file(executable).unwrap();
    }
    std::fs::remove_file(source).unwrap();
}

#[test]
fn imports_const_c_aggregate_globals_as_read_only() {
    let directory =
        std::env::temp_dir().join(format!("mimi_const_c_global_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("constants.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("constants.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "typedef struct Point { int x; int y; } Point; extern const Point origin;\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include \"constants.h\"\nconst Point origin = { 19, 23 };\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"constants.h\") main :: fn () i32 { c.origin.x - 19 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_and_accesses_c_thread_local_globals() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let directory = std::env::temp_dir().join(format!("mimi_c_tls_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("tls.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("tls.c");
    let executable = directory.join("main");
    std::fs::write(&header, "extern _Thread_local int counter;\n").unwrap();
    std::fs::write(&c_source, "_Thread_local int counter = 41;\n").unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"tls.h\") main :: fn () i32 { current := c.counter c.counter = current + 1 c.counter - 42 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn passes_general_slice_pointer_and_length_to_c() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let directory = std::env::temp_dir().join(format!("mimi_c_slice_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("slice.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("slice.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "#include <stddef.h>\nint first_and_len(const unsigned char *data, size_t length);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include \"slice.h\"\nint first_and_len(const unsigned char *data, size_t length) { return (int)data[0] + (int)length; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"slice.h\") main :: fn () i32 { bytes : []u8 = [1, 2] c.first_and_len(bytes.ptr, bytes.len) - 3 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn promotes_narrow_integer_c_variadic_arguments() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("mimi_variadic_narrow_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("vararg.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("vararg.c");
    let executable = directory.join("main");
    std::fs::write(&header, "int check_narrow(int count, ...);\n").unwrap();
    std::fs::write(
        &c_source,
        "#include <stdarg.h>\n#include \"vararg.h\"\nint check_narrow(int count, ...) { va_list args; va_start(args, count); int i8 = va_arg(args, int); int u8 = va_arg(args, int); int i16 = va_arg(args, int); va_end(args); return i8 == -2 && u8 == 250 && i16 == -300 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"vararg.h\") main :: fn () i32 { small : i8 = -2 byte : u8 = 250 medium : i16 = -300 c.check_narrow(3, small, byte, medium) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let output = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(output.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn promotes_f32_c_variadic_arguments_to_double() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let directory = std::env::temp_dir().join(format!("mimi_variadic_f32_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("vararg.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("vararg.c");
    std::fs::write(&header, "int check_f32(int count, ...);\n").unwrap();
    std::fs::write(
        &c_source,
        "#include <stdarg.h>\n#include \"vararg.h\"\nint check_f32(int count, ...) { va_list args; va_start(args, count); double value = va_arg(args, double); va_end(args); return (int)(value * 10.0); }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"vararg.h\") main :: fn () i32 { value : f32 = 1.5 c.check_f32(1, value) }",
    )
    .unwrap();
    let host = <lir::HostTarget as lir::Target>::arch(&lir::HostTarget);
    for target in [lir::TargetArch::X86_64, lir::TargetArch::Aarch64] {
        let executable = directory.join(format!("main-{target:?}"));
        build_file_for_target_with_c_options(
            source.to_str().unwrap(),
            executable.to_str().unwrap(),
            std::slice::from_ref(&directory),
            &[],
            &[],
            target,
            &[],
            None,
            std::slice::from_ref(&c_source),
        )
        .unwrap();
        if target == host {
            let status = std::process::Command::new(&executable).status().unwrap();
            assert_eq!(status.code(), Some(15));
        }
        std::fs::remove_file(executable).unwrap();
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builds_and_runs_lambdas_that_capture_outer_values() {
    let source =
        std::env::temp_dir().join(format!("mimi_capture_source_{}.mi", std::process::id()));
    let executable =
        std::env::temp_dir().join(format!("mimi_capture_executable_{}", std::process::id()));
    std::fs::write(
        &source,
        "apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { offset := 40 addOffset := fn (value : i32) i32 { value + offset } apply(addOffset, 2) - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_nested_lambdas_with_transitive_captures() {
    let source = std::env::temp_dir().join(format!(
        "mimi_nested_capture_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_nested_capture_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "main :: fn () i32 { offset := 40 make := fn () fn(i32)i32 { fn (value : i32) i32 { value + offset } } addOffset := make() addOffset(2) - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_lambdas_that_capture_aggregate_values() {
    let source = std::env::temp_dir().join(format!(
        "mimi_aggregate_capture_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_aggregate_capture_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "point :: struct { x : i32 } main :: fn () i32 { origin := point { x = 40 } addX := fn (value : i32) i32 { origin.x + value } addX(2) - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_generic_struct_specializations() {
    let source = std::env::temp_dir().join(format!(
        "mimi_generic_struct_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_generic_struct_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "pair :: struct <T> { x : T, y : T } offset :: struct <dx> { x : i32 = dx } main :: fn () i32 { values : pair<i32> = pair<i32> { x = 17, y = 20 } origin : offset<5> = offset<5> {} values.x + values.y + origin.x - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_generic_enum_specializations() {
    let source = std::env::temp_dir().join(format!(
        "mimi_generic_enum_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_generic_enum_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "option :: enum <T> { some(T), none } main :: fn () i32 { value : option<i32> = some(41) match value { some(number) => number + 1, none => 0 } - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn imports_and_calls_libc_abs_from_standard_header() {
    let source = std::env::temp_dir().join(format!(
        "mimi_cimport_libc_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_cimport_libc_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "c :: @cimport(\"stdlib.h\") main :: fn () i32 { c.abs(-42) - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn passes_string_literal_as_c_char_pointer() {
    let source = std::env::temp_dir().join(format!(
        "mimi_cimport_puts_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_cimport_puts_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "c :: @cimport(\"stdlib.h\") main :: fn () i32 { c.atoi(\"42\") }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(42));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn imports_libc_enum_constants() {
    let source = std::env::temp_dir().join(format!(
        "mimi_cimport_libc_constant_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_cimport_libc_constant_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "c :: @cimport(\"stdlib.h\") main :: fn () i32 { c.P_ALL }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn imports_numeric_c_macros() {
    let directory = std::env::temp_dir().join(format!("mimi_cimport_macro_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("config.h");
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "#define MIMI_TEST_VALUE 37\n#define MIMI_TEST_HEX 0x2\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"config.h\") main :: fn () i32 { c.MIMI_TEST_VALUE + c.MIMI_TEST_HEX }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(39));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn applies_source_c_define_during_header_import() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cdefine_import_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("feature.h");
    let c_source = directory.join("feature.c");
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "#ifdef MIMI_FEATURE_VALUE\nint feature_value(void);\n#else\n#error source definition was not passed to clang\n#endif\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#define MIMI_FEATURE_VALUE 42\n#include \"feature.h\"\nint feature_value(void) { return MIMI_FEATURE_VALUE; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "config :: @cDefine(\"MIMI_FEATURE_VALUE\", 42) c :: @cimport(\"feature.h\") main :: fn () i32 { c.feature_value() - 42 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn uses_imported_scalar_typedef_as_a_source_type() {
    let source =
        std::env::temp_dir().join(format!("mimi_c_typedef_source_{}.mi", std::process::id()));
    let executable =
        std::env::temp_dir().join(format!("mimi_c_typedef_executable_{}", std::process::id()));
    std::fs::write(
        &source,
        "c :: @cimport(\"stddef.h\") identity :: fn (value : c.size_t) c.size_t { value } main :: fn () i32 { value : c.size_t = identity(42) value value - value 0 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn uses_imported_pointer_typedef_in_function_calls() {
    let directory =
        std::env::temp_dir().join(format!("mimi_c_pointer_typedef_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("pointer.h");
    let c_source = directory.join("pointer.c");
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "typedef int *IntPtr; int read_int(IntPtr value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include \"pointer.h\"\nint read_int(IntPtr value) { return *value; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"pointer.h\") main :: fn () i32 { value := 42 c.read_int(&value) - 42 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_simple_c_struct_layout() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_struct_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("point.h");
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(&header, "struct Point { int x; int y; };\n").unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"point.h\") main :: fn () i32 { p := c.Point {x=7, y=5} p.x + p.y }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(12));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_c_functions_with_aggregate_arguments_and_returns() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_aggregate_abi_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("point.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("point.c");
    let host = <lir::HostTarget as lir::Target>::arch(&lir::HostTarget);
    std::fs::write(
        &header,
        "typedef struct Point { double x; double y; } Point; Point point_add(Point a, Point b); union Value { int integer; double real; }; union Value value_roundtrip(union Value value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <point.h>\nPoint point_add(Point a, Point b) { return (Point){a.x + b.x, a.y + b.y}; }\nunion Value value_roundtrip(union Value value) { return value; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"point.h\") main :: fn () i32 { a := c.Point {x=1.5, y=2.5} b := c.Point {x=3.5, y=4.5} result := c.point_add(a, b) value := c.value_roundtrip(c.Value {real=3.25}) if result.x == 5.0 && result.y == 7.0 && value.real == 3.25 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    for target in [lir::TargetArch::X86_64, lir::TargetArch::Aarch64] {
        let executable = directory.join(format!("main-{target:?}"));
        build_file_for_target_with_c_options(
            source.to_str().unwrap(),
            executable.to_str().unwrap(),
            std::slice::from_ref(&directory),
            &[],
            &[],
            target,
            &[],
            None,
            std::slice::from_ref(&c_source),
        )
        .unwrap();
        if target == host {
            let status = std::process::Command::new(&executable).status().unwrap();
            assert_eq!(status.code(), Some(0));
        }
        std::fs::remove_file(executable).unwrap();
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_and_calls_raylib_when_installed() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let Some((include, library)) = [
        ("/opt/homebrew/include", "/opt/homebrew/lib"),
        ("/usr/local/include", "/usr/local/lib"),
    ]
    .into_iter()
    .map(|(include, library)| {
        (
            std::path::PathBuf::from(include),
            std::path::PathBuf::from(library),
        )
    })
    .find(|(include, library)| {
        include.join("raylib.h").exists() && library.join("libraylib.dylib").exists()
    }) else {
        return;
    };
    let directory = std::env::temp_dir().join(format!("mimi_raylib_import_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &source,
        "ray :: @cimport(\"raylib.h\") main :: fn () i32 { if ray.GetRandomValue(0, 0) == 1 {ray.InitWindow(64, 64, \"test\") ray.DrawCircleV(ray.Vector2 {x=8.0, y=8.0}, 4.0, ray.Color {r=255, g=0, b=0, a=255}) ray.CloseWindow()} if ray.CheckCollisionPointRec(ray.Vector2 {x=5.0, y=5.0}, ray.Rectangle {x=0.0, y=0.0, width=10.0, height=10.0}) {return 0} else {return 1} 0 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&include),
        std::slice::from_ref(&library),
        &["raylib".to_string()],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_and_calls_sdl3_when_installed() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let Some((include, library)) = [
        ("/opt/homebrew/include", "/opt/homebrew/lib"),
        ("/usr/local/include", "/usr/local/lib"),
    ]
    .into_iter()
    .map(|(include, library)| {
        (
            std::path::PathBuf::from(include),
            std::path::PathBuf::from(library),
        )
    })
    .find(|(include, library)| {
        include.join("SDL3/SDL.h").exists() && library.join("libSDL3.dylib").exists()
    }) else {
        return;
    };
    let directory = std::env::temp_dir().join(format!("mimi_sdl3_import_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &source,
        "sdl :: @cimport(\"SDL3/SDL.h\") main :: fn () i32 { if false {window := sdl.SDL_CreateWindow(\"mimi\", 64, 64, 0) if window != null {sdl.SDL_DestroyWindow(window)}} if sdl.SDL_Init(0) {sdl.SDL_Quit()} else {} event := sdl.SDL_Event {type=0} sdl.SDL_PollEvent(&event) if event.type == 0 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&include),
        std::slice::from_ref(&library),
        &["SDL3".to_string()],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn passes_top_level_mimi_functions_as_c_callbacks() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_callback_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("callback.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("callback.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "int apply_callback(int (* _Nonnull callback)(int), int value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <callback.h>\nint apply_callback(int (*callback)(int), int value) { return callback(value); }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"callback.h\") double_value :: fn (value : i32) i32 { value * 2 } main :: fn () i32 { c.apply_callback(double_value, 21) - 42 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn passes_aggregate_arguments_through_c_callbacks() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_aggregate_callback_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("callback.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("callback.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "typedef struct Pair { int left; int right; } Pair; int apply_pair(int (*callback)(struct Pair), struct Pair value); int apply_pair_return(struct Pair (*callback)(struct Pair), struct Pair value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <callback.h>\nint apply_pair(int (*callback)(struct Pair), struct Pair value) { return callback(value); }\nint apply_pair_return(struct Pair (*callback)(struct Pair), struct Pair value) { struct Pair result = callback(value); return result.left == 20 && result.right == 24 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"callback.h\") sum_pair :: fn (value : c.Pair) i32 { value.left + value.right } bump_pair :: fn (value : c.Pair) c.Pair { c.Pair {left=value.left + 1, right=value.right + 1} } main :: fn () i32 { value := c.Pair {left=19, right=23} c.apply_pair(sum_pair, value) - 42 + c.apply_pair_return(bump_pair, value) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn passes_null_pointer_literals_to_c() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_null_pointer_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("pointer.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("pointer.c");
    let executable = directory.join("main");
    std::fs::write(&header, "int is_null(void *value);\n").unwrap();
    std::fs::write(
        &c_source,
        "#include <pointer.h>\nint is_null(void *value) { return value == 0; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"pointer.h\") main :: fn () i32 { if c.is_null(null) == 1 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rejects_c_callbacks_that_capture_local_values() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_callback_capture_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("callback.h");
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "int apply_callback(int (*callback)(int), int value);\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"callback.h\") main :: fn () i32 { offset := 1 callback := fn (value : i32) i32 { value + offset } c.apply_callback(callback, 41) }",
    )
    .unwrap();
    let error = build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
    )
    .unwrap_err();
    assert!(error.contains("C callbacks must be top-level non-capturing function names"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn passes_capturing_closures_through_explicit_c_callback_context() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_callback_context_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("callback.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("callback.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "int apply_callback(int (*callback)(int, void *), void *context, int value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <callback.h>\nint apply_callback(int (*callback)(int, void *), void *context, int value) { return callback(value, context); }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"callback.h\") main :: fn () i32 { offset := 1 c.apply_callback(fn (value : i32) i32 { value + offset }, 41) - 42 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_c_struct_with_mixed_field_widths() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_mixed_struct_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("mixed.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("check.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct Mixed { char tag; int count; double ratio; };\nint verify_mixed(struct Mixed *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <mixed.h>\nint verify_mixed(struct Mixed *value) { return value->tag == 7 && value->count == 5 && value->ratio == 2.5 && sizeof(*value) == 16 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"mixed.h\") main :: fn () i32 { value := c.Mixed {tag=7, count=5, ratio=2.5} c.verify_mixed(&value) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_c_enum_function_signatures() {
    let directory = std::env::temp_dir().join(format!("mimi_cimport_enum_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("color.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("color.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "enum Color { RED, GREEN = 5 }; enum Color echo_color(enum Color value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <color.h>\nenum Color echo_color(enum Color value) { return value; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"color.h\") main :: fn () i32 { color : c.Color = c.GREEN integer : i32 = color returned : c.Color = c.echo_color(integer) returned - c.GREEN }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn links_scalar_c_calls_for_each_supported_target() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_scalar_targets_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("scalar.h");
    let c_source = directory.join("scalar.c");
    let source = directory.join("main.mi");
    std::fs::write(
        &header,
        "int c_add(int lhs, int rhs); double c_add_float(double lhs, double rhs);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <scalar.h>\nint c_add(int lhs, int rhs) { return lhs + rhs; }\ndouble c_add_float(double lhs, double rhs) { return lhs + rhs; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"scalar.h\") main :: fn () i32 { if c.c_add(19, 23) == 42 && c.c_add_float(1.5, 2.25) == 3.75 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    let host = <lir::HostTarget as lir::Target>::arch(&lir::HostTarget);
    for target in [lir::TargetArch::X86_64, lir::TargetArch::Aarch64] {
        let executable = directory.join(format!("main-{target:?}"));
        build_file_for_target_with_c_options(
            source.to_str().unwrap(),
            executable.to_str().unwrap(),
            std::slice::from_ref(&directory),
            &[],
            &[],
            target,
            &[],
            None,
            std::slice::from_ref(&c_source),
        )
        .unwrap();
        if target == host {
            let status = std::process::Command::new(&executable).status().unwrap();
            assert_eq!(status.code(), Some(0));
        }
        std::fs::remove_file(executable).unwrap();
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_nested_c_struct_fields() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_nested_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("nested.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("nested.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct Inner { char tag; int count; }; struct Outer { struct Inner inner; int tail; }; int verify_outer(struct Outer *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <nested.h>\nint verify_outer(struct Outer *value) { return value->inner.tag == 7 && value->inner.count == 5 && value->tail == 3 && sizeof(*value) == 12 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"nested.h\") main :: fn () i32 { value := c.Outer {inner=c.Inner {tag=7, count=5}, tail=3} c.verify_outer(&value) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_record_typedefs_for_pointer_parameters() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_record_typedef_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("record.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("record.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "typedef struct Point { int x; int y; } Point; int sum_point(Point *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <record.h>\nint sum_point(Point *value) { return value->x + value->y; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"record.h\") main :: fn () i32 { point := c.Point {x=7, y=5} c.sum_point(&point) - 12 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_libc_timespec_layout_and_function() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_timespec_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &source,
        "c :: @cimport(\"time.h\") main :: fn () i32 { ts := c.timespec {tv_sec=0, tv_nsec=0} c.timespec_get(&ts, c.TIME_UTC) if ts.tv_sec > 0 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_mutable_c_global_variables() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_global_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("globals.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("globals.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "extern int shared_counter; int verify(int *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <globals.h>\nint shared_counter = 9;\nint verify(int *value) { return *value == 33 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"globals.h\") main :: fn () i32 { before := c.shared_counter c.shared_counter = 33 c.verify(&c.shared_counter) + if before == 9 && c.shared_counter == 33 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_mutable_c_aggregate_globals() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_aggregate_global_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("globals.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("globals.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct Point { int x; int y; }; extern struct Point shared_point; int verify_point(struct Point *point);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <globals.h>\nstruct Point shared_point = {3, 4};\nint verify_point(struct Point *point) { return point->x == 9 && point->y == 4 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"globals.h\") main :: fn () i32 { before := c.shared_point.x + c.shared_point.y c.shared_point.x = 9 c.verify_point(&c.shared_point) + if before == 7 && c.shared_point.x == 9 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rejects_writes_to_const_c_globals() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_const_global_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("globals.h");
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(&header, "extern const int fixed_value;\n").unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"globals.h\") main :: fn () i32 { c.fixed_value = 3 0 }",
    )
    .unwrap();
    let result = build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
    );
    assert!(result.is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_c_unions_with_single_member_initializers() {
    let directory = std::env::temp_dir().join(format!("mimi_cimport_union_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("value.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("value.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "union Value { int integer; double real; }; int verify_value(union Value *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <value.h>\nint verify_value(union Value *value) { return value->integer == 42 && sizeof(*value) == 8 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"value.h\") main :: fn () i32 { value := c.Value {integer=42} c.verify_value(&value) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn passes_object_pointers_to_libc_void_pointer_parameters() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_memcpy_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &source,
        "c :: @cimport(\"string.h\") main :: fn () i32 { source := 42 destination := 0 c.memcpy(&destination, &source, 4) destination - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_libc_malloc_and_free_void_pointer_signatures() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_malloc_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &source,
        "c :: @cimport(\"stdlib.h\") main :: fn () i32 { buffer := c.malloc(8) c.free(buffer) 0 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn passes_mutable_mimi_byte_buffers_to_c_char_pointers() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_mutable_buffer_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("buffer.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("buffer.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "#include <stdint.h>\nuint64_t fill_buffer(char *buffer, uint64_t length);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <buffer.h>\nuint64_t fill_buffer(char *buffer, uint64_t length) { buffer[0] = 'O'; buffer[length - 1] = 'K'; return length; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"buffer.h\") main :: fn () i32 { buffer : [2]u8 = [0, 0] view := buffer[0:2] c.fill_buffer(&view[0], view.len) if buffer.len == 2 && buffer[0] == 79 && buffer[1] == 75 {return 0} else {return 1} 0 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_byte_local_unsigned_c_bitfields() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_bitfield_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("flags.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("flags.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct Flags { unsigned int low:3; unsigned int high:5; unsigned int tail; }; int verify_flags(struct Flags *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <flags.h>\nint verify_flags(struct Flags *value) { return value->low == 2 && value->high == 17 && value->tail == 42 && sizeof(*value) == 8 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"flags.h\") main :: fn () i32 { flags := c.Flags {low=5, high=17, tail=42} flags.low = 2 c.verify_flags(&flags) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_signed_byte_local_c_bitfields() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_signed_bitfield_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("signed.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("signed.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct SignedBits { signed int value:5; unsigned int tail:3; }; int verify_signed(struct SignedBits *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <signed.h>\nint verify_signed(struct SignedBits *value) { return value->value == -3 && value->tail == 5 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"signed.h\") main :: fn () i32 { bits := c.SignedBits {value=-3, tail=5} c.verify_signed(&bits) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_multi_byte_c_bitfields() {
    let directory =
        std::env::temp_dir().join(format!("mimi_cimport_wide_bitfield_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("wide.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("wide.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct WideBits { signed int low : 12; unsigned int high : 20; unsigned int tail; }; int verify_wide(struct WideBits *bits);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <wide.h>\nint verify_wide(struct WideBits *bits) { return bits->low == 291 && bits->high == 344865 && bits->tail == 42 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"wide.h\") main :: fn () i32 { bits := c.WideBits {low=-123, high=344865, tail=42} if bits.low == -123 {bits.low = 291} else {return 1} if bits.high != 344865 {return 2} if bits.low != 291 {return 3} if bits.tail != 42 {return 4} c.verify_wide(&bits) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imports_c_bitfields_crossing_storage_unit_boundaries() {
    let directory = std::env::temp_dir().join(format!(
        "mimi_cimport_crossing_bitfield_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("crossing.h");
    let source = directory.join("main.mi");
    let c_source = directory.join("crossing.c");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct Crossing { unsigned int prefix : 28; unsigned int value : 8; unsigned int tail; }; int verify_crossing(struct Crossing *value);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <crossing.h>\nint verify_crossing(struct Crossing *value) { return value->prefix == 17 && value->value == 0xa5 && value->tail == 42 ? 0 : 1; }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"crossing.h\") main :: fn () i32 { bits := c.Crossing {prefix=17, value=90, tail=42} if bits.value != 90 {return 1} bits.value = 165 c.verify_crossing(&bits) }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn compiles_and_links_c_source_with_mimi_program() {
    let directory = std::env::temp_dir().join(format!("mimi_c_source_link_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("main.mi");
    let c_source = directory.join("helper.c");
    let header = directory.join("helper_config.h");
    let executable = directory.join("main");
    std::fs::write(
        &source,
        "extern helper : fn(value : i32)i32 main :: fn () i32 { helper(40) - 42 }",
    )
    .unwrap();
    std::fs::write(
        &header,
        "#ifndef MIMI_C_BUILD_VALUE\n#error missing compiler definition\n#endif\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include \"helper_config.h\"\nint helper(int value) { return value + 2; }\n",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &["MIMI_C_BUILD_VALUE=42".to_string()],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn uses_opaque_c_handle_through_pointer_functions() {
    let directory =
        std::env::temp_dir().join(format!("mimi_c_opaque_handle_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("handle.h");
    let c_source = directory.join("handle.c");
    let source = directory.join("main.mi");
    let executable = directory.join("main");
    std::fs::write(
        &header,
        "struct Handle; struct Handle *make_handle(void); void destroy_handle(struct Handle *handle);\n",
    )
    .unwrap();
    std::fs::write(
        &c_source,
        "#include <stdlib.h>\nstruct Handle { int value; }; struct Handle *make_handle(void) { return malloc(sizeof(struct Handle)); } void destroy_handle(struct Handle *handle) { free(handle); }\n",
    )
    .unwrap();
    std::fs::write(
        &source,
        "c :: @cimport(\"handle.h\") main :: fn () i32 { handle := c.make_handle() c.destroy_handle(handle) 0 }",
    )
    .unwrap();
    build_file_for_target_with_c_options(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        std::slice::from_ref(&directory),
        &[],
        &[],
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
        &[],
        None,
        std::slice::from_ref(&c_source),
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builds_and_runs_lowercase_generic_type_and_enum_if_let() {
    let source = std::env::temp_dir().join(format!(
        "mimi_generic_lower_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_generic_lower_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "point :: struct { x : i32 } holder :: struct <item> { value : item } option :: enum <T> { some(T), none } main :: fn () i32 { wrapped : holder<point> = holder<point> { value = point { x = 40 } } choice : option<i32> = some(2) result := wrapped.value.x if some(value) := choice { result = result + value } result - 42 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_imported_external_functions() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let package =
        std::env::temp_dir().join(format!("mimi_external_package_{}.mi", std::process::id()));
    let source =
        std::env::temp_dir().join(format!("mimi_external_import_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_external_codegen_{}", std::process::id()));
    std::fs::write(
        &package,
        "pub extern abs : fn(value : i32)i32 pub abs_value :: abs pub call_abs :: fn(value : i32)i32 { abs(value) }",
    )
    .unwrap();
    std::fs::write(
        &source,
        format!(
            "system :: @import(\"{}\") apply :: fn (function : fn(i32)i32, value : i32) i32 {{ function(value) }} main :: fn () i32 {{ system.abs(-17) + system.abs_value(-2) + system.call_abs(-3) + apply(system.abs_value, -1) }}",
            package.display()
        ),
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(23));
    std::fs::remove_file(package).unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_imported_scalar_constants() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let package =
        std::env::temp_dir().join(format!("mimi_constant_package_{}.mi", std::process::id()));
    let source =
        std::env::temp_dir().join(format!("mimi_constant_import_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_constant_codegen_{}", std::process::id()));
    std::fs::write(&package, "pub answer : i32 : 37").unwrap();
    std::fs::write(
        &source,
        format!(
            "values :: @import(\"{}\") main :: fn () i32 {{ values.answer }}",
            package.display()
        ),
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(37));
    std::fs::remove_file(package).unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_imported_scalar_constant_expressions() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let package =
        std::env::temp_dir().join(format!("mimi_expression_package_{}.mi", std::process::id()));
    let source =
        std::env::temp_dir().join(format!("mimi_expression_import_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_expression_codegen_{}", std::process::id()));
    std::fs::write(
        &package,
        "BASE :: 40 pub answer :: BASE + 2 pub ratio :: 1.0 + 0.25 pub enabled :: answer > 40 pub large :: 2147483648",
    )
    .unwrap();
    std::fs::write(
        &source,
        format!(
            "values :: @import(\"{}\") main :: fn () i32 {{ result := if values.enabled && values.ratio == 1.25 && values.large == 2147483648 {{ values.answer }} else {{ 0 }} result }}",
            package.display()
        ),
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(42));
    std::fs::remove_file(package).unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_imported_struct_types() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let package =
        std::env::temp_dir().join(format!("mimi_struct_package_{}.mi", std::process::id()));
    let source = std::env::temp_dir().join(format!("mimi_struct_import_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_struct_codegen_{}", std::process::id()));
    std::fs::write(
        &package,
        "pub Point :: struct { x : i32, y : i32, get_y() :: fn (self : Self) i32 { self.y } } pub Choice :: enum { first, second(i32) } pub get_x :: fn(point : Point)i32 { point.x }",
    )
    .unwrap();
    std::fs::write(
        &source,
        format!(
            "geom :: @import(\"{}\") main :: fn () i32 {{ point : geom.Point = geom.Point {{ x=19, y=23 }} choice : geom.Choice = second(7) geom.get_x(point) + point.get_y() + match choice {{ first => 0, second(value) => value }} }}",
            package.display()
        ),
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(49));
    std::fs::remove_file(package).unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_imported_aggregate_constants() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let package =
        std::env::temp_dir().join(format!("mimi_aggregate_package_{}.mi", std::process::id()));
    let source =
        std::env::temp_dir().join(format!("mimi_aggregate_import_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_aggregate_codegen_{}", std::process::id()));
    std::fs::write(
        &package,
        "pub Point :: struct { x : i32, y : i32 } pub origin :: Point { x=33, y=0 } pub numbers : [2]i32 : [2, 3] pub Choice :: enum { none, some(i32) } pub choice :: some(7) pub message :: \"A\"",
    )
    .unwrap();
    std::fs::write(
        &source,
        format!(
            "geo :: @import(\"{}\") main :: fn () i32 {{ geo.origin.x + geo.numbers[0] + match geo.choice {{ some(value) => value, none => 0 }} + geo.message[0] - 65 }}",
            package.display()
        ),
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(42));
    std::fs::remove_file(package).unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn lowers_dynamic_if_else_to_mir_blocks_and_phi() {
    let ast = parse(tokenize(
        "choose :: fn (condition : bool) i32 { value := if condition { 7 } else { 3 } value }"
            .chars(),
    ));
    let mir = mir::lower_hir(&hir::lower_ast(&ast));
    let function = &mir.functions[0];
    assert_eq!(function.blocks.len(), 4);
    assert!(matches!(
        function.blocks[0].terminator,
        mir::MirTerminator::Branch { .. }
    ));
    assert!(
        function.blocks[3]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, mir::MirInstruction::Phi { .. }))
    );
}

#[test]
fn builds_and_runs_dynamic_if_else_expression() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_dynamic_if_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_dynamic_if_codegen_{}", std::process::id()));
    std::fs::write(&source, "choose :: fn (condition : bool) i32 { value := if condition { 7 } else { 3 } value } main :: fn () i32 { choose(true) }").unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(7));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_logical_boolean_expressions() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_logic_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_logic_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "main :: fn () i32 { result := if (false && (1 / 0 == 0)) || true { 9 } else { 2 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(9));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_while_and_do_while_cfg() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_loop_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_loop_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "main :: fn () i32 { while false { 1 } do { 2 } while false 7 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(7));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_unconditional_loop_with_return() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_loop_return_source_{}.mi", std::process::id()));
    let path =
        std::env::temp_dir().join(format!("mimi_loop_return_codegen_{}", std::process::id()));
    std::fs::write(&source, "main :: fn () i32 { loop { return 17 } 2 }").unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(17));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_while_body_with_early_return() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_while_return_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_while_return_codegen_{}", std::process::id()));
    std::fs::write(&source, "main :: fn () i32 { while true { return 13 } 2 }").unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(13));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_mutable_locals_across_loops() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_mutable_loop_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_mutable_loop_codegen_{}", std::process::id()));
    for (program, expected) in [
        (
            "main :: fn () i32 { i := 0 while i < 5 { i = i + 1 } i }",
            5,
        ),
        (
            "main :: fn () i32 { i := 0 do { i = i + 1 } while i < 3 i }",
            3,
        ),
        (
            "bump :: fn (i : i32) i32 { i = i + 1 i } main :: fn () i32 { bump(4) }",
            5,
        ),
        (
            "identity :: fn (value : i32) i32 { value } sum_after_call :: fn (value : i32) i32 { value + identity(4) } main :: fn () i32 { sum_after_call(3) }",
            7,
        ),
        (
            "update :: fn (condition : bool) i32 { count := 0 value := if condition == true { count = 5 count } else { count = 8 count } count } main :: fn () i32 { update(false) }",
            8,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_fixed_array_literals_and_indexing() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_array_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_array_codegen_{}", std::process::id()));
    for (program, expected) in [
        ("main :: fn () i32 { values := [4, 7, 9] values[1] }", 7),
        (
            "pick :: fn (values : [3]i32) i32 { values[2] } main :: fn () i32 { pick([1, 2, 6]) }",
            6,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_fixed_array_element_assignment() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_array_store_source_{}.mi", std::process::id()));
    let path =
        std::env::temp_dir().join(format!("mimi_array_store_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "main :: fn () i32 { values := [2, 4, 6] values[1] = 9 values[0] + values[1] + values[2] }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(17));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_pointer_reads_and_writes() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_pointer_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_pointer_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "get :: fn (pointer : *i32) i32 { *pointer } set :: fn (pointer : *i32) void { *pointer = 9 } main :: fn () i32 { value := 4 set(&value) get(&value) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(9));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_match_and_if_let_for_payloadless_enums() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_enum_match_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_enum_match_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "gender :: enum { male, female } identity :: fn (value : gender) gender { value } choose :: fn (value : gender) i32 { match value { male => 1, female => 4 } } detect :: fn (value : gender) i32 { result := 0 if male := identity(value) { result = 3 } else { result = 7 } result } main :: fn () i32 { choose(male) + choose(female) + detect(male) + detect(female) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(15));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_payload_enum_construction_match_and_if_let() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_payload_enum_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_payload_enum_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "result :: enum { some(i32), error, none } choose :: fn (value : result) i32 { match value { some(x) => x, error => 3, none => 5 } } unwrap :: fn (value : result) i32 { output := 0 if some(x) := value { output = x } output } main :: fn () i32 { choose(some(7)) + choose(error) + choose(none) + unwrap(some(4)) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(19));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_pointer_to_pointer_indexing_and_arithmetic() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_pointer_to_pointer_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_pointer_to_pointer_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "main :: fn () i32 { value := 37 value_pointer := &value pointer := &value_pointer next := pointer + 1 result := if *(pointer[0]) == 37 && next - pointer == 1 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_pointer_indexing_and_assignment() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_pointer_index_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_pointer_index_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "sum :: fn (pointer : *i32) i32 { pointer[0] + pointer[1] + pointer[2] } set :: fn (pointer : *i32) void { pointer[1] = 7 } main :: fn () i32 { values := [2, 3, 5] set(&values[0]) sum(&values[0]) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(14));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_scalar_struct_fields_and_defaults() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_struct_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_struct_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "pos :: struct { x : i32, y : i32 = 3 } get_x :: fn (point : pos) i32 { point.x } main :: fn () i32 { point := pos { x = 7 } copy : pos = { x = 4, } pointer := &point pointer.y = 6 point.x = 10 get_x(point) + get_x({ x = 4, }) + point.y + copy.y }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(23));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_aggregate_parameters_and_returns_by_value() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_aggregate_abi_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_aggregate_abi_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "point :: struct { x : i32, y : i32 } wrapper :: struct { point : point } make :: fn (x : i32) wrapper { wrapper { point = point { x = x, y = 2 } } } replace :: fn (value : wrapper) wrapper { value.point.x = 100 value } apply :: fn (function : fn(wrapper)wrapper, value : wrapper) wrapper { function(value) } roundtrip :: fn (values : [2]wrapper) [2]wrapper { values } main :: fn () i32 { original := make(9) changed := apply(replace, original) values : [2]wrapper = [original, make(4)] returned := roundtrip(values) original.point.x = 10 values[1].point.x = 30 result := if original.point.x == 10 && changed.point.x == 100 && changed.point.y == 2 && returned[0].point.x == 9 && returned[1].point.x == 4 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_multiword_enum_payloads() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_large_enum_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_large_enum_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "point :: struct { x : i32, y : i32 } wrapper :: struct { point : point } choice :: enum { batch([2]wrapper), none } make :: fn (x : i32) choice { batch([wrapper { point = point { x = x, y = 2 } }, wrapper { point = point { x = x + 1, y = 3 } }]) } identity :: fn (value : choice) choice { value } inspect :: fn (value : choice) i32 { match value { batch(items) => items[1].point.x, none => 0 } } inspect_if :: fn (value : choice) i32 { result := 0 if batch(items) := value { result = items[0].point.x } result } main :: fn () i32 { original := make(4) copied := identity(original) result := inspect(copied) if_result := inspect_if(copied) check := if result == 5 && if_result == 4 { 0 } else { 1 } check }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_slices_inside_aggregates() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_aggregate_slice_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_aggregate_slice_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "sum :: fn (values : []i32) i32 { values[0] + values[1] } holder :: struct { values : []i32 } choice :: enum { values([]i32), none } inspect :: fn (value : choice) i32 { match value { values(items) => sum(items), none => 0 } } main :: fn () i32 { first : [2]i32 = [1, 2] second : [2]i32 = [3, 4] wrapped := holder { values = first[:] } wrapped.values[1] = 7 slices : [2][]i32 = [first[:], second[:]] selected := slices[1] selected[0] = 9 option := values(wrapped.values) result := inspect(option) + sum(slices[0]) + sum(selected) result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(29));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_fixed_arrays_of_structs() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_struct_array_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_struct_array_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "point :: struct { x : i32, y : i32 } second_y :: fn (points : []point) i32 { points[1].y } main :: fn () i32 { original := point { x = 1, y = 2 } points : [2]point = [point { x = 3, y = 4 }, original] original.x = 9 first := points[1].x points[0].x = 7 points[1].x = 11 points[1].y = 12 first + points[0].x + points[0].y + points[1].x + points[1].y + second_y(points[:]) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(47));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_nested_array_layouts() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_nested_array_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_nested_array_codegen_{}", std::process::id()));
    for (program, expected) in [
        (
            "matrix :: struct { cells : [2][3]i32 } main :: fn () i32 { items := [[1, 2, 3], [4, 5, 6]] copy := matrix { cells = items } items[1][0] = 7 copy.cells[1][2] = 9 items[1][0] + items[0][2] + copy.cells[1][2] + copy.cells[0][0] }",
            20,
        ),
        (
            "cell :: struct { value : i32 } matrix :: struct { cells : [2][2]cell } main :: fn () i32 { items := [[cell { value = 1 }, cell { value = 2 }], [cell { value = 3 }, cell { value = 4 }]] copy := matrix { cells = items } items[1][0].value = 9 copy.cells[0][1].value = 8 items[1][0].value + items[0][0].value + copy.cells[0][1].value + copy.cells[1][1].value }",
            22,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_arrays_of_payload_enums() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_enum_array_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_enum_array_codegen_{}", std::process::id()));
    for (program, expected) in [
        (
            "result :: enum { some(i32), error, none } main :: fn () i32 { values : [3]result = [some(7), none, some(9)] first := match values[0] { some(value) => value, error => 0, none => 0 } second := match values[1] { some(value) => value, error => 0, none => 2 } third := match values[2] { some(value) => value, error => 0, none => 0 } first + second + third }",
            18,
        ),
        (
            "result :: enum { some(i32), none } choice :: struct { value : result } main :: fn () i32 { item := choice { value = some(11) } match item.value { some(value) => value, none => 0 } }",
            11,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_pointers_to_structs_and_enums() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_compound_pointer_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_compound_pointer_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "point :: struct { x : i32, y : i32 } result :: enum { some(i32), none } update :: fn (points : *point) i32 { points[1].x = 7 points[1].x + points[0].y } select :: fn (values : *result) i32 { match values[0] { some(value) => value, none => 0 } } read_matrix :: fn (matrix : *[2]point) i32 { matrix[0][1].x } shift_point :: fn (pointer : *point) *point { pointer + 1 } point_distance :: fn (left : *point, right : *point) i64 { right - left } shift_result :: fn (pointer : *result) *result { pointer + 1 } result_distance :: fn (left : *result, right : *result) i64 { right - left } main :: fn () i32 { points : [2]point = [point { x = 1, y = 2 }, point { x = 3, y = 4 }] values : [2]result = [none, some(8)] shifted_point := shift_point(&points[0]) shifted_result := shift_result(&values[0]) distances_ok := point_distance(&points[0], shifted_point) == 1 && result_distance(&values[0], shifted_result) == 1 && read_matrix(&points) == shifted_point[0].x answer := if distances_ok { shifted_point[0].x + select(shifted_result) } else { 0 } answer }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(11));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_nested_struct_field_access() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_nested_struct_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_nested_struct_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "inner :: struct { x : i32, z : i32 } middle :: struct { nested : inner, w : i32 } outer :: struct { nested : middle, y : i32 } main :: fn () i32 { original := inner { x = 3, z = 5 } middle_value := middle { nested = original, w = 7 } value := outer { nested = middle_value, y = 4 } original.x = 8 pointer := &value pointer.nested.nested.z = 6 before := value.nested.nested.x value.nested.nested.x = 9 before + value.nested.nested.x + value.nested.nested.z + value.nested.w + value.y }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(29));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_for_loops_over_fixed_arrays() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_for_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_for_codegen_{}", std::process::id()));
    for program in [
        "main :: fn () i32 { values := [1, 2, 3, 4] sum := 0 for item in values { sum = sum + item } sum }",
        "main :: fn () i32 { sum := 0 for item in [2, 3, 5] { sum = sum + item } sum }",
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(10));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_for_loops_over_array_slices() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_array_slice_for_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_array_slice_for_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "main :: fn () i32 { values := [1, 2, 3, 4] start := 2 sum := 0 for item in values[1:3] { sum = sum + item } for item in values[start:] { sum = sum + item } view := values[1:3] for item in view { sum = sum + item } view = values[0:1] for item in view { sum = sum + item } subview := view[1:] for item in subview { sum = sum + item } sum = sum + view[0] sum }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(19));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_for_loops_over_string_slices() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_string_slice_for_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_string_slice_for_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "main :: fn () i32 { text := \"ABC\" sum := 0 for byte in text { sum = sum + byte } for byte in \"DE\" { sum = sum + byte } sum = sum + text[1] text = \"D\" for byte in text { sum = sum + byte } sum = sum + text[0] sum }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(25));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn passes_slice_pointer_and_length_to_functions() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_slice_parameter_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_slice_parameter_codegen_{}",
        std::process::id()
    ));
    for (program, expected) in [
        (
            "sum :: fn (values : []i32) i32 { total := 0 for item in values { total = total + item } total } main :: fn () i32 { values := [1, 2, 3, 4] sum(values[1:3]) }",
            5,
        ),
        (
            "sum_bytes :: fn (values : []u8) i32 { total := 0 for byte in values { total = total + byte } total } main :: fn () i32 { sum_bytes(\"ABC\") }",
            198,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_packed_arrays_across_slice_boundaries() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_u8_array_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_u8_array_codegen_{}", std::process::id()));
    for (program, expected) in [
        (
            "sum_bytes :: fn (values : []u8) i32 { total := 0 for byte in values { total = total + byte } total } main :: fn () i32 { values : [10]u8 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] values[8] = 20 sum_bytes(values[:]) }",
            66,
        ),
        (
            "sum_bytes :: fn (values : []u8) i32 { total := 0 for byte in values { total = total + byte } total } main :: fn () i32 { sum_bytes([65, 66, 67]) }",
            198,
        ),
        (
            "sum_words :: fn (values : []i16) i32 { total := 0 for word in values { total = total + word } total } main :: fn () i32 { values : [6]i16 = [-2, 50, 60, 70, 80, 90] values[1] = 51 result := if sum_words(values[:]) == 349 { 0 } else { 1 } result }",
            0,
        ),
        (
            "sum_floats :: fn (values : []f32) f32 { total : f32 = 0 for value in values { total = total + value } total } main :: fn () i32 { result := if sum_floats([1.25, 2.25]) == 3.5 { 0 } else { 1 } result }",
            0,
        ),
        (
            "sum_words :: fn (values : []u32) u32 { total : u32 = 0 for word in values { total = total + word } total } main :: fn () u32 { sum_words([40, 50]) }",
            90,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_scaled_pointer_arithmetic() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_pointer_arithmetic_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_pointer_arithmetic_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "main :: fn () i32 { values := [10, 20, 30, 40] pointer := &values[0] third := pointer + 2 second := third - 1 first := 1 + pointer bytes : [5]u8 = [1, 2, 3, 4, 5] byte_pointer := &bytes[0] total := *third + *second + *first + *(byte_pointer + 3) total }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(74));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_pointer_differences_in_elements() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_pointer_difference_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_pointer_difference_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "main :: fn () i32 { values := [10, 20, 30, 40] start := &values[0] end := start + 3 bytes : [5]u8 = [1, 2, 3, 4, 5] byte_start := &bytes[0] byte_end := byte_start + 4 result := if end - start == 3 && start - end == -3 && byte_end - byte_start == 4 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_indexing_through_pointers_to_arrays() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_pointer_to_array_source_{}.mi",
        std::process::id()
    ));
    let path = std::env::temp_dir().join(format!(
        "mimi_pointer_to_array_codegen_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "main :: fn () i32 { values : [3]i32 = [10, 20, 30] start := &values next := start + 1 first := if start[0][1] == 20 { 0 } else { 1 } second := if start[0][2] == 30 { 0 } else { 2 } third := if next - start == 1 { 0 } else { 4 } first + second + third }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_arrays_of_pointers_through_array_pointers() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_pointer_array_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_pointer_array_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "main :: fn () i32 { first := 11 second := 22 values : [2]*i32 = [&first, &second] start := &values result := if *(start[0][0]) == 11 && *(start[0][1]) == 22 && start + 1 - start == 1 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_slice_return_values() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_slice_return_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_slice_return_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "identity :: fn (values : []u8) []u8 { return values } forward :: fn (values : []u8) []u8 { identity(values) } apply :: fn (function : fn([]u8)[]u8, values : []u8) []u8 { function(values) } choose :: fn (values : []u8, tail : bool) []u8 { chosen := if tail { values[1:] } else { values[0:2] } chosen } sum :: fn (values : []u8) i32 { total := 0 for item in values { total = total + item } total } main :: fn () i32 { returned := apply(forward, \"ABC\") selected := choose(returned, true) sum(selected) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(133));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_break_continue_and_loop_defers() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_loop_control_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_loop_control_codegen_{}", std::process::id()));
    for (program, expected) in [
        (
            "main :: fn () i32 { i := 0 sum := 0 while i < 5 { i = i + 1 defer { sum = sum + 1 } if i == 2 { continue } if i == 4 { break } sum = sum + 10 } sum }",
            24,
        ),
        (
            "main :: fn () i32 { outer := 0 sum := 0 while outer < 3 { outer = outer + 1 inner := 0 loop { inner = inner + 1 sum = sum + 1 if inner == 2 { break } } sum = sum + 10 } sum }",
            36,
        ),
        (
            "main :: fn () i32 { sum := 0 for item in [1, 2, 3, 4] { if item == 2 { continue } sum = sum + item } sum }",
            8,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_early_return_in_if_expression() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_return_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_return_codegen_{}", std::process::id()));
    for (condition, expected) in [("true", 11), ("false", 5)] {
        std::fs::write(
            &source,
            format!("choose :: fn (condition : bool) i32 {{ value := if condition {{ return 11 }} else {{ 5 }} value }} main :: fn () i32 {{ choose({condition}) }}"),
        )
        .unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_if_expression_with_both_branches_returning() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_both_return_source_{}.mi", std::process::id()));
    let path =
        std::env::temp_dir().join(format!("mimi_both_return_codegen_{}", std::process::id()));
    for (condition, expected) in [("true", 11), ("false", 5)] {
        std::fs::write(
            &source,
            format!("choose :: fn (condition : bool) i32 {{ value := if condition {{ return 11 }} else {{ return 5 }} value }} main :: fn () i32 {{ choose({condition}) }}"),
        )
        .unwrap();
        build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&path).status().unwrap();
        assert_eq!(status.code(), Some(expected));
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn emits_host_assembly_for_integer_function() {
    let ast = parse(tokenize(
        "add :: fn (x : i32, y : i32) i32 { x + y }".chars(),
    ));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir(&hir);
    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    if cfg!(target_arch = "aarch64") {
        let assembly = codegen::emit_assembly(&lir).unwrap();
        assert!(assembly.contains("add x"));
        assert!(assembly.contains("ldr x"));
        assert!(assembly.contains("str x"));
        assert!(assembly.contains("ret"));
    }
}

#[test]
fn emits_and_assembles_x86_64_integer_control_flow() {
    let ast = parse(tokenize(
        "combine :: fn (left : i32, right : i32) i32 { if left < right { left + right } else { left - right } } main :: fn () i32 { combine(9, 4) }"
            .chars(),
    ));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir_with_layout(&hir, lir::TargetDataLayout::HOST_64);
    let target = lir::X86_64Target;
    let lir = lir::lower_mir(&mir, &target);
    let assembly = codegen::emit_assembly(&lir).unwrap();

    assert!(assembly.contains(".intel_syntax noprefix"));
    assert!(assembly.contains("cmp rax, rcx"));
    assert!(assembly.contains("jz .Lcombine_"));
    assert!(assembly.contains("call _combine"));
    assert!(assembly.contains("leave\n    ret"));

    let assembly_path =
        std::env::temp_dir().join(format!("mimi_x86_assembly_{}.s", std::process::id()));
    let object_path =
        std::env::temp_dir().join(format!("mimi_x86_assembly_{}.o", std::process::id()));
    std::fs::write(&assembly_path, assembly).unwrap();
    let mut command = std::process::Command::new("cc");
    if cfg!(target_os = "macos") {
        command.args(["-arch", "x86_64"]);
    }
    let status = command
        .arg("-c")
        .arg(&assembly_path)
        .arg("-o")
        .arg(&object_path)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_file(assembly_path).unwrap();
    std::fs::remove_file(object_path).unwrap();
}

#[test]
fn emits_and_assembles_x86_64_float_and_stack_arguments() {
    let ast = parse(tokenize(
        "sum10 :: fn (a : i32, b : i32, c : i32, d : i32, e : i32, f : i32, g : i32, h : i32, i : i32, j : i32) i32 { a + b + c + d + e + f + g + h + i + j } add32 :: fn (left : f32, right : f32) f32 { left + right } multiply16 :: fn (left : f16, right : f16) f16 { left * right } main :: fn () i32 { sum10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10) + if add32(1.0, 2.0) > 2.0 { 1 } else { 0 } }"
            .chars(),
    ));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir_with_layout(&hir, lir::TargetDataLayout::HOST_64);
    let target = lir::X86_64Target;
    let lir = lir::lower_mir(&mir, &target);
    let assembly = codegen::emit_assembly(&lir).unwrap();

    assert!(assembly.contains("addss xmm0, xmm1"));
    assert!(assembly.contains("vcvtph2ps"));
    assert!(assembly.contains("vcvtps2ph"));
    assert!(assembly.contains("call _sum10"));
    assert!(assembly.contains("[rsp + 0]"));

    let assembly_path =
        std::env::temp_dir().join(format!("mimi_x86_float_assembly_{}.s", std::process::id()));
    let object_path =
        std::env::temp_dir().join(format!("mimi_x86_float_assembly_{}.o", std::process::id()));
    std::fs::write(&assembly_path, assembly).unwrap();
    let mut command = std::process::Command::new("cc");
    if cfg!(target_os = "macos") {
        command.args(["-arch", "x86_64"]);
    }
    let status = command
        .arg("-c")
        .arg(&assembly_path)
        .arg("-o")
        .arg(&object_path)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_file(assembly_path).unwrap();
    std::fs::remove_file(object_path).unwrap();
}

#[test]
fn builds_x86_64_executable_for_selected_target() {
    let source =
        std::env::temp_dir().join(format!("mimi_x86_target_source_{}.mi", std::process::id()));
    let executable =
        std::env::temp_dir().join(format!("mimi_x86_target_executable_{}", std::process::id()));
    std::fs::write(
        &source,
        "choose :: fn (left : f32, right : f32) i32 { result := if left + right > 3.0 { 41 } else { 1 } result } main :: fn () i32 { choose(1.5, 2.0) }",
    )
    .unwrap();
    build_file_for_target(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
        lir::TargetArch::X86_64,
    )
    .unwrap();
    assert!(executable.is_file());

    let status = if cfg!(target_arch = "x86_64") {
        std::process::Command::new(&executable).status().unwrap()
    } else {
        std::process::Command::new("arch")
            .args(["-x86_64"])
            .arg(&executable)
            .status()
            .unwrap()
    };
    assert_eq!(status.code(), Some(41));

    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_x86_64_executable_linked_with_c_print_library() {
    if !cfg!(target_os = "macos") && !cfg!(target_arch = "x86_64") {
        return;
    }
    let directory = std::env::temp_dir().join(format!("mimi_x86_print_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let object = directory.join("print.o");
    let library = directory.join("libprint.a");
    let source = directory.join("main.mi");
    let executable = directory.join("main");

    let runtime_source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/print.c");
    let mut compile = std::process::Command::new("cc");
    if cfg!(target_os = "macos") {
        compile.args(["-arch", "x86_64"]);
    }
    assert!(
        compile
            .arg("-c")
            .arg(runtime_source)
            .arg("-o")
            .arg(&object)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("ar")
            .arg("rcs")
            .arg(&library)
            .arg(&object)
            .status()
            .unwrap()
            .success()
    );

    std::fs::write(
        &source,
        "extern print : fn(str : []u8)void main :: fn () i32 { print(\"hello\") 0 }",
    )
    .unwrap();
    build_file_for_target(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        std::slice::from_ref(&directory),
        &["print".to_string()],
        lir::TargetArch::X86_64,
    )
    .unwrap();
    let output = if cfg!(target_arch = "x86_64") {
        std::process::Command::new(&executable).output().unwrap()
    } else {
        std::process::Command::new("arch")
            .args(["-x86_64"])
            .arg(&executable)
            .output()
            .unwrap()
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"hello");

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builds_x86_64_arrays_slices_pointers_and_loops() {
    let source =
        std::env::temp_dir().join(format!("mimi_x86_memory_source_{}.mi", std::process::id()));
    let executable =
        std::env::temp_dir().join(format!("mimi_x86_memory_executable_{}", std::process::id()));
    for (program, expected) in [
        (
            "main :: fn () i32 { values : [4]i32 = [3, 5, 7, 9] values[0] - 3 }",
            0,
        ),
        (
            "main :: fn () i32 { values : [4]i32 = [3, 5, 7, 9] values[1] = 11 values[1] - 11 }",
            0,
        ),
        (
            "main :: fn () i32 { values : [4]i32 = [3, 5, 7, 9] pointer := &values[0] pointer[2] = 13 values[2] - 13 }",
            0,
        ),
        (
            "main :: fn () i32 { values : [4]i32 = [3, 5, 7, 9] total := 0 for item in values { total = total + item } result := if total == 24 { 0 } else { 1 } result }",
            0,
        ),
        (
            "sum :: fn (values : []i32) i32 { total := 0 for item in values { total = total + item } total } main :: fn () i32 { values : [4]i32 = [3, 5, 7, 9] result := if sum(values[:]) == 24 { 0 } else { 1 } result }",
            0,
        ),
        (
            "point :: struct { x : i32, y : i32 } shift :: fn (value : point) point { point { x = value.x + 1, y = value.y + 2 } } main :: fn () i32 { value := shift(point { x = 4, y = 5 }) result := if value.x == 5 && value.y == 7 { 0 } else { 1 } result }",
            0,
        ),
        (
            "result :: enum { some(i32), none } make :: fn (value : i32) result { some(value) } main :: fn () i32 { match make(7) { some(value) => value - 7, none => 1 } }",
            0,
        ),
        (
            "mix :: fn (a : i32, b : f16, c : i32, d : f32, e : i32, f : f64, g : i32, h : f16, i : i32, j : f32, k : i32, l : f64, m : i32) i32 { a * 1000000 + c * 100000 + e * 10000 + g * 1000 + i * 100 + k * 10 + m + if b == 1.5 && d == 2.0 && f == 3.0 && h == 2.5 && j == 3.0 && l == 4.0 { 0 } else { 1 } } apply :: fn (function : fn(i32,f16,i32,f32,i32,f64,i32,f16,i32,f32,i32,f64,i32)i32) i32 { function(1, 1.5, 2, 2.0, 3, 3.0, 4, 2.5, 5, 3.0, 6, 4.0, 7) } main :: fn () i32 { apply(mix) - 1234567 }",
            0,
        ),
        (
            "main :: fn () i32 { i := 0 do { i = i + 1 } while i < 4 i - 4 }",
            0,
        ),
        (
            "main :: fn () i32 { i := 0 sum := 0 while i < 5 { i = i + 1 defer { sum = sum + 1 } if i == 2 { continue } if i == 4 { break } sum = sum + 10 } sum - 24 }",
            0,
        ),
        (
            "square :: fn (value : i32) i32 { value * value } apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { before := 9 constant :: square function := constant result := apply(function, 5) + function(6) + before + constant(4) result - 86 }",
            0,
        ),
        (
            "pos :: struct { offset() :: fn (value : i32) i32 { value + 1 } } apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { method := pos::offset apply(pos::offset, 5) + method(5) - 12 }",
            0,
        ),
        (
            "apply :: fn (function : fn(i32)i32) i32 { function(3) } pos :: struct { x : i32, add() :: fn (self : Self, value : i32) i32 { self.x + value } } make :: fn () fn(i32)i32 { point := pos { x = 7 } point.add } main :: fn () i32 { method := make() apply(method) - 10 }",
            0,
        ),
        (
            "pos :: struct { x : f64, add() :: fn (self : Self, value : f64) f64 { self.x + value } } main :: fn () i32 { point := pos { x = 1.5 } method := point.add result := if method(2.0) == 3.5 { 0 } else { 1 } result }",
            0,
        ),
        (
            "counter :: struct { x : i32, increment() :: fn (self : *Self) i32 { self.x = self.x + 1 self.x } } main :: fn () i32 { value := counter { x = 4 } pointer := &value method := pointer.increment method() + value.x - 10 }",
            0,
        ),
        (
            "add :: fn (left : f8, right : f8) f8 { left + right } main :: fn () i32 { max : f8 = 448.0 saturated := add(max, max) result := if add(1.5, 2.25) == 3.75 && saturated == max && -1.5 < 0.0 { 0 } else { 1 } result }",
            0,
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        build_file_for_target(
            source.to_str().unwrap(),
            executable.to_str().unwrap(),
            &[],
            &[],
            &[],
            lir::TargetArch::X86_64,
        )
        .unwrap();
        let status = if cfg!(target_arch = "x86_64") {
            std::process::Command::new(&executable).status().unwrap()
        } else {
            std::process::Command::new("arch")
                .args(["-x86_64"])
                .arg(&executable)
                .status()
                .unwrap()
        };
        assert_eq!(status.code(), Some(expected), "program: {program}");
    }

    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn emits_host_assembly_for_calls_and_control_flow() {
    if !cfg!(target_arch = "aarch64") {
        return;
    }
    let ast = parse(tokenize(
        r#"extern print : fn(str : []u8)void identity :: fn (value : i32) i32 { value } sum10 :: fn (a : i32, b : i32, c : i32, d : i32, e : i32, f : i32, g : i32, h : i32, i : i32, j : i32) i32 { a + b + c + d + e + f + g + h + i + j } main :: fn () i32 { sum10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10) i := 0 total := 0 while i < 3 { total = total + identity(i) i = i + 1 } if total == 3 { print("ok") 10 } else { 1 } }"#.chars(),
    ));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir(&hir);
    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    let assembly = codegen::emit_assembly(&lir).unwrap();
    assert!(assembly.contains("bl _identity"));
    assert!(assembly.contains("bl _sum10"));
    assert!(assembly.contains("bl _print"));
    assert!(assembly.contains("adrp x"));
    assert!(assembly.contains(".section __TEXT,__const"));
    assert!(assembly.contains("_mimi_string_0:"));
    assert!(assembly.contains("cbz x"));
    assert!(assembly.contains(".Lmain_"));
    assert!(assembly.contains("stp x29, x30"));
    assert!(assembly.contains("ldp x29, x30"));
    assert!(assembly.contains("ldr x8, [sp, #"));
    let assembly_path =
        std::env::temp_dir().join(format!("mimi_text_assembly_{}.s", std::process::id()));
    let object_path =
        std::env::temp_dir().join(format!("mimi_text_assembly_{}.o", std::process::id()));
    std::fs::write(&assembly_path, assembly).unwrap();
    let status = std::process::Command::new("cc")
        .arg("-c")
        .arg(&assembly_path)
        .arg("-o")
        .arg(&object_path)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_file(assembly_path).unwrap();
    std::fs::remove_file(object_path).unwrap();
}

#[test]
fn preserves_callee_saved_registers_under_register_pressure() {
    if !cfg!(target_arch = "aarch64") {
        return;
    }
    let mut expression = "20".to_string();
    for value in (1..20).rev() {
        expression = format!("{value} + ({expression})");
    }
    let source_text =
        format!("pressure :: fn () i32 {{ {expression} }} main :: fn () i32 {{ pressure() }}");
    let ast = parse(tokenize(source_text.chars()));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir(&hir);
    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    let assembly = codegen::emit_assembly(&lir).unwrap();
    assert!(assembly.contains("str x19, [sp, #"));
    assert!(assembly.contains("ldr x19, [sp, #"));

    let assembly_path = std::env::temp_dir().join(format!(
        "mimi_callee_saved_assembly_{}.s",
        std::process::id()
    ));
    let object_path = std::env::temp_dir().join(format!(
        "mimi_callee_saved_assembly_{}.o",
        std::process::id()
    ));
    std::fs::write(&assembly_path, assembly).unwrap();
    let status = std::process::Command::new("cc")
        .arg("-c")
        .arg(&assembly_path)
        .arg("-o")
        .arg(&object_path)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::remove_file(assembly_path).unwrap();
    std::fs::remove_file(object_path).unwrap();

    if cfg!(target_os = "macos") {
        let source = std::env::temp_dir().join(format!(
            "mimi_callee_saved_source_{}.mi",
            std::process::id()
        ));
        let executable =
            std::env::temp_dir().join(format!("mimi_callee_saved_codegen_{}", std::process::id()));
        std::fs::write(&source, source_text).unwrap();
        build_file(
            source.to_str().unwrap(),
            executable.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .unwrap();
        let status = std::process::Command::new(&executable).status().unwrap();
        assert_eq!(status.code(), Some(210));
        std::fs::remove_file(source).unwrap();
        std::fs::remove_file(executable).unwrap();
    }
}

#[test]
fn builds_and_runs_a_minimal_host_executable() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_codegen_{}", std::process::id()));
    std::fs::write(&source, "main :: fn () i32 { 7 }").unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(7));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_instance_and_static_methods() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_methods_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_methods_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "pos :: struct { x : i32, get() :: fn (self : Self) i32 { self.x } add() :: fn (self : Self, value : i32) i32 { self.x + value } through_ptr() :: fn (self : *Self) i32 { self.x } offset() :: fn (value : i32) i32 { value + 1 } } main :: fn () i32 { point := pos { x = 4 } pointer := &point point.get() + point.add(3) + pointer.through_ptr() + pos::offset(4) + point.add(point, 1) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(25));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_stack_passed_integer_arguments() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_stack_args_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_stack_args_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "sum10 :: fn (a : i32, b : i32, c : i32, d : i32, e : i32, f : i32, g : i32, h : i32, i : i32, j : i32) i32 { a + b + c + d + e + f + g + h + i + j } main :: fn () i32 { sum10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(55));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_an_internal_function_call() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_call_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_call_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "add :: fn (x : i32, y : i32) i32 { x + y } main :: fn () i32 { add(2, 3) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(5));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_public_functions_from_imported_packages() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("mimi_import_codegen_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let library = directory.join("math.mimi");
    let source = directory.join("main.mimi");
    let output = directory.join("program");
    std::fs::write(
        &library,
        "helper :: fn (left : i32, right : i32) i32 { left + right } pub add :: fn (left : i32, right : i32) i32 { helper(left, right) }",
    )
    .unwrap();
    std::fs::write(
        &source,
        "math :: @import(\"math\") helper :: fn (left : i32, right : i32) i32 { 0 } apply :: fn (function : fn(i32, i32)i32, left : i32, right : i32) i32 { function(left, right) } main :: fn () i32 { apply(math.add, 19, 23) + math.add(1, 1) - 2 }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        output.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&output).status().unwrap();
    assert_eq!(status.code(), Some(42));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn builds_and_runs_nested_imported_packages_with_private_helpers() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("mimi_nested_import_codegen_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let leaf = directory.join("leaf.mimi");
    let bridge = directory.join("bridge.mimi");
    let source = directory.join("main.mimi");
    let output = directory.join("program");
    std::fs::write(
        &leaf,
        "pub Point :: struct { x : i32 } hidden :: fn (value : i32) i32 { value + 7 } pub transform :: fn (point : Point) i32 { hidden(point.x) }",
    )
    .unwrap();
    std::fs::write(
        &bridge,
        "leaf :: @import(\"leaf\") pub forward :: fn (point : leaf.Point) i32 { leaf.transform(point) }",
    )
    .unwrap();
    std::fs::write(
        &source,
        "bridge :: @import(\"bridge\") main :: fn () i32 { point : bridge.leaf.Point = bridge.leaf.Point { x = 35 } bridge.forward(point) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        output.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&output).status().unwrap();
    assert_eq!(status.code(), Some(42));

    std::fs::write(
        &source,
        "bridge :: @import(\"bridge\") main :: fn () i32 { bridge.leaf.hidden(35) }",
    )
    .unwrap();
    assert!(
        build_file(
            source.to_str().unwrap(),
            output.to_str().unwrap(),
            &[],
            &[],
            &[],
        )
        .is_err()
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn lowers_function_values_and_indirect_calls() {
    let ast = parse(tokenize(
        "square :: fn (value : i32) i32 { value * value } apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { apply(square, 6) }"
            .chars(),
    ));
    let mir = mir::lower_hir(&hir::lower_ast(&ast));
    let main = mir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .unwrap();
    assert!(
        main.blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction,
                mir::MirInstruction::LoadFunctionAddress { symbol, .. } if symbol == "_square"
            ))
    );
    assert!(mir.functions.iter().any(|function| {
        function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction, mir::MirInstruction::CallIndirect { .. }))
    }));
}

#[test]
fn builds_and_runs_function_values_and_indirect_calls() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_fn_value_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_fn_value_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "square :: fn (value : i32) i32 { value * value } apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { before := 9 constant :: square function := constant result := apply(function, 5) + function(6) + before + constant(4) result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(86));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_returned_function_values() {
    if !cfg!(target_arch = "x86_64")
        && !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_returned_function_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_returned_function_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "square :: fn (value : i32) i32 { value * value } get_square :: fn () fn(i32)i32 { square } main :: fn () i32 { function := get_square() function(7) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(49));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_function_values_forwarded_through_indirect_calls() {
    if !cfg!(target_arch = "x86_64")
        && !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_forwarded_function_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_forwarded_function_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "square :: fn (x : i32) i32 { x * x } apply :: fn (function : fn(i32)i32, x : i32) i32 { function(x) } forward :: fn (function : fn(fn(i32)i32,i32)i32, value : fn(i32)i32) i32 { function(value, 6) } main :: fn () i32 { forward(apply, square) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(36));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_returned_bound_method_values() {
    if !cfg!(target_arch = "x86_64")
        && !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_bound_method_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_bound_method_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "apply :: fn (function : fn(i32)i32) i32 { function(3) } pos :: struct { x : i32, add() :: fn (self : Self, value : i32) i32 { self.x + value } } make :: fn () fn(i32)i32 { point := pos { x = 7 } point.add } main :: fn () i32 { method := make() apply(method) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(10));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_bound_pointer_receiver_values() {
    if !cfg!(target_arch = "x86_64")
        && !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_bound_pointer_source_{}.mi",
        std::process::id()
    ));
    let executable = std::env::temp_dir().join(format!(
        "mimi_bound_pointer_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "counter :: struct { x : i32, increment() :: fn (self : *Self) i32 { self.x = self.x + 1 self.x } } main :: fn () i32 { value := counter { x = 4 } pointer := &value method := pointer.increment method() + value.x }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(10));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_bound_method_values_stored_in_struct_fields() {
    if !cfg!(target_arch = "x86_64")
        && !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_bound_field_source_{}.mi", std::process::id()));
    let executable = std::env::temp_dir().join(format!(
        "mimi_bound_field_executable_{}",
        std::process::id()
    ));
    std::fs::write(
        &source,
        "pos :: struct { x : i32, add() :: fn (self : Self, value : i32) i32 { self.x + value } } holder :: struct { callback : fn(i32)i32 } main :: fn () i32 { point := pos { x = 7 } stored := holder { callback = point.add } function := stored.callback function(5) + stored.callback(5) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(24));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_static_method_values() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!(
        "mimi_method_value_source_{}.mi",
        std::process::id()
    ));
    let path =
        std::env::temp_dir().join(format!("mimi_method_value_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "pos :: struct { offset() :: fn (value : i32) i32 { value + 1 } } apply :: fn (function : fn(i32)i32, value : i32) i32 { function(value) } main :: fn () i32 { method := pos::offset apply(pos::offset, 5) + method(5) }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(12));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_floating_point_operations_and_abi() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_float_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_float_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "extern sqrt : fn(value : f64)f64 extern sqrtf : fn(value : f32)f32 add64 :: fn (left : f64, right : f64) f64 { left + right } apply64 :: fn (function : fn(f64,f64)f64, left : f64, right : f64) f64 { function(left, right) } add32 :: fn (left : f32, right : f32) f32 { left + right } apply32 :: fn (function : fn(f32,f32)f32, left : f32, right : f32) f32 { function(left, right) } return32 :: fn () f32 { return 1.5 } transform :: fn (value : f64) f64 { ((-value + 4.0) * 2.0) / 2.0 } pos :: struct { x : f32 } main :: fn () i32 { before := 17 result64 := apply64(add64, 1.25, 2.25) result32 := apply32(add32, 1.5, 2) point := pos { x = return32() } point.x = result32 result := if result64 >= 3.5 && result64 < 4.0 && transform(1.0) == 3.0 && point.x == 3.5 && sqrt(4.0) == 2.0 && sqrtf(4.0) == 2.0 && before == 17 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_f16_arithmetic_and_abi() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_f16_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_f16_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "add :: fn (left : f16, right : f16) f16 { left + right } apply :: fn (function : fn(f16, f16)f16, left : f16, right : f16) f16 { function(left, right) } negate :: fn (value : f16) f16 { -value } main :: fn () i32 { sum := apply(add, 1.5, 2.25) difference := 5.5 - 2.25 product := 1.5 * 2.5 quotient := 6.0 / 2.0 negative := negate(1.5) tie : f16 = 1.00048828125 tiny : f16 = 0.000000059604645 result := if sum == 3.75 && difference == 3.25 && product == 3.75 && quotient == 3.0 && negative == -1.5 && tie == 1.0 && tiny != 0.0 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_f8_arithmetic_comparisons_and_abi() {
    if !cfg!(target_arch = "x86_64")
        && !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_f8_source_{}.mi", std::process::id()));
    let executable =
        std::env::temp_dir().join(format!("mimi_f8_executable_{}", std::process::id()));
    std::fs::write(
        &source,
        "add :: fn (left : f8, right : f8) f8 { left + right } apply :: fn (function : fn(f8,f8)f8, left : f8, right : f8) f8 { function(left, right) } main :: fn () i32 { a : f8 = 1.5 b : f8 = 2.25 sum := apply(add, a, b) difference : f8 = 5.5 - 2.25 product : f8 = 1.5 * 2.5 quotient : f8 = 6.0 / 2.0 negative : f8 = -1.5 tie : f8 = 1.0625 tiny : f8 = 0.001953125 max : f8 = 448.0 saturated := add(max, max) result := if sum == 3.75 && difference == 3.25 && product == 3.75 && quotient == 3.0 && negative == -1.5 && tie == 1.0 && tiny != 0.0 && saturated == max { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn builds_and_runs_f8_edge_values_in_compound_storage() {
    if !cfg!(target_arch = "x86_64")
        && !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
    {
        return;
    }
    let source =
        std::env::temp_dir().join(format!("mimi_f8_storage_source_{}.mi", std::process::id()));
    let executable =
        std::env::temp_dir().join(format!("mimi_f8_storage_executable_{}", std::process::id()));
    std::fs::write(
        &source,
        "holder :: struct { value : f8 } main :: fn () i32 { values : [2]f8 = [1.5, 2.25] stored := holder { value = values[1] } result := if values[1] == 2.25 { 0 } else { 1 } result }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        executable.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&executable).status().unwrap();
    assert_eq!(status.code(), Some(0));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(executable).unwrap();
}

#[test]
fn lowers_string_literals_to_data_symbols_in_lir() {
    let ast = parse(tokenize(r#"main :: fn () []u8 { "hello" }"#.chars()));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir(&hir);
    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    assert_eq!(
        lir.strings,
        vec![("_mimi_string_0".into(), b"hello\0".to_vec())]
    );
    assert!(matches!(
        lir.functions[0].blocks[0].instructions.as_slice(),
        [
            lir::LirInstruction::LoadAddress { .. },
            lir::LirInstruction::LoadImmediate { value: 5, .. }
        ]
    ));
}

#[test]
fn builds_and_runs_extended_integer_operations() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_ops_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_ops_codegen_{}", std::process::id()));
    std::fs::write(&source, "main :: fn () i32 { 5 % 2 }").unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(1));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_integer_comparison() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_cmp_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_cmp_codegen_{}", std::process::id()));
    std::fs::write(&source, "main :: fn () bool { 3 < 4 }").unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(1));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_constant_if_else_expression() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_if_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_if_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "main :: fn () i32 { value := if true { 7 } else { 3 } value }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(7));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_functions_with_locals_assignment_and_return() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let source = std::env::temp_dir().join(format!("mimi_local_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_local_codegen_{}", std::process::id()));
    std::fs::write(
        &source,
        "main :: fn () i32 { value := 2 value = value + 3 return value }",
    )
    .unwrap();
    build_file(
        source.to_str().unwrap(),
        path.to_str().unwrap(),
        &[],
        &[],
        &[],
    )
    .unwrap();
    let status = std::process::Command::new(&path).status().unwrap();
    assert_eq!(status.code(), Some(5));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn builds_and_runs_external_print_with_string_literal() {
    if !cfg!(target_os = "macos") || !cfg!(target_arch = "aarch64") {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::process::Command::new("make")
        .current_dir(root.join("runtime"))
        .status()
        .unwrap();
    let source = std::env::temp_dir().join(format!("mimi_print_source_{}.mi", std::process::id()));
    let path = std::env::temp_dir().join(format!("mimi_print_codegen_{}", std::process::id()));
    for (program, expected) in [
        (
            r#"extern print : fn(str : []u8)void main :: fn () void { defer { print("a") } defer { print("b") } print("c") }"#,
            "cba",
        ),
        (
            r#"extern print : fn(str : []u8)void main :: fn () void { defer print("b") print("a") }"#,
            "ab",
        ),
        (
            r#"extern print : fn(str : []u8)void main :: fn () void { defer { print("b") } print("a") return }"#,
            "ab",
        ),
        (
            r#"extern print : fn(str : []u8)void main :: fn () void { defer { print("a") } { defer { print("b") } print("c") } print("d") }"#,
            "cbda",
        ),
        (
            r#"extern print : fn(str : []u8)void main :: fn () void { defer { print("a") } { defer { print("b") } return } }"#,
            "ba",
        ),
    ] {
        std::fs::write(&source, program).unwrap();
        let result = build_file(
            source.to_str().unwrap(),
            path.to_str().unwrap(),
            &[],
            &[root.join("runtime")],
            &["print".into()],
        );
        assert!(result.is_ok(), "{result:?}");
        let output = std::process::Command::new(&path).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(path).unwrap();
    std::process::Command::new("make")
        .current_dir(root.join("runtime"))
        .arg("clean")
        .status()
        .unwrap();
}
