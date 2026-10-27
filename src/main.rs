
#[derive(Debug, PartialEq)]
enum Token {
    Identifier(String),
    LiteralNumber(f64),
    Colon,
    Equal,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LeftParen,
    RightParen,
}

#[derive(Debug, PartialEq)]
enum Expression {
    Identifier(String),
    LiteralNumber(f64),
    Negate(Box<Expression>),
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    Remainder(Box<Expression>, Box<Expression>),
}

#[derive(Debug, PartialEq)]
enum ASTNode {
    VariableDefinition { name: String, value: Expression },
    Assignment { name: String, value: Expression },
    CompileTimeConstant { name: String, value: Expression },
}

fn parse(tokens: Vec<Token>) -> Vec<ASTNode> {
    let mut tokens = tokens.into_iter().peekable();
    let mut nodes = vec![];

    while let Some(token) = tokens.next() {
        let name = match token {
            Token::Identifier(name) => name,
            _ => panic!("expected an identifier at the start of a statement"),
        };

        let node_kind = match tokens.next() {
            Some(Token::Equal) => 0,
            Some(Token::Colon) => match tokens.next() {
                Some(Token::Equal) => 1,
                Some(Token::Colon) => 2,
                _ => panic!("expected ':' or '=' after ':'"),
            },
            _ => panic!("expected an operator after identifier"),
        };

        let value = parse_expression(&mut tokens, 0);

        nodes.push(match node_kind {
            0 => ASTNode::Assignment { name, value },
            1 => ASTNode::VariableDefinition { name, value },
            _ => ASTNode::CompileTimeConstant { name, value },
        });
    }

    nodes
}

fn parse_expression<I>(tokens: &mut std::iter::Peekable<I>, min_precedence: u8) -> Expression
where
    I: Iterator<Item = Token>,
{
    let mut lhs = match tokens.next() {
        Some(Token::Identifier(name)) => Expression::Identifier(name),
        Some(Token::LiteralNumber(value)) => Expression::LiteralNumber(value),
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

fn tokenize<T: Iterator<Item=char>>(code : T) -> Vec<Token> {
    let mut result = vec![];
    let mut chars = code.peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
        } else if ch.is_alphabetic() || ch == '_' {
            let mut identifier = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_alphanumeric() || ch == '_' {
                    identifier.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            result.push(Token::Identifier(identifier));
        } else if ch.is_ascii_digit() {
            let mut number = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() || ch == '.' {
                    number.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            result.push(Token::LiteralNumber(number.parse().expect("invalid number literal")));
        } else {
            chars.next();
            match ch {
                ':' => result.push(Token::Colon),
                '=' => result.push(Token::Equal),
                '+' => result.push(Token::Plus),
                '-' => result.push(Token::Minus),
                '*' => result.push(Token::Star),
                '/' => result.push(Token::Slash),
                '%' => result.push(Token::Percent),
                '(' => result.push(Token::LeftParen),
                ')' => result.push(Token::RightParen),
                _ => panic!("unexpected character: {ch}"),
            }
        }
    }

    result
}

fn main() {
}

#[cfg(test)]
#[path = "../tests/parser/mod.rs"]
mod tests;
