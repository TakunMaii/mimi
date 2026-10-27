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
