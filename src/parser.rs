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
    GenericStruct(&'static str, &'static [GenericArgument]),
    Array(u64, &'static DataType),
    Slice(&'static DataType),
    Enum(&'static str),
    CEnum(&'static str),
    GenericEnum(&'static str, &'static [GenericArgument]),
    Function(&'static [DataType], &'static DataType),
    SelfType,
    Pointer(&'static DataType),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GenericArgument {
    Type(DataType),
    Value(&'static Expression),
    Name(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expression {
    Identifier(String),
    LiteralNumber(String),
    LiteralString(String),
    LiteralBool(bool),
    Negate(Box<Expression>),
    Not(Box<Expression>),
    AddressOf(Box<Expression>),
    Dereference(Box<Expression>),
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
    If {
        condition: Box<Expression>,
        then_branch: Box<Expression>,
        else_branch: Box<Expression>,
    },
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
    Lambda {
        parameters: Vec<(Option<String>, DataType)>,
        return_type: DataType,
        body: Box<Expression>,
    },
    CallValue {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    StaticAccess {
        type_name: String,
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
    IndexAssignment {
        object: Expression,
        index: Expression,
        value: Expression,
    },
    PointerAssignment {
        pointer: Expression,
        value: Expression,
    },
    MemberAssignment {
        object: Expression,
        member: String,
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
    ExternalFunction {
        name: String,
        link_name: Option<String>,
        parameters: Vec<(String, DataType)>,
        return_type: DataType,
        variadic: bool,
        calling_convention: String,
        parameter_qualifiers: Vec<Vec<String>>,
        return_qualifiers: Vec<String>,
    },
    ExternalGlobal {
        name: String,
        link_name: Option<String>,
        data_type: DataType,
        mutable: bool,
        thread_local: bool,
    },
    Return(Option<Expression>),
    Break,
    Continue,
    StructDefinition {
        name: &'static str,
        parameters: Vec<String>,
        fields: Vec<(String, DataType, Option<Expression>, bool)>,
        methods: Vec<MethodDefinition>,
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
    DoWhile {
        body: Expression,
        condition: Expression,
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
        parameters: Vec<String>,
        variants: Vec<(String, Option<DataType>)>,
    },
    IfLet {
        variant: String,
        binding: Option<String>,
        value: Expression,
        then_branch: Expression,
        else_branch: Option<Expression>,
    },
    Import {
        alias: String,
        path: String,
    },
    CImport {
        alias: String,
        path: String,
    },
    CDefine {
        name: String,
        value: String,
    },
    CTypeAlias {
        name: String,
        data_type: DataType,
    },
    CTypeEnumRepresentation {
        name: String,
        underlying_type: DataType,
    },
    CTypeRecordLayout {
        name: String,
        size_bytes: u64,
        align_bytes: u64,
        is_union: bool,
        field_offsets: Vec<(String, u64)>,
        bit_fields: Vec<(String, u8, u8)>,
    },
    Public(Box<ASTNode>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MethodDefinition {
    pub(crate) name: String,
    pub(crate) parameters: Vec<(String, DataType)>,
    pub(crate) return_type: DataType,
    pub(crate) body: Expression,
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
        Token::Pub => ASTNode::Public(Box::new(parse_statement_token(
            tokens.next().expect("expected declaration after 'pub'"),
            tokens,
        ))),
        Token::Identifier(name) if matches!(tokens.peek(), Some(Token::LeftBracket)) => {
            parse_index_assignment_or_expression(name, tokens)
        }
        Token::Identifier(name) if matches!(tokens.peek(), Some(Token::Dot)) => {
            parse_member_assignment_or_expression(name, tokens)
        }
        Token::Star => parse_pointer_assignment_or_expression(tokens),
        Token::Identifier(name) => parse_statement_after_name(name, tokens),
        Token::Extern => parse_external_function(tokens),
        Token::If => parse_if(tokens),
        Token::Match => ASTNode::Expression(parse_match(tokens)),
        Token::Do => parse_do_while(tokens),
        Token::While => parse_while(tokens),
        Token::For => parse_for(tokens),
        Token::Loop => parse_loop(tokens),
        Token::Defer => parse_defer(tokens),
        Token::Break => ASTNode::Break,
        Token::Continue => ASTNode::Continue,
        _ => panic!("expected a statement"),
    }
}

fn parse_external_function<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let name = match tokens.next() {
        Some(Token::Identifier(name)) => name,
        _ => panic!("expected external function name"),
    };
    assert!(
        matches!(tokens.next(), Some(Token::Colon)),
        "expected ':' after external function name"
    );
    assert!(
        matches!(tokens.next(), Some(Token::Fn)),
        "expected 'fn' in external function signature"
    );
    assert!(
        matches!(tokens.next(), Some(Token::LeftParen)),
        "expected '(' in function signature"
    );
    let mut parameters = Vec::new();
    if !matches!(tokens.peek(), Some(Token::RightParen)) {
        loop {
            let parameter_name = match tokens.next() {
                Some(Token::Identifier(name)) => name,
                _ => panic!("expected external parameter name"),
            };
            assert!(
                matches!(tokens.next(), Some(Token::Colon)),
                "expected ':' after parameter name"
            );
            parameters.push((parameter_name, parse_type(tokens)));
            match tokens.next() {
                Some(Token::Comma) => continue,
                Some(Token::RightParen) => break,
                _ => panic!("expected ',' or ')' in external function signature"),
            }
        }
    } else {
        tokens.next();
    }
    let return_type = parse_type(tokens);
    let parameter_qualifiers = vec![Vec::new(); parameters.len()];
    ASTNode::ExternalFunction {
        name,
        link_name: None,
        parameters,
        return_type,
        variadic: false,
        calling_convention: "cdecl".into(),
        parameter_qualifiers,
        return_qualifiers: Vec::new(),
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
        let value = parse_block_condition(tokens);
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
    let condition = parse_block_condition(tokens);
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
    let condition = parse_block_condition(tokens);
    ASTNode::While {
        condition,
        body: parse_block_expression(tokens),
    }
}

fn parse_do_while<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let body = parse_block_expression(tokens);
    assert!(
        matches!(tokens.next(), Some(Token::While)),
        "expected 'while' after do-while body"
    );
    let condition = parse_block_condition(tokens);
    ASTNode::DoWhile { body, condition }
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
    let iterable = match tokens.next() {
        Some(Token::Identifier(name)) if matches!(tokens.peek(), Some(Token::LeftBrace)) => {
            Expression::Identifier(name)
        }
        Some(first) => parse_expression_from(first, tokens, 0),
        None => panic!("expected iterable after 'in'"),
    };
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

fn parse_if_expression<I>(tokens: &mut std::iter::Peekable<I>) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    let condition = parse_block_condition(tokens);
    let then_branch = parse_block_expression(tokens);
    assert!(
        matches!(tokens.next(), Some(Token::Else)),
        "if expression requires an else branch"
    );
    let else_branch = parse_block_expression(tokens);
    Expression::If {
        condition: Box::new(condition),
        then_branch: Box::new(then_branch),
        else_branch: Box::new(else_branch),
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
                if matches!(tokens.peek(), Some(Token::At)) {
                    let mut lookahead = tokens.clone();
                    lookahead.next();
                    return match lookahead.next() {
                        Some(Token::Identifier(kind)) if kind == "import" => {
                            parse_import(name, tokens)
                        }
                        Some(Token::Identifier(kind)) if kind == "cimport" => {
                            parse_c_import(name, tokens)
                        }
                        Some(Token::Identifier(kind)) if kind == "cDefine" => {
                            parse_c_define(name, tokens)
                        }
                        _ => panic!("expected @import, @cimport, or @cDefine after '::'"),
                    };
                }
                (2, None)
            }
            Some(first) => {
                let data_type = parse_type_from(first, tokens);
                match tokens.next() {
                    Some(Token::Equal) => (1, Some(data_type)),
                    Some(Token::Colon) => (2, Some(data_type)),
                    _ => panic!("expected '=' or ':' after type"),
                }
            }
            None => panic!("expected ':', '=', or a type after ':'"),
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

fn parse_index_assignment_or_expression<I>(
    name: String,
    tokens: &mut std::iter::Peekable<I>,
) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let target = parse_expression_from(Token::Identifier(name), tokens, 0);
    if !matches!(tokens.peek(), Some(Token::Equal)) {
        return ASTNode::Expression(target);
    }
    tokens.next();
    let value = parse_expression(tokens, 0);
    match target {
        Expression::Index { object, index } => ASTNode::IndexAssignment {
            object: *object,
            index: *index,
            value,
        },
        Expression::MemberAccess { object, member } => ASTNode::MemberAssignment {
            object: *object,
            member,
            value,
        },
        _ => panic!("indexed assignment requires an array index"),
    }
}

fn parse_import<I>(alias: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(
        matches!(tokens.next(), Some(Token::At)),
        "expected '@import'"
    );
    assert!(
        matches!(tokens.next(), Some(Token::Identifier(name)) if name == "import"),
        "expected '@import'"
    );
    assert!(
        matches!(tokens.next(), Some(Token::LeftParen)),
        "expected '(' after '@import'"
    );
    let path = match tokens.next() {
        Some(Token::LiteralString(path)) => path,
        _ => panic!("expected string path in '@import'"),
    };
    assert!(
        matches!(tokens.next(), Some(Token::RightParen)),
        "expected ')' after import path"
    );
    ASTNode::Import { alias, path }
}

fn parse_c_import<I>(alias: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(matches!(tokens.next(), Some(Token::At)));
    assert!(
        matches!(tokens.next(), Some(Token::Identifier(name)) if name == "cimport"),
        "expected '@cimport'"
    );
    assert!(
        matches!(tokens.next(), Some(Token::LeftParen)),
        "expected '(' after '@cimport'"
    );
    let path = match tokens.next() {
        Some(Token::LiteralString(path)) => path,
        _ => panic!("expected header path in '@cimport'"),
    };
    assert!(
        matches!(tokens.next(), Some(Token::RightParen)),
        "expected ')' after header path"
    );
    ASTNode::CImport { alias, path }
}

fn parse_c_define<I>(_alias: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(matches!(tokens.next(), Some(Token::At)));
    assert!(matches!(tokens.next(), Some(Token::Identifier(name)) if name == "cDefine"));
    assert!(matches!(tokens.next(), Some(Token::LeftParen)));
    let name = match tokens.next() {
        Some(Token::LiteralString(value)) => value,
        _ => panic!("expected macro name in '@cDefine'"),
    };
    assert!(
        matches!(tokens.next(), Some(Token::Comma)),
        "expected ',' in '@cDefine'"
    );
    let value = match tokens.next() {
        Some(Token::LiteralString(value)) => value,
        Some(Token::LiteralNumber(value)) => value,
        _ => panic!("expected macro value in '@cDefine'"),
    };
    assert!(matches!(tokens.next(), Some(Token::RightParen)));
    ASTNode::CDefine { name, value }
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
    let parameters = parse_generic_parameters(tokens);
    assert!(
        matches!(tokens.next(), Some(Token::LeftBrace)),
        "expected '{{' after struct"
    );
    let mut fields = Vec::new();
    let mut methods = Vec::new();
    while !matches!(tokens.peek(), Some(Token::RightBrace)) {
        if struct_member_starts_method(tokens) {
            let method_name = match tokens.next() {
                Some(Token::Identifier(name)) => name,
                _ => unreachable!(),
            };
            assert!(matches!(tokens.next(), Some(Token::LeftParen)));
            assert!(
                matches!(tokens.next(), Some(Token::RightParen)),
                "method member declaration uses empty parentheses"
            );
            assert!(
                matches!(tokens.next(), Some(Token::Colon))
                    && matches!(tokens.next(), Some(Token::Colon)),
                "expected '::' after method name"
            );
            assert!(
                matches!(tokens.next(), Some(Token::Fn)),
                "expected fn after '::'"
            );
            let method = parse_function(method_name, tokens);
            let ASTNode::FunctionDefinition {
                name,
                parameters,
                return_type,
                body,
            } = method
            else {
                unreachable!()
            };
            methods.push(MethodDefinition {
                name,
                parameters,
                return_type,
                body,
            });
            if matches!(tokens.peek(), Some(Token::Comma)) {
                tokens.next();
            }
            continue;
        }
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
    ASTNode::StructDefinition {
        name,
        parameters,
        fields,
        methods,
    }
}

fn struct_member_starts_method<I>(tokens: &std::iter::Peekable<I>) -> bool
where
    I: Iterator<Item = Token> + Clone,
{
    let mut lookahead = tokens.clone();
    matches!(lookahead.next(), Some(Token::Identifier(_)))
        && matches!(lookahead.next(), Some(Token::LeftParen))
        && matches!(lookahead.next(), Some(Token::RightParen))
        && matches!(lookahead.next(), Some(Token::Colon))
        && matches!(lookahead.next(), Some(Token::Colon))
        && matches!(lookahead.next(), Some(Token::Fn))
}

fn parse_enum_definition<I>(name: String, tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let parameters = parse_generic_parameters(tokens);
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
        parameters,
        variants,
    }
}

fn parse_generic_parameters<I>(tokens: &mut std::iter::Peekable<I>) -> Vec<String>
where
    I: Iterator<Item = Token> + Clone,
{
    if !matches!(tokens.peek(), Some(Token::LessThan)) {
        return Vec::new();
    }
    tokens.next();
    let mut parameters = Vec::new();
    loop {
        let Some(Token::Identifier(name)) = tokens.next() else {
            panic!("expected generic parameter name");
        };
        if parameters.contains(&name) {
            panic!("duplicate generic parameter '{name}'");
        }
        parameters.push(name);
        match tokens.next() {
            Some(Token::Comma) => continue,
            Some(Token::GreaterThan) => break,
            _ => panic!("expected ',' or '>' after generic parameter"),
        }
    }
    parameters
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
        "Self" => DataType::SelfType,
        _ => DataType::Struct(Box::leak(name.to_owned().into_boxed_str())),
    }
}

fn parse_named_type<I>(first: String, tokens: &mut std::iter::Peekable<I>) -> DataType
where
    I: Iterator<Item = Token> + Clone,
{
    let mut name = first;
    while matches!(tokens.peek(), Some(Token::Dot)) {
        tokens.next();
        match tokens.next() {
            Some(Token::Identifier(segment)) => {
                name.push('.');
                name.push_str(&segment);
            }
            _ => panic!("expected type name after '.'"),
        }
    }
    let data_type = parse_data_type(&name);
    if matches!(tokens.peek(), Some(Token::LessThan))
        && let Some(arguments) = parse_generic_arguments(tokens)
    {
        let arguments = Box::leak(arguments.into_boxed_slice());
        return match data_type {
            DataType::Struct(name) => DataType::GenericStruct(name, arguments),
            DataType::Enum(name) => DataType::GenericEnum(name, arguments),
            _ => panic!("generic arguments require a named type"),
        };
    }
    data_type
}

fn parse_generic_arguments<I>(tokens: &mut std::iter::Peekable<I>) -> Option<Vec<GenericArgument>>
where
    I: Iterator<Item = Token> + Clone,
{
    if !matches!(tokens.next(), Some(Token::LessThan)) {
        return None;
    }
    let mut arguments = Vec::new();
    loop {
        let argument = match tokens.next()? {
            Token::LiteralNumber(value) => {
                GenericArgument::Value(Box::leak(Box::new(Expression::LiteralNumber(value))))
            }
            Token::LiteralBool(value) => {
                GenericArgument::Value(Box::leak(Box::new(Expression::LiteralBool(value))))
            }
            Token::F8 => GenericArgument::Type(DataType::F8),
            Token::Fn => GenericArgument::Type(parse_type_from(Token::Fn, tokens)),
            Token::Star => GenericArgument::Type(parse_type_from(Token::Star, tokens)),
            Token::LeftBracket => {
                GenericArgument::Type(parse_type_from(Token::LeftBracket, tokens))
            }
            Token::Identifier(name) if matches!(tokens.peek(), Some(Token::LessThan)) => {
                GenericArgument::Type(parse_named_type(name, tokens))
            }
            Token::Identifier(name) if is_builtin_type_name(&name) => {
                GenericArgument::Type(parse_data_type(&name))
            }
            Token::Identifier(name) => GenericArgument::Name(Box::leak(name.into_boxed_str())),
            _ => return None,
        };
        arguments.push(argument);
        match tokens.next()? {
            Token::Comma => continue,
            Token::GreaterThan => return Some(arguments),
            _ => return None,
        }
    }
}

fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "f16"
            | "f32"
            | "f64"
            | "bool"
            | "void"
            | "Self"
    )
}

pub(crate) fn generic_instance_name(name: &str, arguments: &[GenericArgument]) -> String {
    let arguments = arguments
        .iter()
        .map(|argument| match argument {
            GenericArgument::Type(data_type) => format!("type:{data_type}"),
            GenericArgument::Name(name) => format!("name:{name}"),
            GenericArgument::Value(Expression::LiteralNumber(value)) => format!("value:{value}"),
            GenericArgument::Value(Expression::LiteralBool(value)) => format!("value:{value}"),
            GenericArgument::Value(Expression::Identifier(value)) => format!("value:{value}"),
            GenericArgument::Value(_) => String::from("value:<expression>"),
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{name}<{arguments}>")
}

pub(crate) fn decode_generic_instance_name(
    value: &str,
) -> Option<(&'static str, &'static [GenericArgument])> {
    let (name, arguments) = value.split_once('<')?;
    let arguments = arguments.strip_suffix('>')?;
    let parsed = if arguments.is_empty() {
        Vec::new()
    } else {
        split_generic_arguments(arguments)
            .into_iter()
            .map(|argument| {
                if let Some(type_name) = argument.strip_prefix("type:") {
                    Some(GenericArgument::Type(parse_display_type(type_name)?))
                } else if let Some(name) = argument.strip_prefix("name:") {
                    Some(GenericArgument::Name(Box::leak(
                        name.to_string().into_boxed_str(),
                    )))
                } else if let Some(value) = argument.strip_prefix("value:") {
                    let expression = if value == "true" || value == "false" {
                        Expression::LiteralBool(value == "true")
                    } else if value.parse::<f64>().is_ok() {
                        Expression::LiteralNumber(value.to_string())
                    } else {
                        Expression::Identifier(value.to_string())
                    };
                    Some(GenericArgument::Value(Box::leak(Box::new(expression))))
                } else {
                    None
                }
            })
            .collect::<Option<Vec<_>>>()?
    };
    Some((
        Box::leak(name.to_string().into_boxed_str()),
        Box::leak(parsed.into_boxed_slice()),
    ))
}

fn split_generic_arguments(value: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, character) in value.char_indices() {
        match character {
            '<' | '[' => depth += 1,
            '>' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                result.push(&value[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    result.push(&value[start..]);
    result
}

fn parse_display_type(value: &str) -> Option<DataType> {
    if let Some(inner) = value.strip_prefix('*') {
        return Some(DataType::Pointer(Box::leak(Box::new(parse_display_type(
            inner,
        )?))));
    }
    if let Some(inner) = value.strip_prefix("[]") {
        return Some(DataType::Slice(Box::leak(Box::new(parse_display_type(
            inner,
        )?))));
    }
    if let Some(rest) = value.strip_prefix('[') {
        let (length, inner) = rest.split_once(']')?;
        return Some(DataType::Array(
            length.parse().ok()?,
            Box::leak(Box::new(parse_display_type(inner)?)),
        ));
    }
    let (base, generic) = if let Some((base, generic)) = value.split_once('<') {
        (base, Some(generic.strip_suffix('>')?))
    } else {
        (value, None)
    };
    let data_type = match base {
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
        _ => DataType::Struct(Box::leak(base.to_string().into_boxed_str())),
    };
    if let Some(generic) = generic {
        let arguments = split_generic_arguments(generic)
            .into_iter()
            .map(|part| parse_generic_argument_encoding(part))
            .collect::<Option<Vec<_>>>()?;
        let arguments = Box::leak(arguments.into_boxed_slice());
        Some(match data_type {
            DataType::Struct(name) => DataType::GenericStruct(name, arguments),
            DataType::Enum(name) => DataType::GenericEnum(name, arguments),
            _ => return None,
        })
    } else {
        Some(data_type)
    }
}

fn parse_generic_argument_encoding(value: &str) -> Option<GenericArgument> {
    if let Some(data_type) = value.strip_prefix("type:") {
        Some(GenericArgument::Type(parse_display_type(data_type)?))
    } else if let Some(name) = value.strip_prefix("name:") {
        Some(GenericArgument::Name(Box::leak(
            name.to_string().into_boxed_str(),
        )))
    } else if let Some(value) = value.strip_prefix("value:") {
        let expression = if value == "true" || value == "false" {
            Expression::LiteralBool(value == "true")
        } else if value.parse::<f64>().is_ok() {
            Expression::LiteralNumber(value.to_string())
        } else {
            Expression::Identifier(value.to_string())
        };
        Some(GenericArgument::Value(Box::leak(Box::new(expression))))
    } else {
        None
    }
}

fn parse_type<I>(tokens: &mut std::iter::Peekable<I>) -> DataType
where
    I: Iterator<Item = Token> + Clone,
{
    let first = tokens.next().expect("expected type");
    parse_type_from(first, tokens)
}

fn parse_type_from<I>(first: Token, tokens: &mut std::iter::Peekable<I>) -> DataType
where
    I: Iterator<Item = Token> + Clone,
{
    if matches!(first, Token::Fn) {
        assert!(
            matches!(tokens.next(), Some(Token::LeftParen)),
            "expected '(' in function type"
        );
        let mut parameters = Vec::new();
        if !matches!(tokens.peek(), Some(Token::RightParen)) {
            loop {
                parameters.push(parse_type(tokens));
                match tokens.next() {
                    Some(Token::Comma) => continue,
                    Some(Token::RightParen) => break,
                    _ => panic!("expected ',' or ')' in function type"),
                }
            }
        } else {
            tokens.next();
        }
        let return_type = parse_type(tokens);
        return DataType::Function(
            Box::leak(parameters.into_boxed_slice()),
            Box::leak(Box::new(return_type)),
        );
    }
    if matches!(first, Token::Star) {
        return DataType::Pointer(Box::leak(Box::new(parse_type(tokens))));
    }
    if matches!(first, Token::LeftBracket) {
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
    match first {
        Token::F8 => DataType::F8,
        Token::Identifier(name) => parse_named_type(name, tokens),
        _ => panic!("expected type"),
    }
}

fn parse_expression<I>(tokens: &mut std::iter::Peekable<I>, min_precedence: u8) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    let first = tokens.next().expect("expected an expression");
    parse_expression_from_mode(first, tokens, min_precedence, true)
}

fn parse_block_condition<I>(tokens: &mut std::iter::Peekable<I>) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    let first = tokens.next().expect("expected an expression");
    parse_expression_from_mode(first, tokens, 0, false)
}

fn parse_expression_from<I>(
    first: Token,
    tokens: &mut std::iter::Peekable<I>,
    min_precedence: u8,
) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    parse_expression_from_mode(first, tokens, min_precedence, true)
}

fn parse_expression_from_mode<I>(
    first: Token,
    tokens: &mut std::iter::Peekable<I>,
    min_precedence: u8,
    allow_struct_literal: bool,
) -> Expression
where
    I: Iterator<Item = Token> + Clone,
{
    let mut lhs = match first {
        Token::Identifier(name) => Expression::Identifier(name),
        Token::LiteralNumber(value) => Expression::LiteralNumber(value),
        Token::LiteralString(value) => Expression::LiteralString(value),
        Token::LiteralBool(value) => Expression::LiteralBool(value),
        Token::Minus => Expression::Negate(Box::new(parse_expression_from_mode(
            tokens.next().expect("expected an expression"),
            tokens,
            6,
            allow_struct_literal,
        ))),
        Token::Bang => Expression::Not(Box::new(parse_expression_from_mode(
            tokens.next().expect("expected an expression"),
            tokens,
            6,
            allow_struct_literal,
        ))),
        Token::Ampersand => Expression::AddressOf(Box::new(parse_expression_from_mode(
            tokens.next().expect("expected an expression"),
            tokens,
            6,
            allow_struct_literal,
        ))),
        Token::Star => Expression::Dereference(Box::new(parse_expression_from_mode(
            tokens.next().expect("expected an expression"),
            tokens,
            6,
            allow_struct_literal,
        ))),
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
        Token::If => parse_if_expression(tokens),
        Token::Fn => parse_lambda(tokens),
        _ => panic!("expected an expression"),
    };

    if let Expression::Identifier(name) = lhs.clone() {
        let mut is_static = false;
        if matches!(tokens.peek(), Some(Token::Colon)) {
            let mut lookahead = tokens.clone();
            lookahead.next();
            if matches!(lookahead.next(), Some(Token::Colon)) {
                tokens.next();
                tokens.next();
                let member = match tokens.next() {
                    Some(Token::Identifier(member)) => member,
                    _ => panic!("expected static member"),
                };
                lhs = Expression::StaticAccess {
                    type_name: name.clone(),
                    member,
                };
                is_static = true;
            }
        }
        if is_static && matches!(tokens.peek(), Some(Token::LeftParen)) {
            let arguments = parse_arguments(tokens);
            lhs = Expression::CallValue {
                callee: Box::new(lhs),
                arguments,
            };
        } else if is_static {
        } else if matches!(tokens.peek(), Some(Token::LeftParen)) {
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
        } else if allow_struct_literal
            && matches!(tokens.peek(), Some(Token::LessThan))
            && generic_literal_follows(tokens)
        {
            let arguments = parse_generic_arguments(tokens).expect("generic arguments");
            let type_name = Box::leak(generic_instance_name(&name, &arguments).into_boxed_str());
            assert!(matches!(tokens.next(), Some(Token::LeftBrace)));
            lhs = Expression::StructLiteral {
                type_name: Some(type_name),
                fields: parse_struct_fields(tokens),
            };
        } else if allow_struct_literal
            && let Some(literal) = parse_qualified_struct_literal(&name, tokens)
        {
            lhs = literal;
        } else if allow_struct_literal && brace_starts_struct_literal(tokens) {
            tokens.next();
            lhs = Expression::StructLiteral {
                type_name: Some(Box::leak(name.clone().into_boxed_str())),
                fields: parse_struct_fields(tokens),
            };
        } else {
            lhs = Expression::Identifier(name.clone());
        }
    }

    loop {
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
            lhs = Expression::CallValue {
                callee: Box::new(lhs),
                arguments,
            };
            continue;
        }
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
        let rhs = parse_expression_from_mode(
            tokens.next().expect("expected an expression"),
            tokens,
            precedence + 1,
            allow_struct_literal,
        );
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

fn generic_literal_follows<I>(tokens: &std::iter::Peekable<I>) -> bool
where
    I: Iterator<Item = Token> + Clone,
{
    let mut lookahead = tokens.clone();
    if parse_generic_arguments(&mut lookahead).is_none() {
        return false;
    }
    brace_starts_struct_literal(&lookahead)
}

fn parse_pointer_assignment_or_expression<I>(tokens: &mut std::iter::Peekable<I>) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let target = parse_expression_from(Token::Star, tokens, 0);
    if !matches!(tokens.peek(), Some(Token::Equal)) {
        return ASTNode::Expression(target);
    }
    tokens.next();
    let value = parse_expression(tokens, 0);
    match target {
        Expression::Dereference(pointer) => ASTNode::PointerAssignment {
            pointer: *pointer,
            value,
        },
        _ => panic!("pointer assignment requires a dereference target"),
    }
}

fn parse_member_assignment_or_expression<I>(
    name: String,
    tokens: &mut std::iter::Peekable<I>,
) -> ASTNode
where
    I: Iterator<Item = Token> + Clone,
{
    let target = parse_expression_from(Token::Identifier(name), tokens, 0);
    if !matches!(tokens.peek(), Some(Token::Equal)) {
        return ASTNode::Expression(target);
    }
    tokens.next();
    let value = parse_expression(tokens, 0);
    match target {
        Expression::MemberAccess { object, member } => ASTNode::MemberAssignment {
            object: *object,
            member,
            value,
        },
        Expression::Index { object, index } => ASTNode::IndexAssignment {
            object: *object,
            index: *index,
            value,
        },
        _ => panic!("member assignment requires a field target"),
    }
}

fn parse_arguments<I>(tokens: &mut std::iter::Peekable<I>) -> Vec<Expression>
where
    I: Iterator<Item = Token> + Clone,
{
    assert!(matches!(tokens.next(), Some(Token::LeftParen)));
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
    arguments
}

fn parse_lambda<I>(tokens: &mut std::iter::Peekable<I>) -> Expression
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
            let name = match tokens.next() {
                Some(Token::Identifier(name)) => Some(name),
                _ => None,
            };
            assert!(
                matches!(tokens.next(), Some(Token::Colon)),
                "expected ':' in lambda parameter"
            );
            parameters.push((name, parse_type(tokens)));
            match tokens.next() {
                Some(Token::Comma) => continue,
                Some(Token::RightParen) => break,
                _ => panic!("expected ',' or ')' in lambda"),
            }
        }
    } else {
        tokens.next();
    }
    let return_type = parse_type(tokens);
    let body = parse_block_expression(tokens);
    Expression::Lambda {
        parameters,
        return_type,
        body: Box::new(body),
    }
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

fn parse_qualified_struct_literal<I>(
    first: &str,
    tokens: &mut std::iter::Peekable<I>,
) -> Option<Expression>
where
    I: Iterator<Item = Token> + Clone,
{
    let mut lookahead = tokens.clone();
    let mut segments = 0;
    while matches!(lookahead.peek(), Some(Token::Dot)) {
        lookahead.next();
        if !matches!(lookahead.next(), Some(Token::Identifier(_))) {
            return None;
        }
        segments += 1;
    }
    if segments == 0 || !brace_starts_struct_literal(&lookahead) {
        return None;
    }

    let mut type_name = first.to_string();
    while matches!(tokens.peek(), Some(Token::Dot)) {
        tokens.next();
        let Some(Token::Identifier(segment)) = tokens.next() else {
            unreachable!();
        };
        type_name.push('.');
        type_name.push_str(&segment);
    }
    tokens.next();
    Some(Expression::StructLiteral {
        type_name: Some(Box::leak(type_name.into_boxed_str())),
        fields: parse_struct_fields(tokens),
    })
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
            Token::Pub
            | Token::If
            | Token::Do
            | Token::While
            | Token::For
            | Token::Loop
            | Token::Defer
            | Token::Break
            | Token::Continue
            | Token::Star => parse_statement_token(first, tokens),
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
            Token::Identifier(name) if matches!(tokens.peek(), Some(Token::LeftBracket)) => {
                parse_index_assignment_or_expression(name, tokens)
            }
            Token::Identifier(name) if matches!(tokens.peek(), Some(Token::Dot)) => {
                parse_member_assignment_or_expression(name, tokens)
            }
            Token::Identifier(name) => {
                ASTNode::Expression(parse_expression_from(Token::Identifier(name), tokens, 0))
            }
            first => ASTNode::Expression(parse_expression_from(first, tokens, 0)),
        };
        items.push(item);
    }
}
