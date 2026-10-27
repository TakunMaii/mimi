use crate::lexer::Token;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DataType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F8,
    F16,
    F32,
    F64,
    Bool,
    Void,
    Struct(&'static str),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Expression {
    Identifier(String),
    LiteralNumber(String),
    LiteralBool(bool),
    Negate(Box<Expression>),
    Not(Box<Expression>),
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    Remainder(Box<Expression>, Box<Expression>),
    Equal(Box<Expression>, Box<Expression>),
    NotEqual(Box<Expression>, Box<Expression>),
    LessThan(Box<Expression>, Box<Expression>),
    LessThanOrEqual(Box<Expression>, Box<Expression>),
    GreaterThan(Box<Expression>, Box<Expression>),
    GreaterThanOrEqual(Box<Expression>, Box<Expression>),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Block(Vec<ASTNode>),
    Call {
        name: String,
        arguments: Vec<Expression>,
    },
    StructLiteral {
        type_name: Option<&'static str>,
        fields: Vec<(String, Expression)>,
    },
    MemberAccess {
        object: Box<Expression>,
        member: String,
    },
}

#[derive(Clone, Copy)]
enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
}

#[derive(Debug, PartialEq)]
pub(crate) enum ASTNode {
    VariableDefinition {
        name: String,
        data_type: Option<DataType>,
        value: Expression,
    },
    Assignment {
        name: String,
        value: Expression,
    },
    CompileTimeConstant {
        name: String,
        data_type: Option<DataType>,
        value: Expression,
    },
    Expression(Expression),
    FunctionDefinition {
        name: String,
        parameters: Vec<(String, DataType)>,
        return_type: DataType,
        body: Expression,
    },
    Return(Option<Expression>),
    StructDefinition {
        name: &'static str,
        fields: Vec<(String, DataType)>,
    },
}

pub(crate) fn parse(tokens: Vec<Token>) -> Vec<ASTNode> {
    let mut tokens = tokens.into_iter().peekable();
    let mut nodes = vec![];

    while let Some(token) = tokens.next() {
        let name = match token {
            Token::Identifier(name) => name,
            _ => panic!("expected an identifier at the start of a statement"),
        };

        nodes.push(parse_statement_after_name(name, &mut tokens));
    }

    nodes
}

fn parse_statement_after_name<I>(name: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let (node_kind, data_type) = match tokens.next() {
        Some(Token::Equal) => (0, None),
        Some(Token::Colon) => match tokens.next() {
            Some(Token::Equal) => (1, None),
            Some(Token::Colon) => {
                if matches!(tokens.peek(), Some(Token::Struct)) {
                    tokens.next();
                    return parse_struct_definition(name, tokens);
                }
                if matches!(tokens.peek(), Some(Token::Fn)) {
                    tokens.next();
                    return parse_function(name, tokens);
                }
                (2, None)
            }
            Some(Token::Identifier(type_name)) => {
                let data_type = parse_data_type(&type_name);
                match tokens.next() {
                    Some(Token::Equal) => (1, Some(data_type)),
                    Some(Token::Colon) => (2, Some(data_type)),
                    _ => panic!("expected '=' or ':' after type"),
                }
            }
            _ => panic!("expected ':', '=', or a type after ':'"),
        },
        _ => panic!("expected an operator after identifier"),
    };

    let value = parse_expression(tokens, 0);

    match node_kind {
        0 => ASTNode::Assignment { name, value },
        1 => ASTNode::VariableDefinition {
            name,
            data_type,
            value,
        },
        _ => ASTNode::CompileTimeConstant {
            name,
            data_type,
            value,
        },
    }
}

fn parse_function<I>(name: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(
        matches!(tokens.next(), Some(Token::LeftParen)),
        "expected '(' after fn"
    );
    let mut parameters = Vec::new();
    if !matches!(tokens.peek(), Some(Token::RightParen)) {
        loop {
            let parameter_name = match tokens.next() {
                Some(Token::Identifier(name)) => name,
                _ => panic!("expected parameter name"),
            };
            assert!(
                matches!(tokens.next(), Some(Token::Colon)),
                "expected ':' after parameter name"
            );
            let type_name = match tokens.next() {
                Some(Token::Identifier(name)) => name,
                _ => panic!("expected parameter type"),
            };
            parameters.push((parameter_name, parse_data_type(&type_name)));
            match tokens.next() {
                Some(Token::Comma) => continue,
                Some(Token::RightParen) => break,
                _ => panic!("expected ',' or ')' after parameter"),
            }
        }
    } else {
        tokens.next();
    }
    let return_type = match tokens.next() {
        Some(Token::Identifier(name)) => parse_data_type(&name),
        _ => panic!("expected function return type"),
    };
    let body = match tokens.next() {
        Some(Token::LeftBrace) => Expression::Block(parse_block(tokens)),
        _ => panic!("expected function body"),
    };
    ASTNode::FunctionDefinition {
        name,
        parameters,
        return_type,
        body,
    }
}

fn parse_struct_definition<I>(name: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(
        matches!(tokens.next(), Some(Token::LeftBrace)),
        "expected '{{' after struct"
    );
    let mut fields = Vec::new();
    while !matches!(tokens.peek(), Some(Token::RightBrace)) {
        let field_name = match tokens.next() {
            Some(Token::Identifier(name)) => name,
            _ => panic!("expected struct field name"),
        };
        assert!(
            matches!(tokens.next(), Some(Token::Colon)),
            "expected ':' after field name"
        );
        let field_type = match tokens.next() {
            Some(Token::Identifier(name)) => parse_data_type(&name),
            _ => panic!("expected struct field type"),
        };
        fields.push((field_name, field_type));
        if matches!(tokens.peek(), Some(Token::Comma)) {
            tokens.next();
        } else if !matches!(tokens.peek(), Some(Token::RightBrace)) {
            panic!("expected ',' or '}}' after struct field");
        }
    }
    tokens.next();
    let name = Box::leak(name.into_boxed_str());
    ASTNode::StructDefinition { name, fields }
}

fn parse_data_type(name: &str) -> DataType {
    match name {
        "u8" => DataType::U8,
        "u16" => DataType::U16,
        "u32" => DataType::U32,
        "u64" => DataType::U64,
        "i8" => DataType::I8,
        "i16" => DataType::I16,
        "i32" => DataType::I32,
        "i64" => DataType::I64,
        "f8" => DataType::F8,
        "f16" => DataType::F16,
        "f32" => DataType::F32,
        "f64" => DataType::F64,
        "bool" => DataType::Bool,
        "void" => DataType::Void,
        _ => DataType::Struct(Box::leak(name.to_owned().into_boxed_str())),
    }
}

fn parse_expression<I>(tokens: &mut std::iter::Peekable<I>, min_precedence: u8) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    let first = tokens.next().expect("expected an expression");
    parse_expression_from(first, tokens, min_precedence)
}

fn parse_expression_from<I>(
    first: Token,
    tokens: &mut std::iter::Peekable<I>,
    min_precedence: u8,
) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    let mut lhs = match first {
        Token::Identifier(name) => Expression::Identifier(name),
        Token::LiteralNumber(value) => Expression::LiteralNumber(value),
        Token::LiteralBool(value) => Expression::LiteralBool(value),
        Token::Minus => Expression::Negate(Box::new(parse_expression(tokens, 6))),
        Token::Bang => Expression::Not(Box::new(parse_expression(tokens, 6))),
        Token::LeftParen => {
            let expression = parse_expression(tokens, 0);
            match tokens.next() {
                Some(Token::RightParen) => expression,
                _ => panic!("expected ')' after expression"),
            }
        }
        Token::LeftBrace => parse_brace_expression(tokens),
        _ => panic!("expected an expression"),
    };

    if let Expression::Identifier(name) = lhs {
        if matches!(tokens.peek(), Some(Token::LeftParen)) {
            tokens.next();
            let mut arguments = Vec::new();
            if !matches!(tokens.peek(), Some(Token::RightParen)) {
                loop {
                    arguments.push(parse_expression(tokens, 0));
                    match tokens.next() {
                        Some(Token::Comma) => continue,
                        Some(Token::RightParen) => break,
                        _ => panic!("expected ',' or ')' after argument"),
                    }
                }
            } else {
                tokens.next();
            }
            lhs = Expression::Call { name, arguments };
        } else if matches!(tokens.peek(), Some(Token::LeftBrace)) {
            tokens.next();
            lhs = Expression::StructLiteral {
                type_name: Some(Box::leak(name.into_boxed_str())),
                fields: parse_struct_fields(tokens),
            };
        } else {
            lhs = Expression::Identifier(name);
        }
    }

    loop {
        if matches!(tokens.peek(), Some(Token::Dot)) {
            tokens.next();
            let member = match tokens.next() {
                Some(Token::Identifier(member)) => member,
                _ => panic!("expected member name after '.'"),
            };
            lhs = Expression::MemberAccess {
                object: Box::new(lhs),
                member,
            };
            continue;
        }
        let (precedence, operator) = match tokens.peek() {
            Some(Token::OrOr) => (1, BinaryOperator::Or),
            Some(Token::AndAnd) => (2, BinaryOperator::And),
            Some(Token::EqualEqual) => (3, BinaryOperator::Equal),
            Some(Token::NotEqual) => (3, BinaryOperator::NotEqual),
            Some(Token::LessThan) => (3, BinaryOperator::LessThan),
            Some(Token::LessThanOrEqual) => (3, BinaryOperator::LessThanOrEqual),
            Some(Token::GreaterThan) => (3, BinaryOperator::GreaterThan),
            Some(Token::GreaterThanOrEqual) => (3, BinaryOperator::GreaterThanOrEqual),
            Some(Token::Plus) => (4, BinaryOperator::Add),
            Some(Token::Minus) => (4, BinaryOperator::Subtract),
            Some(Token::Star) => (5, BinaryOperator::Multiply),
            Some(Token::Slash) => (5, BinaryOperator::Divide),
            Some(Token::Percent) => (5, BinaryOperator::Remainder),
            _ => break,
        };
        if precedence < min_precedence {
            break;
        }
        tokens.next();
        let rhs = parse_expression(tokens, precedence + 1);
        lhs = match operator {
            BinaryOperator::Add => Expression::Add(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::Subtract => Expression::Subtract(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::Multiply => Expression::Multiply(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::Divide => Expression::Divide(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::Remainder => Expression::Remainder(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::Equal => Expression::Equal(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::NotEqual => Expression::NotEqual(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::LessThan => Expression::LessThan(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::LessThanOrEqual => {
                Expression::LessThanOrEqual(Box::new(lhs), Box::new(rhs))
            }
            BinaryOperator::GreaterThan => Expression::GreaterThan(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::GreaterThanOrEqual => {
                Expression::GreaterThanOrEqual(Box::new(lhs), Box::new(rhs))
            }
            BinaryOperator::And => Expression::And(Box::new(lhs), Box::new(rhs)),
            BinaryOperator::Or => Expression::Or(Box::new(lhs), Box::new(rhs)),
        };
    }

    lhs
}

fn parse_struct_fields<I>(tokens: &mut std::iter::Peekable<I>) -> Vec<(String, Expression)>
where
    I: Iterator<Item = Token> + Clone,
{
    let mut fields = Vec::new();
    while !matches!(tokens.peek(), Some(Token::RightBrace)) {
        let name = match tokens.next() {
            Some(Token::Identifier(name)) => name,
            _ => panic!("expected struct field name"),
        };
        assert!(
            matches!(tokens.next(), Some(Token::Equal)),
            "expected '=' after field name"
        );
        fields.push((name, parse_expression(tokens, 0)));
        if matches!(tokens.peek(), Some(Token::Comma)) {
            tokens.next();
        } else if !matches!(tokens.peek(), Some(Token::RightBrace)) {
            panic!("expected ',' or '}}' after struct field value");
        }
    }
    tokens.next();
    fields
}

fn parse_brace_expression<I>(tokens: &mut std::iter::Peekable<I>) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    if matches!(tokens.peek(), Some(Token::RightBrace)) {
        tokens.next();
        return Expression::Block(Vec::new());
    }
    if let Some(Token::Identifier(name)) = tokens.peek().cloned() {
        let mut lookahead = tokens.clone();
        lookahead.next();
        if matches!(lookahead.next(), Some(Token::Equal)) {
            tokens.next();
            tokens.next();
            let value = parse_expression(tokens, 0);
            if matches!(tokens.peek(), Some(Token::Comma)) {
                tokens.next();
                let mut fields = vec![(name, value)];
                fields.extend(parse_struct_fields(tokens));
                return Expression::StructLiteral {
                    type_name: None,
                    fields,
                };
            }
            let mut items = vec![ASTNode::Assignment { name, value }];
            items.extend(parse_block(tokens));
            return Expression::Block(items);
        }
    }
    Expression::Block(parse_block(tokens))
}

fn parse_block<I>(tokens: &mut std::iter::Peekable<I>) -> Vec<ASTNode>
where
    I: Iterator<Item = Token> + Clone,
{
    let mut items = Vec::new();

    loop {
        match tokens.peek() {
            Some(Token::RightBrace) => {
                tokens.next();
                return items;
            }
            None => panic!("expected '}}' to close block"),
            _ => {}
        }

        let first = tokens.next().unwrap();
        let item = match first {
            Token::Return => {
                let value = if matches!(tokens.peek(), Some(Token::RightBrace)) {
                    None
                } else {
                    Some(parse_expression(tokens, 0))
                };
                ASTNode::Return(value)
            }
            Token::Identifier(name)
                if matches!(tokens.peek(), Some(Token::Equal | Token::Colon)) =>
            {
                parse_statement_after_name(name, tokens)
            }
            Token::Identifier(name) => {
                ASTNode::Expression(parse_expression_from(Token::Identifier(name), tokens, 0))
            }
            first => ASTNode::Expression(parse_expression_from(first, tokens, 0)),
        };
        items.push(item);
    }
}
