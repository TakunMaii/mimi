use super::{
    build_file, build_file_for_target, codegen, hir, lexer::tokenize, lir, mir, parser::parse,
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

    if cfg!(target_arch = "x86_64") {
        let status = std::process::Command::new(&executable).status().unwrap();
        assert_eq!(status.code(), Some(41));
    } else if cfg!(target_os = "macos") {
        let description = std::process::Command::new("file")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(description.status.success());
        assert!(String::from_utf8_lossy(&description.stdout).contains("x86_64"));
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
