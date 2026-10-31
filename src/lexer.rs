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
    As,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpannedToken {
    pub(crate) token: Token,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LexError {
    pub(crate) message: String,
    pub(crate) span: Span,
}

#[cfg(test)]
pub(crate) fn tokenize<T: Iterator<Item = char>>(code: T) -> Vec<Token> {
    let source: String = code.collect();
    tokenize_spanned(&source)
        .unwrap_or_else(|error| panic!("{}", error.message))
        .into_iter()
        .map(|token| token.token)
        .collect()
}

pub(crate) fn tokenize_spanned(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let chars: Vec<char> = source.chars().collect();
    let byte_offsets: Vec<usize> = source
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(source.len()))
        .collect();
    let mut result = Vec::new();
    let mut index = 0;
    let mut after_member_dot = false;

    while let Some(&ch) = chars.get(index) {
        let start = index;
        if ch.is_whitespace() {
            index += 1;
        } else if ch.is_alphabetic() || ch == '_' {
            after_member_dot = false;
            let mut identifier = String::new();
            while let Some(&ch) = chars.get(index) {
                if ch.is_alphanumeric() || ch == '_' {
                    identifier.push(ch);
                    index += 1;
                } else {
                    break;
                }
            }
            let token = match identifier.as_str() {
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
                "as" => Token::As,
                "return" => Token::Return,
                "break" => Token::Break,
                "continue" => Token::Continue,
                _ => Token::Identifier(identifier),
            };
            push_spanned(&mut result, token, start, index, &byte_offsets);
        } else if ch == '"' {
            after_member_dot = false;
            index += 1;
            let mut value = String::new();
            let mut closed = false;
            while let Some(&next) = chars.get(index) {
                index += 1;
                match next {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' => {
                        let escape_start = index - 1;
                        let Some(escape) = chars.get(index).copied() else {
                            break;
                        };
                        index += 1;
                        match escape {
                            '"' => value.push('"'),
                            '\\' => value.push('\\'),
                            'n' => value.push('\n'),
                            'r' => value.push('\r'),
                            't' => value.push('\t'),
                            other => {
                                return Err(LexError {
                                    message: format!("unsupported string escape: \\{other}"),
                                    span: make_span(escape_start, index, &byte_offsets),
                                });
                            }
                        }
                    }
                    other => value.push(other),
                }
            }
            if !closed {
                return Err(LexError {
                    message: "unterminated string literal".into(),
                    span: make_span(start, chars.len(), &byte_offsets),
                });
            }
            push_spanned(
                &mut result,
                Token::LiteralString(value),
                start,
                index,
                &byte_offsets,
            );
        } else if ch.is_ascii_digit() {
            let mut number = String::new();
            while let Some(&ch) = chars.get(index) {
                if ch.is_ascii_digit()
                    || (ch == '.'
                        && !after_member_dot
                        && chars.get(index + 1) != Some(&'.'))
                {
                    number.push(ch);
                    index += 1;
                } else {
                    break;
                }
            }
            after_member_dot = false;
            if number.parse::<f64>().is_err() {
                return Err(LexError {
                    message: "invalid number literal".into(),
                    span: make_span(start, index, &byte_offsets),
                });
            }
            push_spanned(
                &mut result,
                Token::LiteralNumber(number),
                start,
                index,
                &byte_offsets,
            );
        } else {
            index += 1;
            after_member_dot = false;
            let token = match ch {
                ':' => Some(Token::Colon),
                ',' => Some(Token::Comma),
                '.' => {
                    if chars.get(index) == Some(&'.') {
                        index += 1;
                        Some(Token::DotDot)
                    } else {
                        after_member_dot = true;
                        Some(Token::Dot)
                    }
                }
                '=' => {
                    if chars.get(index) == Some(&'=') {
                        index += 1;
                        Some(Token::EqualEqual)
                    } else if chars.get(index) == Some(&'>') {
                        index += 1;
                        Some(Token::FatArrow)
                    } else {
                        Some(Token::Equal)
                    }
                }
                '!' => {
                    if chars.get(index) == Some(&'=') {
                        index += 1;
                        Some(Token::NotEqual)
                    } else {
                        Some(Token::Bang)
                    }
                }
                '<' => {
                    if chars.get(index) == Some(&'=') {
                        index += 1;
                        Some(Token::LessThanOrEqual)
                    } else {
                        Some(Token::LessThan)
                    }
                }
                '>' => {
                    if chars.get(index) == Some(&'=') {
                        index += 1;
                        Some(Token::GreaterThanOrEqual)
                    } else {
                        Some(Token::GreaterThan)
                    }
                }
                '&' => {
                    if chars.get(index) == Some(&'&') {
                        index += 1;
                        Some(Token::AndAnd)
                    } else {
                        Some(Token::Ampersand)
                    }
                }
                '@' => Some(Token::At),
                '|' => {
                    if chars.get(index) != Some(&'|') {
                        return Err(LexError {
                            message: "expected '||'".into(),
                            span: make_span(start, index, &byte_offsets),
                        });
                    }
                    index += 1;
                    Some(Token::OrOr)
                }
                '+' => Some(Token::Plus),
                '-' => Some(Token::Minus),
                '*' => Some(Token::Star),
                '/' => match chars.get(index) {
                    Some('/') => {
                        index += 1;
                        while chars.get(index).is_some_and(|ch| *ch != '\n') {
                            index += 1;
                        }
                        None
                    }
                    Some('*') => {
                        index += 1;
                        let mut closed = false;
                        while let Some(&comment_char) = chars.get(index) {
                            index += 1;
                            if comment_char == '*' && chars.get(index) == Some(&'/') {
                                index += 1;
                                closed = true;
                                break;
                            }
                        }
                        if !closed {
                            return Err(LexError {
                                message: "unterminated block comment".into(),
                                span: make_span(start, index, &byte_offsets),
                            });
                        }
                        None
                    }
                    _ => Some(Token::Slash),
                },
                '%' => Some(Token::Percent),
                '(' => Some(Token::LeftParen),
                ')' => Some(Token::RightParen),
                '{' => Some(Token::LeftBrace),
                '}' => Some(Token::RightBrace),
                '[' => Some(Token::LeftBracket),
                ']' => Some(Token::RightBracket),
                _ => {
                    return Err(LexError {
                        message: format!("unexpected character: {ch}"),
                        span: make_span(start, index, &byte_offsets),
                    });
                }
            };
            if let Some(token) = token {
                push_spanned(&mut result, token, start, index, &byte_offsets);
            }
        }
    }

    Ok(result)
}

fn push_spanned(
    output: &mut Vec<SpannedToken>,
    token: Token,
    start: usize,
    end: usize,
    byte_offsets: &[usize],
) {
    output.push(SpannedToken {
        token,
        span: make_span(start, end, byte_offsets),
    });
}

fn make_span(start: usize, end: usize, byte_offsets: &[usize]) -> Span {
    Span {
        start: byte_offsets[start],
        end: byte_offsets[end],
    }
}
