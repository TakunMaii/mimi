pub(crate) fn render(source_name: &str, source: &str, phase: &str, message: &str) -> String {
    let location = locate(source, message);
    let Some((line_index, column)) = location else {
        return format!(
            "error[{phase}]: {message}\n --> {source_name}\n  |\n  = help: {}",
            suggestion(phase, message)
        );
    };
    render_at(source_name, source, phase, message, line_index, column, 1)
}

pub(crate) fn render_span(
    source_name: &str,
    source: &str,
    phase: &str,
    message: &str,
    span: crate::lexer::Span,
) -> String {
    let Some((line_index, column)) = offset_to_line_column(source, span.start) else {
        return render(source_name, source, phase, message);
    };
    let width = source
        .get(span.start..span.end)
        .map(|text| text.chars().count())
        .unwrap_or(1)
        .max(1);
    render_at(
        source_name,
        source,
        phase,
        message,
        line_index,
        column,
        width,
    )
}

pub(crate) fn find_token_span(
    tokens: &[crate::lexer::SpannedToken],
    message: &str,
) -> Option<crate::lexer::Span> {
    if message.contains("found bool")
        && let Some(token) = tokens
            .iter()
            .rev()
            .find(|token| matches!(token.token, crate::lexer::Token::LiteralBool(_)))
    {
        return Some(token.span);
    }
    let candidates = message
        .split('\'')
        .skip(1)
        .step_by(2)
        .filter(|part| !part.is_empty() && part.len() < 80);
    for candidate in candidates {
        if let Some(token) = tokens
            .iter()
            .rev()
            .find(|token| token_matches_text(&token.token, candidate))
        {
            return Some(token.span);
        }
    }
    None
}

fn token_matches_text(token: &crate::lexer::Token, text: &str) -> bool {
    match token {
        crate::lexer::Token::Identifier(value) | crate::lexer::Token::LiteralNumber(value) => {
            value == text
        }
        crate::lexer::Token::LiteralString(value) => value == text,
        crate::lexer::Token::LiteralBool(value) => text == value.to_string(),
        _ => false,
    }
}

fn render_at(
    source_name: &str,
    source: &str,
    phase: &str,
    message: &str,
    line_index: usize,
    column: usize,
    span_width: usize,
) -> String {
    let lines: Vec<_> = source.split('\n').collect();
    let start = line_index.saturating_sub(1);
    let end = (line_index + 2).min(lines.len());
    let number_width = end.max(1).to_string().len();
    let mut output = format!(
        "error[{phase}]: {message}\n --> {source_name}:{}:{}\n  |\n",
        line_index + 1,
        column + 1
    );
    for index in start..end {
        output.push_str(&format!(
            "{:>width$} | {}\n",
            index + 1,
            lines[index],
            width = number_width
        ));
        if index == line_index {
            let prefix: String = lines[index].chars().take(column).collect();
            let remaining = lines[index].chars().count().saturating_sub(column).max(1);
            let marker = format!(
                "^{}",
                "~".repeat(span_width.min(remaining).saturating_sub(1))
            );
            let indent = prefix
                .chars()
                .map(|ch| if ch == '\t' { '\t' } else { ' ' })
                .collect::<String>();
            output.push_str(&format!(
                "{:>width$} | {indent}{marker}\n",
                "",
                width = number_width
            ));
        }
    }
    output.push_str(&format!("  = help: {}", suggestion(phase, message)));
    output
}

fn locate(source: &str, message: &str) -> Option<(usize, usize)> {
    if message.contains("expected '}'")
        || message.contains("expected ']'")
        || message.contains("expected ')'")
        || message.contains("unterminated")
    {
        return offset_to_line_column(source, source.trim_end().len());
    }
    if let Some((_, character)) = message.rsplit_once("unexpected character: ")
        && let Some(ch) = character.chars().next()
        && let Some(offset) = source.find(ch)
    {
        return offset_to_line_column(source, offset);
    }
    if message.contains("expected an expression") || message.contains("expected expression") {
        let offset = source
            .split_inclusive('\n')
            .scan(0usize, |offset, line| {
                let start = *offset;
                *offset += line.len();
                Some((start, line))
            })
            .find_map(|(start, line)| {
                (line.contains('=')
                    && line
                        .trim_end()
                        .chars()
                        .last()
                        .is_some_and(|ch| matches!(ch, '}' | ')' | ']')))
                .then(|| {
                    line.char_indices()
                        .rev()
                        .find(|(_, ch)| matches!(ch, '}' | ')' | ']'))
                        .map(|(offset, _)| start + offset)
                })
                .flatten()
            })
            .or_else(|| {
                source
                    .char_indices()
                    .rev()
                    .find(|(_, ch)| matches!(ch, '}' | ')' | ']'))
                    .map(|(offset, _)| offset)
            })
            .unwrap_or_else(|| source.trim_end().len());
        return offset_to_line_column(source, offset);
    }
    let mut candidates = message
        .split('\'')
        .skip(1)
        .step_by(2)
        .filter(|part| !part.is_empty() && part.len() < 80);
    for candidate in candidates.by_ref() {
        if let Some(offset) = source.find(candidate) {
            return offset_to_line_column(source, offset);
        }
    }
    if message.contains("bool")
        && let Some(offset) = source.find("true").or_else(|| source.find("false"))
    {
        return offset_to_line_column(source, offset);
    }
    if (message.contains("number") || message.contains("integer"))
        && let Some((offset, _)) = source.char_indices().find(|(_, ch)| ch.is_ascii_digit())
    {
        return offset_to_line_column(source, offset);
    }
    let offset = source
        .trim_end()
        .char_indices()
        .last()
        .map_or(0, |(offset, ch)| offset + ch.len_utf8());
    offset_to_line_column(source, offset)
}

fn offset_to_line_column(source: &str, offset: usize) -> Option<(usize, usize)> {
    let prefix = source.get(..offset)?;
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix.rsplit('\n').next()?.chars().count();
    Some((line, column))
}

fn suggestion(phase: &str, message: &str) -> &'static str {
    if message.contains("expected '}'")
        || message.contains("expected ']'")
        || message.contains("expected ')'")
    {
        "check that each opening brace, bracket, or parenthesis has a matching closer"
    } else if message.contains("expected an expression") || message.contains("expected expression")
    {
        "add an expression here, such as a variable, literal, function call, or parenthesized expression"
    } else if message.contains("expected type") {
        "add a supported type name after this token"
    } else if message.contains("unsupported string escape") {
        r#"use one of the supported escapes: \", \\, \n, \r, or \t"#
    } else if message.contains("unterminated string")
        || message.contains("unterminated block comment")
    {
        "add the matching closing quote or */ before the end of the file"
    } else if message.contains("undefined identifier") || message.contains("undefined variable") {
        "check the spelling, declaration order, and imports for this name"
    } else if message.contains("expected ") && message.contains("found ") {
        "make the expression's type match the type required at this location"
    } else if phase.contains("lowering") || message.contains("compiler operation failed") {
        "this construct may not be supported by the current backend; simplify it or report this source and diagnostic"
    } else {
        "check the syntax at this location and compare it with the expected form"
    }
}

#[cfg(test)]
mod tests {
    use super::{find_token_span, render, render_span};

    #[test]
    fn renders_source_context_and_actionable_help() {
        let output = render(
            "example.mi",
            "main :: fn () i32 {\n  value := }\n",
            "parse",
            "expected an expression",
        );
        assert!(output.contains("example.mi:2:12"), "{output}");
        assert!(output.contains("value := }"));
        assert!(output.contains("add an expression here"));
    }

    #[test]
    fn points_type_errors_at_named_source_text() {
        let output = render(
            "example.mi",
            "main :: fn () i32 {\n  missing\n}",
            "type",
            "undefined identifier 'missing'",
        );
        assert!(output.contains("example.mi:2:3"));
        assert!(output.contains("check the spelling"));
    }

    #[test]
    fn renders_explicit_spans_and_highlights_the_full_token() {
        let output = render_span(
            "example.mi",
            "value := 123\n",
            "lex",
            "invalid number literal",
            crate::lexer::Span { start: 9, end: 12 },
        );
        assert!(output.contains("example.mi:1:10"));
        assert!(output.contains("^~~"));
    }

    #[test]
    fn finds_type_error_tokens_using_lexed_spans() {
        let source = "main :: fn () i32 { missing } ";
        let tokens = crate::lexer::tokenize_spanned(source).unwrap();
        let span = find_token_span(&tokens, "undefined identifier 'missing'").unwrap();
        assert_eq!(&source[span.start..span.end], "missing");
    }
}
