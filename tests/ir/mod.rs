use super::{build_file, codegen, hir, lexer::tokenize, lir, mir, parser::parse};

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
    assert!(
        lir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, lir::LirInstruction::Add { .. }))
    );
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
    assert_eq!(status.code(), Some(20));
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
        "main :: fn () i32 { values := [1, 2, 3, 4] start := 2 sum := 0 for item in values[1:3] { sum = sum + item } for item in values[start:] { sum = sum + item } sum }",
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
fn lowers_string_literals_to_data_symbols_in_lir() {
    let ast = parse(tokenize(r#"main :: fn () []u8 { "hello" }"#.chars()));
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir(&hir);
    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    assert_eq!(
        lir.strings,
        vec![("_mimi_string_0".into(), b"hello".to_vec())]
    );
    assert!(matches!(
        lir.functions[0].blocks[0].instructions.as_slice(),
        [lir::LirInstruction::LoadAddress { .. }]
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
