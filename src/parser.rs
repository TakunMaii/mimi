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
    Array(u64, &'static DataType),
    Slice(&'static DataType),
    Enum(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
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
    ArrayLiteral(Vec<Expression>),
    Index {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    Slice {
        object: Box<Expression>,
        start: Option<Box<Expression>>,
        end: Option<Box<Expression>>,
    },
    Match {
        value: Box<Expression>,
        arms: Vec<(Pattern, Expression)>,
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

#[derive(Clone, Debug, PartialEq)]
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
        fields: Vec<(String, DataType, Option<Expression>, bool)>,
    },
    If {
        condition: Expression,
        then_branch: Expression,
        else_branch: Option<Expression>,
    },
    While {
        condition: Expression,
        body: Expression,
    },
    For {
        binding: String,
        iterable: Expression,
        body: Expression,
    },
    Loop {
        body: Expression,
    },
    Defer {
        body: Expression,
    },
    EnumDefinition {
        name: &'static str,
        variants: Vec<(String, Option<DataType>)>,
    },
    IfLet {
        variant: String,
        binding: Option<String>,
        value: Expression,
        then_branch: Expression,
        else_branch: Option<Expression>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pattern {
    Variant {
        name: String,
        binding: Option<String>,
    },
}

pub(crate) fn parse(tokens: Vec<Token>) -> Vec<ASTNode> {
    let mut tokens = tokens.into_iter().peekable();
    let mut nodes = vec![];

    while let Some(token) = tokens.next() {
        nodes.push(parse_statement_token(token, &mut tokens));
    }

    nodes
}

fn parse_statement_token<I>(token: Token, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    match token {
        Token::Identifier(name) => parse_statement_after_name(name, tokens),
        Token::If => parse_if(tokens),
        Token::Match => ASTNode::Expression(parse_match(tokens)),
        Token::While => parse_while(tokens),
        Token::For => parse_for(tokens),
        Token::Loop => parse_loop(tokens),
        Token::Defer => parse_defer(tokens),
        _ => panic!("expected a statement"),
    }
}

fn parse_block_expression<I>(tokens: &mut std::iter::Peekable<I>) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(
        matches!(tokens.next(), Some(Token::LeftBrace)),
        "expected '{{'"
    );
    Expression::Block(parse_block(tokens))
}

fn parse_if<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    if let Some((variant, binding)) = peek_variant_pattern(tokens) {
        tokens.next();
        if binding.is_some() {
            tokens.next();
            tokens.next();
            tokens.next();
        }
        assert!(
            matches!(tokens.next(), Some(Token::Colon)),
            "expected ':' in if-let"
        );
        assert!(
            matches!(tokens.next(), Some(Token::Equal)),
            "expected '=' in if-let"
        );
        let value = parse_expression(tokens, 0);
        let then_branch = parse_block_expression(tokens);
        let else_branch = if matches!(tokens.peek(), Some(Token::Else)) {
            tokens.next();
            Some(parse_block_expression(tokens))
        } else {
            None
        };
        return ASTNode::IfLet {
            variant,
            binding,
            value,
            then_branch,
            else_branch,
        };
    }
    let condition = parse_expression(tokens, 0);
    let then_branch = parse_block_expression(tokens);
    let else_branch = if matches!(tokens.peek(), Some(Token::Else)) {
        tokens.next();
        Some(parse_block_expression(tokens))
    } else {
        None
    };
    ASTNode::If {
        condition,
        then_branch,
        else_branch,
    }
}

fn peek_variant_pattern<I>(tokens: &std::iter::Peekable<I>) -> Option<(String, Option<String>)>
where
    I: Iterator<Item = Token> + Clone,
{
    let mut lookahead = tokens.clone();
    let variant = match lookahead.next()? {
        Token::Identifier(name) => name,
        _ => return None,
    };
    let binding = if matches!(lookahead.peek(), Some(Token::LeftParen)) {
        lookahead.next();
        let binding = match lookahead.next()? {
            Token::Identifier(name) => name,
            _ => return None,
        };
        if !matches!(lookahead.next(), Some(Token::RightParen)) {
            return None;
        }
        Some(binding)
    } else {
        None
    };
    matches!(lookahead.next(), Some(Token::Colon)).then_some((variant, binding))
}

fn parse_match<I>(tokens: &mut std::iter::Peekable<I>) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    let value = parse_expression(tokens, 0);
    assert!(
        matches!(tokens.next(), Some(Token::LeftBrace)),
        "expected '{{' after match value"
    );
    let mut arms = Vec::new();
    while !matches!(tokens.peek(), Some(Token::RightBrace)) {
        let variant = match tokens.next() {
            Some(Token::Identifier(name)) => name,
            _ => panic!("expected enum variant pattern"),
        };
        let binding = if matches!(tokens.peek(), Some(Token::LeftParen)) {
            tokens.next();
            let name = match tokens.next() {
                Some(Token::Identifier(name)) => name,
                _ => panic!("expected pattern binding"),
            };
            assert!(
                matches!(tokens.next(), Some(Token::RightParen)),
                "expected ')' after pattern binding"
            );
            Some(name)
        } else {
            None
        };
        assert!(
            matches!(tokens.next(), Some(Token::FatArrow)),
            "expected '=>' after match pattern"
        );
        let body = parse_expression(tokens, 0);
        arms.push((
            Pattern::Variant {
                name: variant,
                binding,
            },
            body,
        ));
        if matches!(tokens.peek(), Some(Token::Comma)) {
            tokens.next();
        } else if !matches!(tokens.peek(), Some(Token::RightBrace)) {
            panic!("expected ',' or '}}' after match arm");
        }
    }
    tokens.next();
    Expression::Match {
        value: Box::new(value),
        arms,
    }
}

fn parse_while<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let condition = parse_expression(tokens, 0);
    ASTNode::While {
        condition,
        body: parse_block_expression(tokens),
    }
}

fn parse_for<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let binding = match tokens.next() {
        Some(Token::Identifier(name)) => name,
        _ => panic!("expected loop binding after 'for'"),
    };
    assert!(
        matches!(tokens.next(), Some(Token::In)),
        "expected 'in' after loop binding"
    );
    let iterable = parse_expression(tokens, 0);
    ASTNode::For {
        binding,
        iterable,
        body: parse_block_expression(tokens),
    }
}

fn parse_loop<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    ASTNode::Loop {
        body: parse_block_expression(tokens),
    }
}

fn parse_defer<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    ASTNode::Defer {
        body: parse_block_expression(tokens),
    }
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
                if matches!(tokens.peek(), Some(Token::Enum)) {
                    tokens.next();
                    return parse_enum_definition(name, tokens);
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
            Some(Token::LeftBracket) => {
                let mut type_tokens = vec![Token::LeftBracket];
                while let Some(token) = tokens.next() {
                    let done = matches!(token, Token::Identifier(_) | Token::RightBracket)
                        && matches!(tokens.peek(), Some(Token::Equal | Token::Colon));
                    type_tokens.push(token);
                    if done {
                        break;
                    }
                }
                let mut type_iter = type_tokens.into_iter().peekable();
                let data_type = parse_type(&mut type_iter);
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
            parameters.push((parameter_name, parse_type(tokens)));
            match tokens.next() {
                Some(Token::Comma) => continue,
                Some(Token::RightParen) => break,
                _ => panic!("expected ',' or ')' after parameter"),
            }
        }
    } else {
        tokens.next();
    }
    let return_type = parse_type(tokens);
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
        let field_type = parse_type(tokens);
        let (default, mutable) = match tokens.peek() {
            Some(Token::Equal) => {
                tokens.next();
                (Some(parse_expression(tokens, 0)), true)
            }
            Some(Token::Colon) => {
                tokens.next();
                (Some(parse_expression(tokens, 0)), false)
            }
            _ => (None, true),
        };
        fields.push((field_name, field_type, default, mutable));
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

fn parse_enum_definition<I>(name: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(
        matches!(tokens.next(), Some(Token::LeftBrace)),
        "expected '{{' after enum"
    );
    let mut variants = Vec::new();
    while !matches!(tokens.peek(), Some(Token::RightBrace)) {
        let variant = match tokens.next() {
            Some(Token::Identifier(name)) => name,
            _ => panic!("expected enum variant"),
        };
        let payload = if matches!(tokens.peek(), Some(Token::LeftParen)) {
            tokens.next();
            let ty = parse_type(tokens);
            assert!(
                matches!(tokens.next(), Some(Token::RightParen)),
                "expected ')' after enum payload type"
            );
            Some(ty)
        } else {
            None
        };
        variants.push((variant, payload));
        if matches!(tokens.peek(), Some(Token::Comma)) {
            tokens.next();
        } else if !matches!(tokens.peek(), Some(Token::RightBrace)) {
            panic!("expected ',' or '}}' after enum variant");
        }
    }
    tokens.next();
    ASTNode::EnumDefinition {
        name: Box::leak(name.into_boxed_str()),
        variants,
    }
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

fn parse_type<I>(tokens: &mut std::iter::Peekable<I>) -> DataType
where
    I: Iterator<Item = Token> + Clone,
{
    if matches!(tokens.peek(), Some(Token::LeftBracket)) {
        tokens.next();
        let length = match tokens.next() {
            Some(Token::RightBracket) => None,
            Some(Token::LiteralNumber(value)) => {
                let length = value.parse().expect("array length must be an integer");
                assert!(
                    matches!(tokens.next(), Some(Token::RightBracket)),
                    "expected ']' in array type"
                );
                Some(length)
            }
            _ => panic!("expected array length or ']'"),
        };
        let element = parse_type(tokens);
        let element = Box::leak(Box::new(element));
        return match length {
            Some(length) => DataType::Array(length, element),
            None => DataType::Slice(element),
        };
    }
    match tokens.next() {
        Some(Token::Identifier(name)) => parse_data_type(&name),
        _ => panic!("expected type"),
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
        Token::LeftBracket => {
            let mut values = Vec::new();
            if !matches!(tokens.peek(), Some(Token::RightBracket)) {
                loop {
                    values.push(parse_expression(tokens, 0));
                    match tokens.next() {
                        Some(Token::Comma) => continue,
                        Some(Token::RightBracket) => break,
                        _ => panic!("expected ',' or ']' in array literal"),
                    }
                }
            } else {
                tokens.next();
            }
            Expression::ArrayLiteral(values)
        }
        Token::Match => parse_match(tokens),
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
        } else if brace_starts_struct_literal(tokens) {
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
        if matches!(tokens.peek(), Some(Token::LeftBracket)) {
            tokens.next();
            let start = if matches!(
                tokens.peek(),
                Some(Token::Colon | Token::DotDot | Token::RightBracket)
            ) {
                None
            } else {
                Some(Box::new(parse_expression(tokens, 0)))
            };
            if matches!(tokens.peek(), Some(Token::Colon)) {
                tokens.next();
                let end = if matches!(tokens.peek(), Some(Token::RightBracket)) {
                    None
                } else {
                    Some(Box::new(parse_expression(tokens, 0)))
                };
                assert!(
                    matches!(tokens.next(), Some(Token::RightBracket)),
                    "expected ']' after slice"
                );
                lhs = Expression::Slice {
                    object: Box::new(lhs),
                    start,
                    end,
                };
            } else {
                assert!(
                    matches!(tokens.next(), Some(Token::RightBracket)),
                    "expected ']' after index"
                );
                lhs = Expression::Index {
                    object: Box::new(lhs),
                    index: start.expect("expected index"),
                };
            }
            continue;
        }
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

fn brace_starts_struct_literal<I>(tokens: &std::iter::Peekable<I>) -> bool
where
    I: Iterator<Item = Token> + Clone,
{
    let mut lookahead = tokens.clone();
    if !matches!(lookahead.next(), Some(Token::LeftBrace)) {
        return false;
    }
    match lookahead.next() {
        Some(Token::RightBrace) => true,
        Some(Token::Identifier(_)) => matches!(lookahead.next(), Some(Token::Equal)),
        _ => false,
    }
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
            Token::If | Token::While | Token::For | Token::Loop | Token::Defer => {
                parse_statement_token(first, tokens)
            }
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
