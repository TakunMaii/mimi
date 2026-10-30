#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Identifier(String),
    LiteralString(String),
    LiteralNumber(String),
    LiteralBool(bool),
    F8,
    Fn,
    Struct,
    Enum,
    Match,
    FatArrow,
    If,
    Else,
    Pub,
    Extern,
    Do,
    While,
    For,
    Loop,
    Defer,
    In,
    Return,
    Break,
    Continue,
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
    Ampersand,
    At,
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
    LeftBracket,
    RightBracket,
    DotDot,
}

pub(crate) fn tokenize<T: Iterator<Item = char>>(code: T) -> Vec<Token> {
    let mut result = vec![];
    let mut chars = code.peekable();
    let mut after_member_dot = false;

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
        } else if ch.is_alphabetic() || ch == '_' {
            after_member_dot = false;
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
                "f8" => Token::F8,
                "fn" => Token::Fn,
                "struct" => Token::Struct,
                "enum" => Token::Enum,
                "match" => Token::Match,
                "if" => Token::If,
                "else" => Token::Else,
                "pub" => Token::Pub,
                "extern" => Token::Extern,
                "do" => Token::Do,
                "while" => Token::While,
                "for" => Token::For,
                "loop" => Token::Loop,
                "defer" => Token::Defer,
                "in" => Token::In,
                "return" => Token::Return,
                "break" => Token::Break,
                "continue" => Token::Continue,
                _ => Token::Identifier(identifier),
            });
        } else if ch == '"' {
            after_member_dot = false;
            chars.next();
            let mut value = String::new();
            let mut closed = false;
            while let Some(next) = chars.next() {
                match next {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' => match chars.next() {
                        Some('"') => value.push('"'),
                        Some('\\') => value.push('\\'),
                        Some('n') => value.push('\n'),
                        Some('r') => value.push('\r'),
                        Some('t') => value.push('\t'),
                        Some(other) => panic!("unsupported string escape: \\{other}"),
                        None => break,
                    },
                    other => value.push(other),
                }
            }
            assert!(closed, "unterminated string literal");
            result.push(Token::LiteralString(value));
        } else if ch.is_ascii_digit() {
            let mut number = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() || (ch == '.' && !after_member_dot) {
                    number.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            after_member_dot = false;
            number.parse::<f64>().expect("invalid number literal");
            result.push(Token::LiteralNumber(number));
        } else {
            chars.next();
            after_member_dot = false;
            match ch {
                ':' => result.push(Token::Colon),
                ',' => result.push(Token::Comma),
                '.' => {
                    if chars.peek() == Some(&'.') {
                        chars.next();
                        result.push(Token::DotDot);
                    } else {
                        after_member_dot = true;
                        result.push(Token::Dot);
                    }
                }
                '=' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        result.push(Token::EqualEqual);
                    } else if chars.peek() == Some(&'>') {
                        chars.next();
                        result.push(Token::FatArrow);
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
                    if chars.peek() == Some(&'&') {
                        chars.next();
                        result.push(Token::AndAnd);
                    } else {
                        result.push(Token::Ampersand);
                    }
                }
                '@' => result.push(Token::At),
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
                '[' => result.push(Token::LeftBracket),
                ']' => result.push(Token::RightBracket),
                _ => panic!("unexpected character: {ch}"),
            }
        }
    }

    result
}
