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
        Some(Token::Minus) => Expression::Negate(Box::new(parse_expression(tokens, 6))),
        Some(Token::Bang) => Expression::Not(Box::new(parse_expression(tokens, 6))),
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
