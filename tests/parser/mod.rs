use super::*;

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
            Token::LiteralNumber(2.0),
            Token::RightParen,
            Token::Star,
            Token::LiteralNumber(3.0),
            Token::Slash,
            Token::LiteralNumber(4.0),
            Token::Percent,
            Token::LiteralNumber(2.0),
        ]
    );
}

#[test]
fn parses_arithmetic_precedence_and_parentheses() {
    assert_eq!(
        parse(tokenize("result := 1 + 2 * (3 - 4)".chars())),
        vec![ASTNode::VariableDefinition {
            name: "result".into(),
            value: Expression::Add(
                Box::new(Expression::LiteralNumber(1.0)),
                Box::new(Expression::Multiply(
                    Box::new(Expression::LiteralNumber(2.0)),
                    Box::new(Expression::Subtract(
                        Box::new(Expression::LiteralNumber(3.0)),
                        Box::new(Expression::LiteralNumber(4.0)),
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
                value: Expression::Add(
                    Box::new(Expression::Identifier("a".into())),
                    Box::new(Expression::LiteralNumber(1.0)),
                ),
            },
            ASTNode::Assignment {
                name: "y".into(),
                value: Expression::Multiply(
                    Box::new(Expression::Identifier("x".into())),
                    Box::new(Expression::LiteralNumber(2.0)),
                ),
            },
            ASTNode::CompileTimeConstant {
                name: "z".into(),
                value: Expression::Remainder(
                    Box::new(Expression::Subtract(
                        Box::new(Expression::Identifier("y".into())),
                        Box::new(Expression::LiteralNumber(3.0)),
                    )),
                    Box::new(Expression::LiteralNumber(2.0)),
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
            value: Expression::Negate(Box::new(Expression::Identifier("value".into()))),
        }]
    );
}
