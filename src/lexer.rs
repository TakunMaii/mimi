#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Identifier(String),
    LiteralNumber(String),
    LiteralBool(bool),
    Fn,
    Struct,
    If,
    Else,
    While,
    For,
    Loop,
    Defer,
    In,
    Return,
    Comma,
    Dot,
    Colon,
    Equal,
    EqualEqual,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    AndAnd,
    OrOr,
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
}

pub(crate) fn tokenize<T: Iterator<Item = char>>(code: T) -> Vec<Token> {
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
            result.push(match identifier.as_str() {
                "true" => Token::LiteralBool(true),
                "false" => Token::LiteralBool(false),
                "fn" => Token::Fn,
                "struct" => Token::Struct,
                "if" => Token::If,
                "else" => Token::Else,
                "while" => Token::While,
                "for" => Token::For,
                "loop" => Token::Loop,
                "defer" => Token::Defer,
                "in" => Token::In,
                "return" => Token::Return,
                _ => Token::Identifier(identifier),
            });
        } else if ch.is_ascii_digit() {
            let mut number = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() || ch == '.' {
                    number.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            number.parse::<f64>().expect("invalid number literal");
            result.push(Token::LiteralNumber(number));
        } else {
            chars.next();
            match ch {
                ':' => result.push(Token::Colon),
                ',' => result.push(Token::Comma),
                '.' => result.push(Token::Dot),
                '=' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        result.push(Token::EqualEqual);
                    } else {
                        result.push(Token::Equal);
                    }
                }
                '!' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        result.push(Token::NotEqual);
                    } else {
                        result.push(Token::Bang);
                    }
                }
                '<' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        result.push(Token::LessThanOrEqual);
                    } else {
                        result.push(Token::LessThan);
                    }
                }
                '>' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        result.push(Token::GreaterThanOrEqual);
                    } else {
                        result.push(Token::GreaterThan);
                    }
                }
                '&' => {
                    assert_eq!(chars.next(), Some('&'), "expected '&&'");
                    result.push(Token::AndAnd);
                }
                '|' => {
                    assert_eq!(chars.next(), Some('|'), "expected '||'");
                    result.push(Token::OrOr);
                }
                '+' => result.push(Token::Plus),
                '-' => result.push(Token::Minus),
                '*' => result.push(Token::Star),
                '/' => match chars.peek() {
                    Some('/') => {
                        chars.next();
                        while !matches!(chars.next(), None | Some('\n')) {}
                    }
                    Some('*') => {
                        chars.next();
                        let mut closed = false;
                        while let Some(comment_char) = chars.next() {
                            if comment_char == '*' && chars.peek() == Some(&'/') {
                                chars.next();
                                closed = true;
                                break;
                            }
                        }
                        assert!(closed, "unterminated block comment");
                    }
                    _ => result.push(Token::Slash),
                },
                '%' => result.push(Token::Percent),
                '(' => result.push(Token::LeftParen),
                ')' => result.push(Token::RightParen),
                '{' => result.push(Token::LeftBrace),
                '}' => result.push(Token::RightBrace),
                _ => panic!("unexpected character: {ch}"),
            }
        }
    }

    result
}
