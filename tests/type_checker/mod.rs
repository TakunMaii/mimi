use super::{
    lexer::tokenize,
    parser::{DataType, parse},
    type_checker::check,
};

#[test]
fn infers_numeric_literal_types_by_value() {
    let ast = parse(tokenize(
        "small := 127 wider := 2147483648 unsigned := 9223372036854775808 decimal := 1.25 negative := -2147483649"
            .chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["small"], DataType::I32);
    assert_eq!(types["wider"], DataType::I64);
    assert_eq!(types["unsigned"], DataType::U64);
    assert_eq!(types["decimal"], DataType::F64);
    assert_eq!(types["negative"], DataType::I64);
}

#[test]
fn checks_integer_range_loop_bounds() {
    let valid = parse(tokenize(
        "main :: fn () i32 { total := 0 for index in 0..5 { total = total + index } total }"
            .chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize(
        "main :: fn (start : u32) void { for index in start..5 { index } }".chars(),
    ));
    assert!(check(&invalid).is_err());
}

#[test]
fn specializes_generic_struct_fields_and_checks_field_types() {
    let valid = parse(tokenize(
        "pair :: struct <T> { x : T, y : T } value : pair<i32> = pair<i32> { x = 1, y = 2 } get_x :: fn (item : pair<i32>) i32 { item.x }".chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize(
        "pair :: struct <T> { x : T, y : T } value : pair<i32> = pair<i32> { x = 1, y = true }"
            .chars(),
    ));
    let errors = check(&invalid).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("expected i32, found bool"))
    );
}

#[test]
fn checks_tuple_inference_signatures_and_member_bounds() {
    let valid = parse(tokenize(
        "make :: fn (value : i32) (i32, bool) { (value, true) } first :: fn (pair : (i32, bool)) i32 { pair.0 } main :: fn () i32 { pair := make(7) first(pair) }".chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize(
        "main :: fn () i32 { pair := (7, true) pair.2 }".chars(),
    ));
    let errors = check(&invalid).unwrap_err();
    assert!(
        errors[0]
            .message
            .contains("tuple index '2' is out of bounds")
    );

    let immutable = parse(tokenize(
        "main :: fn () i32 { pair :: (7, true) pair.0 = 9 0 }".chars(),
    ));
    let errors = check(&immutable).unwrap_err();
    assert!(
        errors[0]
            .message
            .contains("cannot assign through constant tuple")
    );
}

#[test]
fn checks_tuple_destructuring_types_and_arity() {
    let valid = parse(tokenize(
        "main :: fn () i32 { first, second := (7, true) result := if second { first } else { 0 } result }".chars(),
    ));
    check(&valid).unwrap();

    for (program, expected) in [
        (
            "main :: fn () i32 { first, second := (7,) 0 }",
            "has 2 names, but tuple has 1 elements",
        ),
        (
            "main :: fn () i32 { first, second := 7 0 }",
            "requires a tuple value",
        ),
        (
            "main :: fn () i32 { first, first := (1, 2) 0 }",
            "duplicate names",
        ),
    ] {
        let errors = check(&parse(tokenize(program.chars()))).unwrap_err();
        assert!(
            errors.iter().any(|error| error.message.contains(expected)),
            "{errors:?}"
        );
    }
}

#[test]
fn specializes_compile_time_generic_defaults_and_checks_argument_kind() {
    let valid = parse(tokenize(
        "fixed :: struct <dx> { x : f32 = dx } value : fixed<3.5> = fixed<3.5> {}".chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize(
        "fixed :: struct <dx> { x : f32 = dx } value : fixed<f32> = fixed<f32> {}".chars(),
    ));
    let errors = check(&invalid).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("requires a value"))
    );
}

#[test]
fn specializes_generic_enum_payload_types() {
    let valid = parse(tokenize(
        "option :: enum <T> { some(T), none } read :: fn (value : option<i32>) i32 { match value { some(number) => number, none => 0 } } value : option<i32> = some(42)".chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize(
        "option :: enum <T> { some(T), none } value : option<i32> = some(true)".chars(),
    ));
    let errors = check(&invalid).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("expected i32, found bool"))
    );
}

#[test]
fn keeps_mimi_enums_nominal() {
    let invalid = parse(tokenize(
        "color :: enum { red, green } value : color = 1".chars(),
    ));
    assert!(check(&invalid).is_err());
}

#[test]
fn rejects_missing_and_invalid_generic_arguments() {
    for (program, expected) in [
        (
            "pair :: struct <T> { x : T } value : pair = pair { x = 1 }",
            "requires 1 arguments",
        ),
        (
            "pair :: struct <T> { x : T } value : pair<i32, i32> = pair<i32, i32> { x = 1 }",
            "expects 1 generic arguments, got 2",
        ),
        (
            "pair :: struct <T> { x : T } value : pair<Unknown> = pair<Unknown> { x = 1 }",
            "unknown type argument 'Unknown'",
        ),
        (
            "fixed :: struct <dx> { x : f32 = dx } value : fixed<true> = fixed<true> {}",
            "expected f32, found bool",
        ),
    ] {
        let ast = parse(tokenize(program.chars()));
        let errors = check(&ast).unwrap_err();
        assert!(
            errors.iter().any(|error| error.message.contains(expected)),
            "expected diagnostic containing '{expected}', got {errors:?}"
        );
    }
}

#[test]
fn uses_declared_types_as_literal_context() {
    let ast = parse(tokenize(
        "byte : u8 = 255 half : f16 = 1.5 widened : f32 = 2 flag : bool = true".chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["byte"], DataType::U8);
    assert_eq!(types["half"], DataType::F16);
    assert_eq!(types["widened"], DataType::F32);
    assert_eq!(types["flag"], DataType::Bool);
}

#[test]
fn infers_arithmetic_and_boolean_expression_types() {
    let ast = parse(tokenize(
        "count := 2 + 3.0 valid := count >= 5 && !false".chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["count"], DataType::F64);
    assert_eq!(types["valid"], DataType::Bool);
}

#[test]
fn rejects_out_of_range_and_incompatible_literals() {
    let ast = parse(tokenize("small : u8 = 256 invalid := 1 && true".chars()));

    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].message.contains("out of range for u8"));
    assert!(errors[1].message.contains("bool"));
}

#[test]
fn checks_the_result_type_of_comparison_expressions() {
    let ast = parse(tokenize("wrong : i32 = true == false".chars()));

    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("expected i32, found bool"));
}

#[test]
fn checks_assignment_types_and_constant_mutability() {
    let ast = parse(tokenize(
        "mutable : u8 = 2 mutable = 3 fixed :: 4 fixed = 5 missing = 1".chars(),
    ));

    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].message.contains("constant 'fixed'"));
    assert!(errors[1].message.contains("undefined variable 'missing'"));
}

#[test]
fn rejects_undefined_identifiers_and_duplicate_definitions() {
    let ast = parse(tokenize(
        "first := absent second := true second := false".chars(),
    ));

    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].message.contains("undefined identifier 'absent'"));
    assert!(errors[1].message.contains("'second' is already defined"));
}

#[test]
fn block_value_comes_from_its_final_item() {
    let ast = parse(tokenize(
        "value := { local := 2 local + 3 } unit := { local := true } empty := {}".chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["value"], DataType::I32);
    assert_eq!(types["unit"], DataType::Void);
    assert_eq!(types["empty"], DataType::Void);
    assert!(!types.contains_key("local"));
}

#[test]
fn block_scope_can_read_and_shadow_outer_names() {
    let ast = parse(tokenize(
        "outer := 5 shadow := { outer := true outer } after := outer".chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["outer"], DataType::I32);
    assert_eq!(types["shadow"], DataType::Bool);
    assert_eq!(types["after"], DataType::I32);
    assert!(!types.contains_key("local"));
}

#[test]
fn block_assignments_can_update_outer_variables() {
    let ast = parse(tokenize("outer := 1 result := { outer = 2 outer }".chars()));

    let types = check(&ast).unwrap();
    assert_eq!(types["outer"], DataType::I32);
    assert_eq!(types["result"], DataType::I32);
}

#[test]
fn supports_explicit_void_values() {
    let ast = parse(tokenize("unit : void = {}".chars()));

    let types = check(&ast).unwrap();
    assert_eq!(types["unit"], DataType::Void);
}

#[test]
fn checks_function_definitions_and_calls() {
    let ast = parse(tokenize(
        "add :: fn (x : i32, y : i32) i32 { x + y } result := add(3, 2)".chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["result"], DataType::I32);
}

#[test]
fn checks_explicit_return_and_function_errors() {
    let ast = parse(tokenize(
        "id :: fn (x : i32) i32 { return x } bad := id(true) missing := nope()".chars(),
    ));

    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].message.contains("expected i32, found bool"));
    assert!(errors[1].message.contains("undefined function 'nope'"));
}

#[test]
fn supports_void_functions_with_return() {
    let ast = parse(tokenize(
        "noop :: fn () void { return } result := noop()".chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["result"], DataType::Void);
}

#[test]
fn infers_and_checks_struct_literals_and_members() {
    let ast = parse(tokenize(
        "pos :: struct { x : f32, y : f32, } make :: fn (p : pos) f32 { p.x } point := pos { x = 1, y = 2 } copy : pos = { x = 3, y = 4 } value := make({ x = 5, y = 6 })"
            .chars(),
    ));

    let types = check(&ast).unwrap();
    assert_eq!(types["point"], DataType::Struct("pos"));
    assert_eq!(types["copy"], DataType::Struct("pos"));
    assert_eq!(types["value"], DataType::F32);
}

#[test]
fn rejects_invalid_struct_fields_and_ambiguous_struct_literals() {
    let ast = parse(tokenize(
        "pos :: struct { x : i32, y : bool } bad := pos { x = true, z = 2 } unknown := { x = 1, y = false }"
            .chars(),
    ));

    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(
        errors[0].message.contains("expected i32, found bool")
            || errors[0].message.contains("unknown field 'z'")
    );
    assert!(
        errors[1]
            .message
            .contains("cannot infer anonymous struct type")
    );
}

#[test]
fn checks_control_flow_conditions() {
    let ast = parse(tokenize(
        "values := [1, 2] if true { value := 1 } while false { defer { loop {} } } for item in values { item }"
            .chars(),
    ));
    check(&ast).unwrap();
}

#[test]
fn checks_do_while_conditions_as_booleans() {
    let valid = parse(tokenize("do { value := 1 } while true".chars()));
    check(&valid).unwrap();

    let invalid = parse(tokenize("do {} while 1".chars()));
    let errors = check(&invalid).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("bool"));
}

#[test]
fn checks_break_and_continue_are_inside_loops() {
    let valid = parse(tokenize(
        "main :: fn () i32 { while true { continue } loop { break } 0 }".chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize("break continue".chars()));
    let errors = check(&invalid).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(
        errors[0]
            .message
            .contains("break is only valid inside a loop")
    );
    assert!(
        errors[1]
            .message
            .contains("continue is only valid inside a loop")
    );
}

#[test]
fn infers_value_from_if_else_expression() {
    let ast = parse(tokenize(
        "value := if true { 1 } else { 2 } flag := if false { true } else { false }".chars(),
    ));
    let types = check(&ast).unwrap();
    assert_eq!(types["value"], DataType::I32);
    assert_eq!(types["flag"], DataType::Bool);
}

#[test]
fn rejects_if_else_expressions_with_incompatible_branches() {
    let ast = parse(tokenize("value := if true { 1 } else { false }".chars()));
    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("bool") || errors[0].message.contains("incompatible"));
}

#[test]
fn imports_public_functions_through_package_namespaces() {
    let path = std::env::temp_dir().join(format!("mimi_import_{}.mimi", std::process::id()));
    std::fs::write(
        &path,
        "pub add :: fn (left : i32, right : i32) i32 { left + right } hidden :: fn () i32 { 0 }",
    )
    .unwrap();
    let source = format!(
        "std :: @import(\"{}\") result := std.add(1, 2)",
        path.display()
    );
    let ast = parse(tokenize(source.chars()));
    let types = check(&ast).unwrap();
    assert_eq!(types["result"], DataType::I32);

    let source = format!(
        "std :: @import(\"{}\") apply :: fn (function : fn(i32, i32)i32, left : i32, right : i32) i32 {{ function(left, right) }} main :: fn () i32 {{ apply(std.add, 1, 2) }}",
        path.display()
    );
    let ast = parse(tokenize(source.chars()));
    check(&ast).unwrap();

    let source = format!(
        "std :: @import(\"{}\") main :: fn () i32 {{ std.hidden() }}",
        path.display()
    );
    let ast = parse(tokenize(source.chars()));
    assert!(check(&ast).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn reports_cyclic_package_imports() {
    let directory = std::env::temp_dir().join(format!("mimi_import_cycle_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let first = directory.join("first.mimi");
    let second = directory.join("second.mimi");
    std::fs::write(
        &first,
        format!("second :: @import(\"{}\")", second.display()),
    )
    .unwrap();
    std::fs::write(
        &second,
        format!("first :: @import(\"{}\")", first.display()),
    )
    .unwrap();
    let source = format!("first :: @import(\"{}\")", first.display());
    let errors = check(&parse(tokenize(source.chars()))).unwrap_err();

    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("cyclic import"))
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn treats_string_literals_as_byte_slices_for_extern_calls() {
    let ast = parse(tokenize(
        "extern print : fn(str : []u8)void main :: fn () void { print(\"hello\") }".chars(),
    ));
    check(&ast).unwrap();
}

#[test]
fn imports_public_external_functions_but_hides_private_ones() {
    let path = std::env::temp_dir().join(format!("mimi_import_extern_{}.mimi", std::process::id()));
    std::fs::write(
        &path,
        "pub extern abs : fn(value : i32)i32 extern hidden : fn(value : i32)i32",
    )
    .unwrap();
    let source = format!(
        "system :: @import(\"{}\") result := system.abs(-3)",
        path.display()
    );
    check(&parse(tokenize(source.chars()))).unwrap();

    let source = format!(
        "system :: @import(\"{}\") result := system.hidden(-3)",
        path.display()
    );
    assert!(check(&parse(tokenize(source.chars()))).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn imports_public_function_value_constants_but_hides_their_targets() {
    let path = std::env::temp_dir().join(format!("mimi_import_alias_{}.mimi", std::process::id()));
    std::fs::write(
        &path,
        "extern print : fn(value : []u8)void pub printl :: print",
    )
    .unwrap();
    let source = format!(
        "lib :: @import(\"{}\") main :: fn () void {{ lib.printl(\"hello\") }}",
        path.display()
    );
    check(&parse(tokenize(source.chars()))).unwrap();

    let source = format!(
        "lib :: @import(\"{}\") main :: fn () void {{ lib.print(\"hello\") }}",
        path.display()
    );
    assert!(check(&parse(tokenize(source.chars()))).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn imports_public_scalar_constants_with_their_types() {
    let path =
        std::env::temp_dir().join(format!("mimi_import_constants_{}.mimi", std::process::id()));
    std::fs::write(
        &path,
        "BASE :: 40 pub answer :: BASE + 2 pub ratio :: 1.0 + 0.25 pub enabled :: answer > 40 pub large :: 2147483648 hidden :: 9 + 3",
    )
    .unwrap();
    let source = format!(
        "values :: @import(\"{}\") get_ratio :: fn () f64 {{ values.ratio }} get_answer :: fn () i32 {{ values.answer }} is_enabled :: fn () bool {{ values.enabled }}",
        path.display()
    );
    let types = check(&parse(tokenize(source.chars()))).unwrap();
    assert_eq!(types["values.answer"], DataType::I32);
    assert_eq!(types["values.ratio"], DataType::F64);
    assert_eq!(types["values.enabled"], DataType::Bool);
    assert_eq!(types["values.large"], DataType::I64);

    let source = format!(
        "values :: @import(\"{}\") main :: fn () i32 {{ values.hidden }}",
        path.display()
    );
    assert!(check(&parse(tokenize(source.chars()))).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn imports_public_structs_and_enums_with_package_qualified_types() {
    let path = std::env::temp_dir().join(format!("mimi_import_types_{}.mimi", std::process::id()));
    std::fs::write(
        &path,
        "pub Point :: struct { x : i32, y : i32 } pub Choice :: enum { first, second(i32) } Secret :: struct { value : i32 } pub get_x :: fn(point : Point)i32 { point.x }",
    )
    .unwrap();
    let source = format!(
        "geom :: @import(\"{}\") point : geom.Point = geom.Point {{ x=19, y=23 }} choice : geom.Choice = second(7) result := geom.get_x(point)",
        path.display()
    );
    let types = check(&parse(tokenize(source.chars()))).unwrap();
    assert_eq!(types["point"], DataType::Struct("geom.Point"));
    assert_eq!(types["choice"], DataType::Struct("geom.Choice"));
    assert_eq!(types["result"], DataType::I32);

    let source = format!(
        "geom :: @import(\"{}\") point : geom.Secret = geom.Secret {{ value=1 }}",
        path.display()
    );
    assert!(check(&parse(tokenize(source.chars()))).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn imports_public_aggregate_constants_and_hides_private_types() {
    let path = std::env::temp_dir().join(format!(
        "mimi_import_aggregates_{}.mimi",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "pub Point :: struct { x : i32, y : i32 } pub origin :: Point { x=33, y=0 } pub numbers : [2]i32 : [2, 3] pub Choice :: enum { none, some(i32) } pub choice :: some(7) pub message :: \"A\" Secret :: struct { value : i32 } pub hidden :: Secret { value=1 }",
    )
    .unwrap();
    let source = format!(
        "geo :: @import(\"{}\") main :: fn () i32 {{ geo.origin.x + geo.numbers[0] + match geo.choice {{ some(value) => value, none => 0 }} }}",
        path.display()
    );
    let types = check(&parse(tokenize(source.chars()))).unwrap();
    assert_eq!(types["geo.origin"], DataType::Struct("geo.Point"));
    assert_eq!(
        types["geo.numbers"],
        DataType::Array(2, Box::leak(Box::new(DataType::I32)))
    );
    assert_eq!(types["geo.choice"], DataType::Enum("geo.Choice"));
    assert_eq!(
        types["geo.message"],
        DataType::Slice(Box::leak(Box::new(DataType::U8)))
    );

    let source = format!(
        "geo :: @import(\"{}\") result := geo.hidden.value",
        path.display()
    );
    assert!(check(&parse(tokenize(source.chars()))).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn rejects_slice_return_types_for_external_functions() {
    let ast = parse(tokenize("extern get_text : fn()[]u8".chars()));
    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("cannot return a slice"));
}

#[test]
fn rejects_string_literals_when_a_non_byte_slice_is_expected() {
    let ast = parse(tokenize(
        "extern print : fn(value : i32)void main :: fn () void { print(\"hello\") }".chars(),
    ));
    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("[]u8"));
}

#[test]
fn resolves_imports_from_search_paths() {
    let directory = std::env::temp_dir().join(format!("mimi_import_path_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("math.mimi"),
        "pub add :: fn (a : i32, b : i32) i32 { a + b }",
    )
    .unwrap();
    let ast = parse(tokenize(
        r#"math :: @import("math") result := math.add(2, 3)"#.chars(),
    ));
    let types = super::type_checker::check_with_import_paths(&ast, &[directory.clone()]).unwrap();
    assert_eq!(types["result"], DataType::I32);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rejects_non_boolean_if_and_while_conditions() {
    let ast = parse(tokenize("if 1 {} while 2 {}".chars()));
    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors.iter().all(|error| error.message.contains("bool")));
}

#[test]
fn checks_arrays_indexing_and_slices() {
    let ast = parse(tokenize(
        "values : [3]i32 = [1, 2, 3] first := values[0] part : []i32 = values[1:3]".chars(),
    ));
    let types = check(&ast).unwrap();
    assert_eq!(types["first"], DataType::I32);
    assert!(matches!(types["part"], DataType::Slice(element) if *element == DataType::I32));
}

#[test]
fn checks_fixed_array_element_assignment() {
    let valid = parse(tokenize(
        "main :: fn () i32 { values := [1, 2] values[1] = 7 values[0] + values[1] }".chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize(
        "values : [2]i32 : [1, 2] values[0] = true".chars(),
    ));
    let errors = check(&invalid).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("constant 'values'"));

    let invalid_value = parse(tokenize(
        "main :: fn () void { values := [1] values[0] = true }".chars(),
    ));
    let errors = check(&invalid_value).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("expected i32, found bool"));

    let nested_member = parse(tokenize(
        "matrix :: struct { cells : [2][2]i32 } main :: fn () i32 { value := matrix { cells = [[1, 2], [3, 4]] } value.cells[1][0] = 5 value.cells[1][0] }".chars(),
    ));
    check(&nested_member).unwrap();

    let fixed_member = parse(tokenize(
        "matrix :: struct { cells : [2]i32 : [1, 2] } main :: fn () i32 { value := matrix {} value.cells[0] = 5 0 }".chars(),
    ));
    let errors = check(&fixed_member).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].message.contains("fixed field 'cells'"),
        "{errors:?}"
    );
}

#[test]
fn checks_address_of_dereference_and_pointer_assignment() {
    let valid = parse(tokenize(
        "get :: fn (pointer : *i32) i32 { *pointer } set :: fn (pointer : *i32) void { *pointer = 9 } main :: fn () i32 { value := 1 get(&value) }".chars(),
    ));
    check(&valid).unwrap();

    let invalid_value = parse(tokenize(
        "set :: fn (pointer : *i32) void { *pointer = true }".chars(),
    ));
    let errors = check(&invalid_value).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("expected i32, found bool"));

    let invalid_address = parse(tokenize("pointer := &1".chars()));
    let errors = check(&invalid_address).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .message
            .contains("address-of requires a mutable variable")
    );
}

#[test]
fn checks_pointer_indexing_and_assignment() {
    let ast = parse(tokenize(
        "read :: fn (pointer : *i32) i32 { pointer[1] } write :: fn (pointer : *i32) void { pointer[2] = 9 } main :: fn () i32 { values := [1, 2, 3] write(&values[0]) read(&values[0]) }".chars(),
    ));
    check(&ast).unwrap();
}

#[test]
fn checks_pointer_difference_for_matching_scalar_types() {
    let valid = parse(tokenize(
        "distance :: fn (left : *i32, right : *i32) i64 { right - left }".chars(),
    ));
    check(&valid).unwrap();

    let mismatched = parse(tokenize(
        "distance :: fn (left : *i32, right : *u8) i64 { right - left }".chars(),
    ));
    assert!(check(&mismatched).is_err());
}

#[test]
fn checks_pointer_arithmetic_on_compound_elements() {
    let ast = parse(tokenize(
        "pos :: struct { x : i32, y : i32 } result :: enum { some(i32), none } shift_pos :: fn (pointer : *pos) *pos { pointer + 1 } distance_pos :: fn (left : *pos, right : *pos) i64 { right - left } shift_result :: fn (pointer : *result) *result { pointer + 1 } distance_result :: fn (left : *result, right : *result) i64 { right - left } main :: fn () void {}".chars(),
    ));
    check(&ast).unwrap();
}

#[test]
fn checks_struct_field_assignment_and_fixed_fields() {
    let valid = parse(tokenize(
        "pos :: struct { x : i32 } main :: fn () i32 { point := pos { x = 1 } pointer := &point pointer.x = 8 point.x }".chars(),
    ));
    check(&valid).unwrap();

    let invalid = parse(tokenize(
        "pos :: struct { fixed : i32 : 3 } main :: fn () i32 { point := pos {} point.fixed = 8 0 }"
            .chars(),
    ));
    let errors = check(&invalid).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("fixed field 'fixed'"));
}

#[test]
fn checks_enum_construction_if_let_and_match() {
    let ast = parse(tokenize("result :: enum { some(f32), error, none, } value := some(1.0) selected := match value { some(x) => x, error => 0.0, none => 0.0 }".chars()));
    let types = check(&ast).unwrap();
    assert_eq!(types["selected"], DataType::F32);
}

#[test]
fn struct_literals_may_omit_fields_with_defaults() {
    let ast = parse(tokenize(
        "point :: struct { fixed: f32 : 32, mutable: f32 = 3.0, required: i32 } value := point { required = 7 }"
            .chars(),
    ));
    let types = check(&ast).unwrap();
    assert_eq!(types["value"], DataType::Struct("point"));
}

#[test]
fn struct_literals_require_fields_without_defaults() {
    let ast = parse(tokenize(
        "point :: struct { fixed: f32 : 32, required: i32 } value := point {}".chars(),
    ));
    let errors = check(&ast).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .message
            .contains("missing required field 'required'")
    );
}

#[test]
fn compile_time_field_defaults_must_be_constant() {
    let ast = parse(tokenize(
        "point :: struct { base: f32 = 2.0, fixed: f32 : base } value := point {}".chars(),
    ));
    let errors = check(&ast).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("must be constant"))
    );
}

#[test]
fn checks_struct_method_self_and_method_access() {
    let ast = parse(tokenize(
        "pos :: struct { x : i32, getX() :: fn (self : Self) i32 { self.x } } point : pos = pos { x = 4 } value := point.getX()"
            .chars(),
    ));
    let types = check(&ast).unwrap();
    assert_eq!(types["value"], DataType::I32);
}

#[test]
fn checks_function_values_and_static_methods() {
    let ast = parse(tokenize(
        "apply :: fn (f : fn(i32)i32, value : i32) i32 { f(value) } pos :: struct { x : i32, getX() :: fn (self : Self) i32 { self.x } } point : pos = pos { x = 1 } result := apply(fn(x : i32) i32 { x + 1 }, 2)"
            .chars(),
    ));
    let types = check(&ast).unwrap();
    assert_eq!(types["result"], DataType::I32);
}

#[test]
fn type_checks_lambdas_that_capture_local_variables() {
    let ast = parse(tokenize(
        "main :: fn () i32 { outer := 5 add_outer := fn (value : i32) i32 { outer + value } assigned := fn (value : i32) i32 { outer = value outer } shadowed := fn (outer : i32) i32 { outer + 1 } add_outer(2) + assigned(8) + shadowed(3) }"
            .chars(),
    ));
    check(&ast).unwrap();
}

#[test]
fn lambda_capture_lookup_respects_lexical_scope_and_undefined_names() {
    let ast = parse(tokenize(
        "main :: fn () i32 { value := 7 result := { inner := fn () i32 { value } inner() } result }"
            .chars(),
    ));
    check(&ast).unwrap();

    let invalid = parse(tokenize(
        "main :: fn () i32 { missing := fn () i32 { absent } missing() }".chars(),
    ));
    let errors = check(&invalid).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("undefined identifier 'absent'"));
}

#[test]
fn supports_pointer_member_access_and_automatic_method_receivers() {
    let ast = parse(tokenize(
        "pos :: struct { x : i32, value() :: fn (self : Self) i32 { self.x } through_ptr() :: fn (self : *Self) i32 { self.x } } point : pos = pos { x = 4 } pointer := &point from_value := point.value() from_pointer := pointer.value() through_value := point.through_ptr() through_pointer := pointer.through_ptr() field := pointer.x"
            .chars(),
    ));
    let types = check(&ast).unwrap();
    assert_eq!(types["from_value"], DataType::I32);
    assert_eq!(types["from_pointer"], DataType::I32);
    assert_eq!(types["through_value"], DataType::I32);
    assert_eq!(types["through_pointer"], DataType::I32);
    assert_eq!(types["field"], DataType::I32);
}

#[test]
fn allows_explicit_method_receivers() {
    let ast = parse(tokenize(
        "pos :: struct { x : i32, value() :: fn (self : Self) i32 { self.x } through_ptr() :: fn (self : *Self) i32 { self.x } } point : pos = pos { x = 4 } value := point.value(point) pointer := &point through := pointer.through_ptr(&point)"
            .chars(),
    ));
    let types = check(&ast).unwrap();
    assert_eq!(types["value"], DataType::I32);
    assert_eq!(types["through"], DataType::I32);
}
