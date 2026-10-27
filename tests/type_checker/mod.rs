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
