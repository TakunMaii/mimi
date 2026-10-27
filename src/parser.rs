use crate::lexer::Token;

#[derive(Debug, PartialEq)]
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
}

#[derive(Debug, PartialEq)]
pub(crate) enum Expression {
    Identifier(String),
    LiteralNumber(f64),
    LiteralBool(bool),
    Negate(Box<Expression>),
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    Remainder(Box<Expression>, Box<Expression>),
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
}

pub(crate) fn parse(tokens: Vec<Token>) -> Vec<ASTNode> {
    let mut tokens = tokens.into_iter().peekable();
    let mut nodes = vec![];

    while let Some(token) = tokens.next() {
        let name = match token {
            Token::Identifier(name) => name,
            _ => panic!("expected an identifier at the start of a statement"),
        };

        let (node_kind, data_type) = match tokens.next() {
            Some(Token::Equal) => (0, None),
            Some(Token::Colon) => match tokens.next() {
                Some(Token::Equal) => (1, None),
                Some(Token::Colon) => (2, None),
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

        let value = parse_expression(&mut tokens, 0);

        nodes.push(match node_kind {
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
        });
    }

    nodes
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
        _ => panic!("unknown data type: {name}"),
    }
}

fn parse_expression<I>(tokens: &mut std::iter::Peekable<I>, min_precedence: u8) -> Expression
where
    I: Iterator<Item = Token>,
{
    let mut lhs = match tokens.next() {
        Some(Token::Identifier(name)) => Expression::Identifier(name),
        Some(Token::LiteralNumber(value)) => Expression::LiteralNumber(value),
        Some(Token::LiteralBool(value)) => Expression::LiteralBool(value),
        Some(Token::Minus) => Expression::Negate(Box::new(parse_expression(tokens, 3))),
        Some(Token::LeftParen) => {
            let expression = parse_expression(tokens, 0);
            match tokens.next() {
                Some(Token::RightParen) => expression,
                _ => panic!("expected ')' after expression"),
            }
        }
        _ => panic!("expected an expression"),
    };

    loop {
        let (precedence, operator) = match tokens.peek() {
            Some(Token::Plus) => (1, 0),
            Some(Token::Minus) => (1, 1),
            Some(Token::Star) => (2, 2),
            Some(Token::Slash) => (2, 3),
            Some(Token::Percent) => (2, 4),
            _ => break,
        };
        if precedence < min_precedence {
            break;
        }
        tokens.next();
        let rhs = parse_expression(tokens, precedence + 1);
        lhs = match operator {
            0 => Expression::Add(Box::new(lhs), Box::new(rhs)),
            1 => Expression::Subtract(Box::new(lhs), Box::new(rhs)),
            2 => Expression::Multiply(Box::new(lhs), Box::new(rhs)),
            3 => Expression::Divide(Box::new(lhs), Box::new(rhs)),
            _ => Expression::Remainder(Box::new(lhs), Box::new(rhs)),
        };
    }

    lhs
}
