use super::{
    lexer::{Token, tokenize},
    parser::{ASTNode, DataType, Expression, parse},
};

#[test]
fn tokenizes_arithmetic_operators_and_parentheses() {
    assert_eq!(
        tokenize("value := -(a + 2) * 3 / 4 % 2".chars()),
        vec![
            Token::Identifier("value".into()),
            Token::Colon,
            Token::Equal,
            Token::Minus,
            Token::LeftParen,
            Token::Identifier("a".into()),
            Token::Plus,
            Token::LiteralNumber("2".into()),
            Token::RightParen,
            Token::Star,
            Token::LiteralNumber("3".into()),
            Token::Slash,
            Token::LiteralNumber("4".into()),
            Token::Percent,
            Token::LiteralNumber("2".into()),
        ]
    );
}

#[test]
fn parses_arithmetic_precedence_and_parentheses() {
    assert_eq!(
        parse(tokenize("result := 1 + 2 * (3 - 4)".chars())),
        vec![ASTNode::VariableDefinition {
            name: "result".into(),
            data_type: None,
            value: Expression::Add(
                Box::new(Expression::LiteralNumber("1".into())),
                Box::new(Expression::Multiply(
                    Box::new(Expression::LiteralNumber("2".into())),
                    Box::new(Expression::Subtract(
                        Box::new(Expression::LiteralNumber("3".into())),
                        Box::new(Expression::LiteralNumber("4".into())),
                    )),
                )),
            ),
        }]
    );
}

#[test]
fn parses_all_statement_kinds_with_expression_values() {
    assert_eq!(
        parse(tokenize("x := a + 1 y = x * 2 z :: (y - 3) % 2".chars())),
        vec![
            ASTNode::VariableDefinition {
                name: "x".into(),
                data_type: None,
                value: Expression::Add(
                    Box::new(Expression::Identifier("a".into())),
                    Box::new(Expression::LiteralNumber("1".into())),
                ),
            },
            ASTNode::Assignment {
                name: "y".into(),
                value: Expression::Multiply(
                    Box::new(Expression::Identifier("x".into())),
                    Box::new(Expression::LiteralNumber("2".into())),
                ),
            },
            ASTNode::CompileTimeConstant {
                name: "z".into(),
                data_type: None,
                value: Expression::Remainder(
                    Box::new(Expression::Subtract(
                        Box::new(Expression::Identifier("y".into())),
                        Box::new(Expression::LiteralNumber("3".into())),
                    )),
                    Box::new(Expression::LiteralNumber("2".into())),
                ),
            },
        ]
    );
}

#[test]
fn parses_unary_negation() {
    assert_eq!(
        parse(tokenize("x := -value".chars())),
        vec![ASTNode::VariableDefinition {
            name: "x".into(),
            data_type: None,
            value: Expression::Negate(Box::new(Expression::Identifier("value".into()))),
        }]
    );
}

#[test]
fn tokenizes_boolean_literals() {
    assert_eq!(
        tokenize("true false".chars()),
        vec![Token::LiteralBool(true), Token::LiteralBool(false)]
    );
}

#[test]
fn parses_explicit_and_inferred_types() {
    let type_cases = [
        ("u8", DataType::U8),
        ("u16", DataType::U16),
        ("u32", DataType::U32),
        ("u64", DataType::U64),
        ("i8", DataType::I8),
        ("i16", DataType::I16),
        ("i32", DataType::I32),
        ("i64", DataType::I64),
        ("f8", DataType::F8),
        ("f16", DataType::F16),
        ("f32", DataType::F32),
        ("f64", DataType::F64),
        ("bool", DataType::Bool),
    ];

    for (type_name, data_type) in type_cases {
        assert_eq!(
            parse(tokenize(format!("value : {type_name} = 1").chars())),
            vec![ASTNode::VariableDefinition {
                name: "value".into(),
                data_type: Some(data_type),
                value: Expression::LiteralNumber("1".into()),
            }]
        );
    }

    assert_eq!(
        parse(tokenize("flag : bool : true inferred := false".chars())),
        vec![
            ASTNode::CompileTimeConstant {
                name: "flag".into(),
                data_type: Some(DataType::Bool),
                value: Expression::LiteralBool(true),
            },
            ASTNode::VariableDefinition {
                name: "inferred".into(),
                data_type: None,
                value: Expression::LiteralBool(false),
            },
        ]
    );
}

#[test]
fn tokenizes_comparison_and_logical_operators() {
    assert_eq!(
        tokenize("a == b != c < d <= e > f >= g && h || !i".chars()),
        vec![
            Token::Identifier("a".into()),
            Token::EqualEqual,
            Token::Identifier("b".into()),
            Token::NotEqual,
            Token::Identifier("c".into()),
            Token::LessThan,
            Token::Identifier("d".into()),
            Token::LessThanOrEqual,
            Token::Identifier("e".into()),
            Token::GreaterThan,
            Token::Identifier("f".into()),
            Token::GreaterThanOrEqual,
            Token::Identifier("g".into()),
            Token::AndAnd,
            Token::Identifier("h".into()),
            Token::OrOr,
            Token::Bang,
            Token::Identifier("i".into()),
        ]
    );
}

#[test]
fn parses_comparison_and_logical_precedence() {
    assert_eq!(
        parse(tokenize(
            "result := a + 2 * 3 >= b && !false || c == d".chars()
        )),
        vec![ASTNode::VariableDefinition {
            name: "result".into(),
            data_type: None,
            value: Expression::Or(
                Box::new(Expression::And(
                    Box::new(Expression::GreaterThanOrEqual(
                        Box::new(Expression::Add(
                            Box::new(Expression::Identifier("a".into())),
                            Box::new(Expression::Multiply(
                                Box::new(Expression::LiteralNumber("2".into())),
                                Box::new(Expression::LiteralNumber("3".into())),
                            )),
                        )),
                        Box::new(Expression::Identifier("b".into())),
                    )),
                    Box::new(Expression::Not(Box::new(Expression::LiteralBool(false)))),
                )),
                Box::new(Expression::Equal(
                    Box::new(Expression::Identifier("c".into())),
                    Box::new(Expression::Identifier("d".into())),
                )),
            ),
        }]
    );
}

#[test]
fn single_equal_remains_assignment() {
    assert_eq!(
        parse(tokenize("value = left == right".chars())),
        vec![ASTNode::Assignment {
            name: "value".into(),
            value: Expression::Equal(
                Box::new(Expression::Identifier("left".into())),
                Box::new(Expression::Identifier("right".into())),
            ),
        }]
    );
}

#[test]
fn parses_blocks_with_statements_and_a_final_expression() {
    let nodes = parse(tokenize("result := { inner := 2 inner + 3 }".chars()));
    let ASTNode::VariableDefinition {
        value: Expression::Block(items),
        ..
    } = &nodes[0]
    else {
        panic!("expected a block expression");
    };

    assert_eq!(items.len(), 2);
    assert!(matches!(items[0], ASTNode::VariableDefinition { .. }));
    assert!(matches!(
        items[1],
        ASTNode::Expression(Expression::Add(_, _))
    ));
}

#[test]
fn parses_an_operator_after_a_block_expression() {
    let nodes = parse(tokenize("result := { 1 } + 2".chars()));

    assert!(matches!(
        &nodes[0],
        ASTNode::VariableDefinition {
            value: Expression::Add(lhs, rhs),
            ..
        } if matches!(lhs.as_ref(), Expression::Block(_))
            && matches!(rhs.as_ref(), Expression::LiteralNumber(value) if value == "2")
    ));
}

#[test]
fn parses_function_definition_and_call() {
    let nodes = parse(tokenize(
        "add :: fn (x : i32, y : i32) i32 { x + y } result := add(3, 2)".chars(),
    ));
    assert!(matches!(
        &nodes[0],
        ASTNode::FunctionDefinition { name, parameters, return_type: DataType::I32, body: Expression::Block(_) }
            if name == "add" && parameters == &vec![("x".into(), DataType::I32), ("y".into(), DataType::I32)]
    ));
    assert!(matches!(
        &nodes[1],
        ASTNode::VariableDefinition { value: Expression::Call { name, arguments }, .. }
            if name == "add" && arguments.len() == 2
    ));
}

#[test]
fn parses_return_statement() {
    let nodes = parse(tokenize("id :: fn (x : i32) i32 { return x }".chars()));
    assert!(matches!(
        &nodes[0],
        ASTNode::FunctionDefinition { body: Expression::Block(items), .. }
            if matches!(&items[0], ASTNode::Return(Some(Expression::Identifier(name))) if name == "x")
    ));
}

#[test]
fn ignores_c_style_comments() {
    assert_eq!(
        tokenize("x := 1 // line comment\ny := /* block\ncomment */ x + 2".chars()),
        vec![
            Token::Identifier("x".into()),
            Token::Colon,
            Token::Equal,
            Token::LiteralNumber("1".into()),
            Token::Identifier("y".into()),
            Token::Colon,
            Token::Equal,
            Token::Identifier("x".into()),
            Token::Plus,
            Token::LiteralNumber("2".into()),
        ]
    );
}

#[test]
fn comments_can_appear_inside_function_definitions() {
    let nodes = parse(tokenize(
        "add :: fn (x : i32, y : i32) i32 { /* add */ x + y } result := add(1, 2) // done".chars(),
    ));
    assert_eq!(nodes.len(), 2);
}

#[test]
fn parses_struct_definition_literals_and_member_access() {
    let nodes = parse(tokenize(
        "pos :: struct { x : f32, y : f32 } point : pos = pos { x = 0, y = 0 } x := point.x"
            .chars(),
    ));
    assert!(
        matches!(&nodes[0], ASTNode::StructDefinition { name, fields }
        if *name == "pos" && fields.len() == 2)
    );
    assert!(
        matches!(&nodes[1], ASTNode::VariableDefinition { value: Expression::StructLiteral { type_name: Some("pos"), fields }, .. }
        if fields.len() == 2)
    );
    assert!(
        matches!(&nodes[2], ASTNode::VariableDefinition { value: Expression::MemberAccess { member, .. }, .. }
        if member == "x")
    );
}
