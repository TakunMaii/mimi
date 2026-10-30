use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::lir::TargetArch;
use crate::parser::{ASTNode, DataType, Expression};

#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(HashMap<String, Json>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(fields) => fields.get(key),
            _ => None,
        }
    }

    fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    fn bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> JsonParser<'a> {
    fn parse(input: &'a str) -> Result<Json, String> {
        let mut parser = Self {
            bytes: input.as_bytes(),
            position: 0,
        };
        let value = parser.value()?;
        parser.whitespace();
        if parser.position != parser.bytes.len() {
            return Err("unexpected trailing JSON data from clang".into());
        }
        Ok(value)
    }

    fn whitespace(&mut self) {
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.whitespace();
        match self.bytes.get(self.position).copied() {
            Some(b'n') => {
                self.keyword(b"null")?;
                Ok(Json::Null)
            }
            Some(b't') => {
                self.keyword(b"true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.keyword(b"false")?;
                Ok(Json::Bool(false))
            }
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!(
                "invalid JSON from clang near byte {}",
                self.position
            )),
        }
    }

    fn keyword(&mut self, keyword: &[u8]) -> Result<(), String> {
        if self.bytes.get(self.position..self.position + keyword.len()) == Some(keyword) {
            self.position += keyword.len();
            Ok(())
        } else {
            Err("invalid JSON keyword from clang".into())
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.position += 1;
        let mut output = String::new();
        while let Some(byte) = self.bytes.get(self.position).copied() {
            self.position += 1;
            match byte {
                b'"' => return Ok(output),
                b'\\' => {
                    let escaped = self
                        .bytes
                        .get(self.position)
                        .copied()
                        .ok_or("unterminated JSON escape from clang")?;
                    self.position += 1;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{8}'),
                        b'f' => output.push('\u{c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => {
                            let end = self.position + 4;
                            let digits = std::str::from_utf8(
                                self.bytes
                                    .get(self.position..end)
                                    .ok_or("short unicode escape from clang")?,
                            )
                            .map_err(|_| "invalid unicode escape from clang")?;
                            let scalar = u32::from_str_radix(digits, 16)
                                .map_err(|_| "invalid unicode escape from clang")?;
                            output.push(
                                char::from_u32(scalar)
                                    .ok_or("invalid unicode scalar from clang")?,
                            );
                            self.position = end;
                        }
                        _ => return Err("invalid JSON escape from clang".into()),
                    }
                }
                0..=0x1f => return Err("control byte in JSON string from clang".into()),
                _ => {
                    let start = self.position - 1;
                    let first = byte;
                    let length = if first < 0x80 {
                        1
                    } else if first & 0xe0 == 0xc0 {
                        2
                    } else if first & 0xf0 == 0xe0 {
                        3
                    } else if first & 0xf8 == 0xf0 {
                        4
                    } else {
                        return Err("invalid UTF-8 in clang JSON".into());
                    };
                    let end = start + length;
                    let text = std::str::from_utf8(
                        self.bytes
                            .get(start..end)
                            .ok_or("short UTF-8 sequence in clang JSON")?,
                    )
                    .map_err(|_| "invalid UTF-8 in clang JSON")?;
                    output.push_str(text);
                    self.position = end;
                }
            }
        }
        Err("unterminated JSON string from clang".into())
    }

    fn array(&mut self) -> Result<Json, String> {
        self.position += 1;
        let mut values = Vec::new();
        loop {
            self.whitespace();
            if self.bytes.get(self.position) == Some(&b']') {
                self.position += 1;
                return Ok(Json::Array(values));
            }
            values.push(self.value()?);
            self.whitespace();
            match self.bytes.get(self.position) {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(Json::Array(values));
                }
                _ => return Err("expected ',' or ']' in clang JSON".into()),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.position += 1;
        let mut fields = HashMap::new();
        loop {
            self.whitespace();
            if self.bytes.get(self.position) == Some(&b'}') {
                self.position += 1;
                return Ok(Json::Object(fields));
            }
            if self.bytes.get(self.position) != Some(&b'"') {
                return Err("expected string key in clang JSON".into());
            }
            let key = self.string()?;
            self.whitespace();
            if self.bytes.get(self.position) != Some(&b':') {
                return Err("expected ':' in clang JSON object".into());
            }
            self.position += 1;
            let value = self.value()?;
            fields.insert(key, value);
            self.whitespace();
            match self.bytes.get(self.position) {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    return Ok(Json::Object(fields));
                }
                _ => return Err("expected ',' or '}' in clang JSON".into()),
            }
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(|byte| matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.position += 1;
        }
        let value = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| "invalid number in clang JSON")?;
        Ok(Json::Number(value.into()))
    }
}

pub(crate) fn expand_with_shims(
    nodes: &[ASTNode],
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> Result<(Vec<ASTNode>, Vec<String>), String> {
    let mut output = nodes.to_vec();
    let mut generated_sources = Vec::new();
    let mut generated_source_set = HashSet::new();
    let mut imports = Vec::new();
    let mut source_defines = Vec::new();
    for node in nodes {
        if let ASTNode::CImport { alias, path } = node {
            imports.push((alias.clone(), path.clone()));
        }
        if let ASTNode::CDefine { name, value } = node {
            source_defines.push(format!("{name}={value}"));
        }
    }
    let mut defines = defines.to_vec();
    defines.extend(source_defines);
    for (alias, path) in &imports {
        let (declarations, shims) =
            import_header_with_shims(path, target, include_paths, &defines, sysroot)?;
        generated_sources.extend(
            shims
                .into_iter()
                .filter(|shim| generated_source_set.insert(shim.clone())),
        );
        output.extend(declarations.into_iter().map(|mut node| {
            match &mut node {
                ASTNode::ExternalFunction {
                    name,
                    parameters,
                    return_type,
                    ..
                } => {
                    *name = format!("{alias}.{name}");
                    for (_, data_type) in parameters {
                        *data_type = qualify_c_type(*data_type, &alias);
                    }
                    *return_type = qualify_c_type(*return_type, &alias);
                }
                ASTNode::ExternalGlobal {
                    name, data_type, ..
                } => {
                    *name = format!("{alias}.{name}");
                    *data_type = qualify_c_type(*data_type, &alias);
                }
                ASTNode::StructDefinition { name, fields, .. } => {
                    *name = Box::leak(format!("{alias}.{}", *name).into_boxed_str());
                    for (_, data_type, _, _) in fields {
                        *data_type = qualify_c_type(*data_type, &alias);
                    }
                }
                ASTNode::EnumDefinition { name, variants, .. } => {
                    *name = Box::leak(format!("{alias}.{}", *name).into_boxed_str());
                    for (_, payload) in variants {
                        if let Some(data_type) = payload {
                            *data_type = qualify_c_type(*data_type, &alias);
                        }
                    }
                }
                ASTNode::CompileTimeConstant { name, .. } => {
                    *name = format!("{alias}.{name}");
                }
                ASTNode::CTypeAlias { name, data_type } => {
                    *name = format!("{alias}.{name}");
                    *data_type = qualify_c_type(*data_type, &alias);
                }
                ASTNode::CTypeEnumRepresentation { name, .. } => {
                    *name = format!("{alias}.{name}");
                }
                ASTNode::CTypeInfo { name, .. } => {
                    *name = format!("{alias}.{name}");
                }
                ASTNode::CTypeRecordLayout { name, .. } => {
                    *name = format!("{alias}.{name}");
                }
                _ => {}
            }
            node
        }));
    }
    let mut aliases = output
        .iter()
        .filter_map(|node| match node {
            ASTNode::CTypeAlias { name, data_type } => Some((name.clone(), *data_type)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for name in output.iter().filter_map(|node| match node {
        ASTNode::CTypeEnumRepresentation { name, .. } => Some(name.clone()),
        _ => None,
    }) {
        aliases.insert(
            name.clone(),
            DataType::CEnum(Box::leak(name.into_boxed_str())),
        );
    }
    rewrite_c_aliases(&mut output, &aliases);
    prepare_aggregate_callback_adapters(&mut output, &mut generated_sources, &imports)?;
    Ok((output, generated_sources))
}

fn prepare_aggregate_callback_adapters(
    nodes: &mut Vec<ASTNode>,
    generated_sources: &mut Vec<String>,
    imports: &[(String, String)],
) -> Result<(), String> {
    let records = nodes
        .iter()
        .filter_map(|node| match node {
            ASTNode::CTypeRecordLayout { name, is_union, .. } => Some((name.as_str(), *is_union)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let callbacks = nodes
        .iter()
        .filter_map(|node| match node {
            ASTNode::ExternalFunction {
                name, parameters, ..
            } => Some((
                name.clone(),
                parameters
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (_, data_type))| {
                        let DataType::Pointer(inner) = data_type else {
                            return None;
                        };
                        let DataType::Function(callback_parameters, result) = **inner else {
                            return None;
                        };
                        has_aggregate_callback_abi(callback_parameters, *result, &records)
                            .then(|| (index, callback_parameters.to_vec(), *result))
                    })
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        })
        .filter(|(_, callbacks)| !callbacks.is_empty())
        .collect::<Vec<_>>();
    if callbacks.is_empty() {
        return Ok(());
    }
    let functions = nodes
        .iter()
        .filter_map(|node| match node {
            ASTNode::FunctionDefinition {
                name,
                parameters,
                return_type,
                ..
            } => Some((
                name.clone(),
                parameters.iter().map(|(_, ty)| *ty).collect::<Vec<_>>(),
                *return_type,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut adapters = HashMap::new();
    let mut declarations = Vec::new();
    for (external_name, signatures) in callbacks {
        let alias = external_name
            .split_once('.')
            .map(|(alias, _)| alias)
            .unwrap_or_default();
        let Some((_, header)) = imports.iter().find(|(candidate, _)| candidate == alias) else {
            continue;
        };
        for (argument_index, parameters, result) in signatures {
            if !parameters
                .iter()
                .all(|ty| is_supported_callback_c_type(*ty, &records))
                || !is_supported_callback_c_type(result, &records)
            {
                continue;
            }
            for (target, target_parameters, target_result) in &functions {
                if *target_parameters != parameters || *target_result != result {
                    continue;
                }
                let adapter = format!("__mimi_c_aggregate_callback_{}", adapters.len());
                generated_sources.push(generate_aggregate_callback_adapter(
                    header,
                    &adapter,
                    target,
                    &parameters,
                    result,
                    &records,
                )?);
                adapters.insert(
                    (external_name.clone(), argument_index, target.clone()),
                    adapter.clone(),
                );
                declarations.push(ASTNode::ExternalFunction {
                    name: adapter,
                    link_name: None,
                    parameters: parameters
                        .iter()
                        .enumerate()
                        .map(|(index, ty)| (format!("arg{index}"), *ty))
                        .collect(),
                    return_type: result,
                    variadic: false,
                    calling_convention: "cdecl".into(),
                    parameter_qualifiers: vec![Vec::new(); parameters.len()],
                    return_qualifiers: Vec::new(),
                });
            }
        }
    }
    if adapters.is_empty() {
        return Ok(());
    }
    for node in nodes.iter_mut() {
        rewrite_callback_expression_in_node(node, &adapters);
    }
    nodes.extend(declarations);
    Ok(())
}

fn has_aggregate_callback_abi(
    parameters: &[DataType],
    result: DataType,
    records: &HashMap<&str, bool>,
) -> bool {
    parameters
        .iter()
        .any(|ty| matches!(ty, DataType::Struct(name) if records.contains_key(name)))
        || matches!(result, DataType::Struct(name) if records.contains_key(name))
}

fn is_supported_callback_c_type(data_type: DataType, records: &HashMap<&str, bool>) -> bool {
    matches!(
        data_type,
        DataType::Void
            | DataType::U8
            | DataType::U16
            | DataType::U32
            | DataType::U64
            | DataType::I8
            | DataType::I16
            | DataType::I32
            | DataType::I64
            | DataType::F32
            | DataType::F64
            | DataType::Bool
            | DataType::CEnum(_)
    ) || matches!(data_type, DataType::Struct(name) if records.contains_key(name))
        || matches!(data_type, DataType::Pointer(inner) if is_supported_callback_c_type(*inner, records) && !matches!(*inner, DataType::Struct(_)))
}

fn callback_c_type(data_type: DataType, records: &HashMap<&str, bool>) -> Result<String, String> {
    Ok(match data_type {
        DataType::Void => "void".into(),
        DataType::U8 => "unsigned char".into(),
        DataType::U16 => "unsigned short".into(),
        DataType::U32 => "unsigned int".into(),
        DataType::U64 => "unsigned long long".into(),
        DataType::I8 => "signed char".into(),
        DataType::I16 => "short".into(),
        DataType::I32 => "int".into(),
        DataType::I64 => "long long".into(),
        DataType::F32 => "float".into(),
        DataType::F64 => "double".into(),
        DataType::Bool => "_Bool".into(),
        DataType::CEnum(name) => format!("enum {}", name.rsplit('.').next().unwrap_or(name)),
        DataType::Struct(name) => {
            let tag = name.rsplit('.').next().unwrap_or(name);
            if records.get(name).copied().unwrap_or(false) {
                format!("union {tag}")
            } else {
                format!("struct {tag}")
            }
        }
        DataType::Pointer(inner) => format!("{} *", callback_c_type(*inner, records)?),
        _ => return Err(format!("unsupported C callback type {data_type:?}")),
    })
}

fn generate_aggregate_callback_adapter(
    header: &str,
    adapter: &str,
    target: &str,
    parameters: &[DataType],
    result: DataType,
    records: &HashMap<&str, bool>,
) -> Result<String, String> {
    let include = header.replace('\\', "\\\\").replace('"', "\\\"");
    let result_type = callback_c_type(result, records)?;
    let aggregate_result = matches!(result, DataType::Struct(_));
    let mut target_parameters = Vec::new();
    if aggregate_result {
        target_parameters.push(format!("{result_type} *result"));
    }
    for (index, parameter) in parameters.iter().enumerate() {
        let parameter_type = callback_c_type(*parameter, records)?;
        target_parameters.push(if matches!(parameter, DataType::Struct(_)) {
            format!("{parameter_type} *arg{index}")
        } else {
            format!("{parameter_type} arg{index}")
        });
    }
    let target_result = if aggregate_result {
        "void"
    } else {
        &result_type
    };
    let target_symbol = format!("_{}", target.replace('\\', "\\\\").replace('"', "\\\""));
    let mut source = format!(
        "#include \"{include}\"\nextern {target_result} __mimi_callback_target({}) __asm__(\"{target_symbol}\");\n",
        if target_parameters.is_empty() {
            "void".into()
        } else {
            target_parameters.join(", ")
        }
    );
    let wrapper_parameters = parameters
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("{} arg{index}", callback_c_type(*ty, records).unwrap()))
        .collect::<Vec<_>>();
    source.push_str(&format!(
        "{result_type} {adapter}({}) {{\n",
        if wrapper_parameters.is_empty() {
            "void".into()
        } else {
            wrapper_parameters.join(", ")
        }
    ));
    let mut arguments = Vec::new();
    if aggregate_result {
        source.push_str(&format!("    {result_type} result;\n"));
        arguments.push("&result".to_string());
    }
    arguments.extend(parameters.iter().enumerate().map(|(index, ty)| {
        if matches!(ty, DataType::Struct(_)) {
            format!("&arg{index}")
        } else {
            format!("arg{index}")
        }
    }));
    if aggregate_result {
        source.push_str(&format!(
            "    __mimi_callback_target({});\n    return result;\n}}\n",
            arguments.join(", ")
        ));
    } else if result == DataType::Void {
        source.push_str(&format!(
            "    __mimi_callback_target({});\n}}\n",
            arguments.join(", ")
        ));
    } else {
        source.push_str(&format!(
            "    return __mimi_callback_target({});\n}}\n",
            arguments.join(", ")
        ));
    }
    Ok(source)
}

fn rewrite_callback_expression_in_node(
    node: &mut ASTNode,
    adapters: &HashMap<(String, usize, String), String>,
) {
    match node {
        ASTNode::VariableDefinition { value, .. }
        | ASTNode::CompileTimeConstant { value, .. }
        | ASTNode::Expression(value) => rewrite_callback_expression(value, adapters),
        ASTNode::FunctionDefinition { body, .. } => rewrite_callback_expression(body, adapters),
        ASTNode::Public(inner) => rewrite_callback_expression_in_node(inner, adapters),
        _ => {}
    }
}

fn rewrite_callback_expression(
    expression: &mut Expression,
    adapters: &HashMap<(String, usize, String), String>,
) {
    match expression {
        Expression::Block(nodes) => {
            for node in nodes {
                rewrite_callback_expression_in_node(node, adapters);
            }
        }
        Expression::Call { name, arguments } => {
            for argument in arguments.iter_mut() {
                rewrite_callback_expression(argument, adapters);
            }
            for (index, argument) in arguments.iter_mut().enumerate() {
                if let Expression::Identifier(target) = argument
                    && let Some(adapter) = adapters.get(&(name.clone(), index, target.clone()))
                {
                    *target = adapter.clone();
                }
            }
        }
        Expression::CallValue { callee, arguments } => {
            let qualified_name = expression_path(callee);
            rewrite_callback_expression(callee, adapters);
            for argument in arguments.iter_mut() {
                rewrite_callback_expression(argument, adapters);
            }
            if let Some(name) = qualified_name {
                for (index, argument) in arguments.iter_mut().enumerate() {
                    if let Expression::Identifier(target) = argument
                        && let Some(adapter) = adapters.get(&(name.clone(), index, target.clone()))
                    {
                        *target = adapter.clone();
                    }
                }
            }
        }
        Expression::Negate(inner)
        | Expression::Not(inner)
        | Expression::AddressOf(inner)
        | Expression::Dereference(inner)
        | Expression::MemberAccess { object: inner, .. } => {
            rewrite_callback_expression(inner, adapters)
        }
        Expression::Add(left, right)
        | Expression::Subtract(left, right)
        | Expression::Multiply(left, right)
        | Expression::Divide(left, right)
        | Expression::Remainder(left, right)
        | Expression::Equal(left, right)
        | Expression::NotEqual(left, right)
        | Expression::LessThan(left, right)
        | Expression::LessThanOrEqual(left, right)
        | Expression::GreaterThan(left, right)
        | Expression::GreaterThanOrEqual(left, right)
        | Expression::And(left, right)
        | Expression::Or(left, right)
        | Expression::Index {
            object: left,
            index: right,
        } => {
            rewrite_callback_expression(left, adapters);
            rewrite_callback_expression(right, adapters);
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            rewrite_callback_expression(condition, adapters);
            rewrite_callback_expression(then_branch, adapters);
            rewrite_callback_expression(else_branch, adapters);
        }
        Expression::StructLiteral { fields, .. } => {
            for (_, value) in fields {
                rewrite_callback_expression(value, adapters);
            }
        }
        Expression::ArrayLiteral(values) | Expression::TupleLiteral(values) => {
            for value in values {
                rewrite_callback_expression(value, adapters);
            }
        }
        Expression::Slice { object, start, end } => {
            rewrite_callback_expression(object, adapters);
            for bound in start.iter_mut().chain(end.iter_mut()) {
                rewrite_callback_expression(bound, adapters);
            }
        }
        Expression::Match { value, arms } => {
            rewrite_callback_expression(value, adapters);
            for (_, arm) in arms {
                rewrite_callback_expression(arm, adapters);
            }
        }
        Expression::Lambda { body, .. } => rewrite_callback_expression(body, adapters),
        Expression::Identifier(_)
        | Expression::LiteralNumber(_)
        | Expression::LiteralString(_)
        | Expression::LiteralBool(_)
        | Expression::StaticAccess { .. } => {}
    }
}

fn expression_path(expression: &Expression) -> Option<String> {
    match expression {
        Expression::Identifier(name) => Some(name.clone()),
        Expression::MemberAccess { object, member } => {
            Some(format!("{}.{}", expression_path(object)?, member))
        }
        _ => None,
    }
}

fn rewrite_c_aliases(nodes: &mut [ASTNode], aliases: &HashMap<String, DataType>) {
    fn rewrite(data_type: DataType, aliases: &HashMap<String, DataType>) -> DataType {
        match data_type {
            DataType::Struct(name) => aliases.get(name).copied().unwrap_or(data_type),
            DataType::Pointer(inner) => {
                DataType::Pointer(Box::leak(Box::new(rewrite(*inner, aliases))))
            }
            DataType::Array(length, inner) => {
                DataType::Array(length, Box::leak(Box::new(rewrite(*inner, aliases))))
            }
            DataType::Slice(inner) => {
                DataType::Slice(Box::leak(Box::new(rewrite(*inner, aliases))))
            }
            DataType::Function(parameters, result) => DataType::Function(
                Box::leak(
                    parameters
                        .iter()
                        .map(|parameter| rewrite(*parameter, aliases))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
                Box::leak(Box::new(rewrite(*result, aliases))),
            ),
            other => other,
        }
    }
    fn rewrite_expression(expression: &mut Expression, aliases: &HashMap<String, DataType>) {
        match expression {
            Expression::Block(nodes) => rewrite_c_aliases(nodes, aliases),
            Expression::Call { arguments, .. } | Expression::ArrayLiteral(arguments) => {
                for argument in arguments {
                    rewrite_expression(argument, aliases);
                }
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
            } => {
                rewrite_expression(condition, aliases);
                rewrite_expression(then_branch, aliases);
                rewrite_expression(else_branch, aliases);
            }
            Expression::StructLiteral { fields, .. } => {
                for (_, value) in fields {
                    rewrite_expression(value, aliases);
                }
            }
            Expression::CallValue { callee, arguments } => {
                rewrite_expression(callee, aliases);
                for argument in arguments {
                    rewrite_expression(argument, aliases);
                }
            }
            _ => {}
        }
    }
    for node in nodes {
        match node {
            ASTNode::VariableDefinition {
                data_type, value, ..
            }
            | ASTNode::CompileTimeConstant {
                data_type, value, ..
            } => {
                if let Some(data_type) = data_type {
                    *data_type = rewrite(*data_type, aliases);
                }
                rewrite_expression(value, aliases);
            }
            ASTNode::Expression(value) => rewrite_expression(value, aliases),
            ASTNode::FunctionDefinition {
                parameters,
                return_type,
                body,
                ..
            } => {
                for (_, data_type) in parameters {
                    *data_type = rewrite(*data_type, aliases);
                }
                *return_type = rewrite(*return_type, aliases);
                rewrite_expression(body, aliases);
            }
            ASTNode::ExternalFunction {
                parameters,
                return_type,
                ..
            } => {
                for (_, data_type) in parameters {
                    *data_type = rewrite(*data_type, aliases);
                }
                *return_type = rewrite(*return_type, aliases);
            }
            ASTNode::StructDefinition {
                fields, methods, ..
            } => {
                for (_, data_type, _, _) in fields {
                    *data_type = rewrite(*data_type, aliases);
                }
                for method in methods {
                    for (_, data_type) in &mut method.parameters {
                        *data_type = rewrite(*data_type, aliases);
                    }
                    method.return_type = rewrite(method.return_type, aliases);
                    rewrite_expression(&mut method.body, aliases);
                }
            }
            ASTNode::Public(inner) => rewrite_c_aliases(std::slice::from_mut(inner), aliases),
            _ => {}
        }
    }
}

fn qualify_c_type(data_type: DataType, alias: &str) -> DataType {
    match data_type {
        DataType::Struct(name) => {
            DataType::Struct(Box::leak(format!("{alias}.{name}").into_boxed_str()))
        }
        DataType::Pointer(inner) => {
            DataType::Pointer(Box::leak(Box::new(qualify_c_type(*inner, alias))))
        }
        DataType::Function(parameters, result) => DataType::Function(
            Box::leak(
                parameters
                    .iter()
                    .map(|parameter| qualify_c_type(*parameter, alias))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            Box::leak(Box::new(qualify_c_type(*result, alias))),
        ),
        other => other,
    }
}

fn import_header_with_shims(
    path: &str,
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> Result<(Vec<ASTNode>, Vec<String>), String> {
    let clang = std::env::var_os("MIMI_CLANG").unwrap_or("clang".into());
    let cache_key = header_cache_key(&clang, path, target, include_paths, defines, sysroot);
    static CACHE: OnceLock<Mutex<HashMap<u64, (Vec<ASTNode>, Vec<String>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(imported) = cache.get(&cache_key)
    {
        return Ok(imported.clone());
    }
    let mut command = Command::new(&clang);
    command.args(["-Xclang", "-ast-dump=json", "-fsyntax-only", "-x", "c"]);
    command.arg(match target {
        TargetArch::Aarch64 => "--target=aarch64-apple-darwin",
        TargetArch::X86_64 => "--target=x86_64-apple-darwin",
    });
    if let Some(sysroot) = sysroot {
        command.arg("--sysroot").arg(sysroot);
    }
    command.arg("-include").arg(path);
    for include in include_paths {
        command.arg("-I").arg(include);
    }
    for define in defines {
        command.arg(format!("-D{define}"));
    }
    command.arg("-");
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start clang for @cimport('{path}'): {error}"))?;
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .ok_or("clang stdin unavailable")?
        .write_all(b"\n")
        .map_err(|error| format!("cannot provide clang input: {error}"))?;
    let result = child
        .wait_with_output()
        .map_err(|error| format!("cannot read clang output: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "clang failed while importing '{path}': {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let json = JsonParser::parse(&String::from_utf8_lossy(&result.stdout))?;
    let record_layouts =
        import_record_layouts(&clang, path, target, include_paths, defines, sysroot)?;
    let (mut declarations, generated_sources) =
        declarations_from_ast_with_shims(&json, path, &record_layouts)?;
    declarations.extend(import_numeric_macros(
        path,
        target,
        include_paths,
        defines,
        sysroot,
        &declarations,
    )?);
    if declarations.is_empty() {
        return Err(format!(
            "@cimport('{path}') found no supported C declarations"
        ));
    }
    if let Ok(mut cache) = cache.lock() {
        cache.insert(cache_key, (declarations.clone(), generated_sources.clone()));
    }
    Ok((declarations, generated_sources))
}

fn header_cache_key(
    clang: &std::ffi::OsStr,
    header: &str,
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    clang.hash(&mut hasher);
    header.hash(&mut hasher);
    format!("{target:?}").hash(&mut hasher);
    for include in include_paths {
        include.hash(&mut hasher);
    }
    defines.hash(&mut hasher);
    sysroot.hash(&mut hasher);
    if let Some(header_path) = std::iter::once(std::path::PathBuf::from(header))
        .chain(include_paths.iter().map(|directory| directory.join(header)))
        .find(|candidate| candidate.is_file())
        && let Ok(contents) = std::fs::read(header_path)
    {
        contents.hash(&mut hasher);
    }
    let version = Command::new(clang).arg("--version").output();
    if let Ok(version) = version {
        version.stdout.hash(&mut hasher);
    }
    hasher.finish()
}

fn import_numeric_macros(
    path: &str,
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
    existing: &[ASTNode],
) -> Result<Vec<ASTNode>, String> {
    let mut command = Command::new(std::env::var_os("MIMI_CLANG").unwrap_or("clang".into()));
    command.args(["-dM", "-E", "-x", "c"]);
    command.arg(match target {
        TargetArch::Aarch64 => "--target=aarch64-apple-darwin",
        TargetArch::X86_64 => "--target=x86_64-apple-darwin",
    });
    if let Some(sysroot) = sysroot {
        command.arg("--sysroot").arg(sysroot);
    }
    command.arg("-include").arg(path);
    for include in include_paths {
        command.arg("-I").arg(include);
    }
    for define in defines {
        command.arg(format!("-D{define}"));
    }
    command.arg("-");
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        format!("cannot start clang macro pass for @cimport('{path}'): {error}")
    })?;
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .ok_or("clang stdin unavailable")?
        .write_all(b"\n")
        .map_err(|error| format!("cannot provide clang macro input: {error}"))?;
    let result = child
        .wait_with_output()
        .map_err(|error| format!("cannot read clang macro output: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "clang failed while importing macros from '{path}': {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let existing = existing
        .iter()
        .filter_map(|node| match node {
            ASTNode::CompileTimeConstant { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut output = Vec::new();
    for line in String::from_utf8_lossy(&result.stdout).lines() {
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(name) = parts.next() else {
            continue;
        };
        if name.starts_with('_') || name.contains('(') || existing.contains(name) {
            continue;
        }
        let Some(value) = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let value = value
            .strip_suffix("U")
            .or_else(|| value.strip_suffix("L"))
            .unwrap_or(value);
        let expression = if let Some(negative) = value.strip_prefix('-') {
            if !negative.chars().all(|character| character.is_ascii_digit()) {
                continue;
            }
            Expression::Negate(Box::new(Expression::LiteralNumber(negative.to_string())))
        } else if let Some(hex) = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
        {
            let Ok(value) = u64::from_str_radix(hex.trim_end_matches(['u', 'U', 'l', 'L']), 16)
            else {
                continue;
            };
            Expression::LiteralNumber(value.to_string())
        } else if value.parse::<f64>().is_ok() {
            Expression::LiteralNumber(value.to_string())
        } else {
            continue;
        };
        output.push(ASTNode::CompileTimeConstant {
            name: name.to_string(),
            data_type: None,
            value: expression,
        });
    }
    Ok(output)
}

#[derive(Clone, Debug)]
struct CRecordLayout {
    size_bytes: u64,
    align_bytes: u64,
    is_union: bool,
    field_offsets: Vec<(String, u64)>,
    bit_fields: Vec<(String, u64, u16)>,
    fields_supported: bool,
}

fn import_record_layouts(
    clang: &std::ffi::OsStr,
    path: &str,
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> Result<HashMap<String, CRecordLayout>, String> {
    let mut command = Command::new(clang);
    command.args([
        "-Xclang",
        "-fdump-record-layouts-complete",
        "-fsyntax-only",
        "-x",
        "c",
    ]);
    command.arg(match target {
        TargetArch::Aarch64 => "--target=aarch64-apple-darwin",
        TargetArch::X86_64 => "--target=x86_64-apple-darwin",
    });
    if let Some(sysroot) = sysroot {
        command.arg("--sysroot").arg(sysroot);
    }
    command.arg("-include").arg(path);
    for include in include_paths {
        command.arg("-I").arg(include);
    }
    for define in defines {
        command.arg(format!("-D{define}"));
    }
    command.arg("-").stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        format!("cannot start clang layout pass for @cimport('{path}'): {error}")
    })?;
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .ok_or("clang layout stdin unavailable")?
        .write_all(b"\n")
        .map_err(|error| format!("cannot provide clang layout input: {error}"))?;
    let result = child
        .wait_with_output()
        .map_err(|error| format!("cannot read clang layout output: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "clang failed while querying C layouts from '{path}': {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    let layouts = parse_record_layouts(&format!("{stdout}\n{stderr}"))?;
    Ok(layouts)
}

fn parse_record_layouts(output: &str) -> Result<HashMap<String, CRecordLayout>, String> {
    let mut layouts = HashMap::new();
    let mut current: Option<(
        String,
        bool,
        Vec<(String, u64)>,
        Vec<(String, u64, u16)>,
        bool,
    )> = None;
    for line in output.lines() {
        let Some((offset, content)) = line.split_once('|') else {
            continue;
        };
        let raw_content = content;
        let content = raw_content.trim();
        let depth = raw_content.bytes().take_while(|byte| *byte == b' ').count();
        if depth <= 1 {
            let record = content
                .strip_prefix("struct ")
                .map(|record| (record, false))
                .or_else(|| content.strip_prefix("union ").map(|record| (record, true)));
            if let Some((record, is_union)) = record {
                current = Some((
                    record.trim().to_string(),
                    is_union,
                    Vec::new(),
                    Vec::new(),
                    true,
                ));
                continue;
            }
        }
        if let Some(metadata) = content.strip_prefix("[sizeof=") {
            let Some((size, align)) = metadata
                .strip_suffix(']')
                .and_then(|metadata| metadata.split_once(", align="))
            else {
                continue;
            };
            if let (
                Some((name, is_union, fields, bit_fields, fields_supported)),
                Ok(size_bytes),
                Ok(align_bytes),
            ) = (current.take(), size.parse::<u64>(), align.parse::<u64>())
            {
                layouts.insert(
                    name,
                    CRecordLayout {
                        size_bytes,
                        align_bytes,
                        is_union,
                        field_offsets: fields,
                        bit_fields,
                        fields_supported,
                    },
                );
            }
            continue;
        }
        let Some((_, _, fields, bit_fields, fields_supported)) = current.as_mut() else {
            continue;
        };
        if depth != 3 {
            continue;
        }
        let Some(field_name) = content.split_whitespace().last() else {
            *fields_supported = false;
            continue;
        };
        if let Some((byte_offset, range)) = offset.trim().split_once(':') {
            if let (Ok(byte_offset), Some((start, end))) =
                (byte_offset.parse::<u64>(), range.split_once('-'))
                && let (Ok(start), Ok(end)) = (start.parse::<u16>(), end.parse::<u16>())
            {
                fields.push((field_name.to_string(), byte_offset));
                if let Some(width) = end
                    .checked_sub(start)
                    .and_then(|width| width.checked_add(1))
                    .and_then(|width| u16::try_from(width).ok())
                {
                    let bit_offset = byte_offset
                        .checked_mul(8)
                        .and_then(|offset| offset.checked_add(u64::from(start)));
                    if let Some(bit_offset) = bit_offset {
                        bit_fields.push((field_name.to_string(), bit_offset, width));
                    } else {
                        *fields_supported = false;
                    }
                } else {
                    *fields_supported = false;
                }
            } else {
                *fields_supported = false;
            }
        } else if let Ok(offset) = offset.trim().parse::<u64>() {
            fields.push((field_name.to_string(), offset));
        } else {
            *fields_supported = false;
        }
    }
    Ok(layouts)
}

fn declarations_from_ast(
    root: &Json,
    header: &str,
    record_layouts: &HashMap<String, CRecordLayout>,
) -> Result<Vec<ASTNode>, String> {
    declarations_from_ast_with_shims(root, header, record_layouts).map(|(nodes, _)| nodes)
}

fn declarations_from_ast_with_shims(
    root: &Json,
    header: &str,
    record_layouts: &HashMap<String, CRecordLayout>,
) -> Result<(Vec<ASTNode>, Vec<String>), String> {
    let mut output = Vec::new();
    let mut generated_sources = Vec::new();
    let mut seen = HashSet::new();
    let mut alias_types = HashMap::new();
    let mut enum_underlying_types = HashMap::new();
    let Some(inner) = root.get("inner").and_then(Json::array) else {
        return Err("clang AST has no translation unit declarations".into());
    };
    collect_enum_constants(inner, &mut output, &mut seen);
    let complete_records = inner
        .iter()
        .filter(|declaration| {
            declaration.get("kind").and_then(Json::string) == Some("RecordDecl")
                && declaration.get("completeDefinition").and_then(Json::bool) == Some(true)
                && matches!(
                    declaration.get("tagUsed").and_then(Json::string),
                    Some("struct" | "union")
                )
        })
        .filter_map(|declaration| declaration.get("name")?.string())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) == Some("RecordDecl")
            && let Some(name) = declaration.get("name").and_then(Json::string)
        {
            let name = Box::leak(name.to_string().into_boxed_str());
            alias_types
                .entry(name.to_string())
                .or_insert(DataType::Struct(name));
        }
    }
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("RecordDecl")
            || declaration.get("completeDefinition").and_then(Json::bool) == Some(true)
            || !matches!(
                declaration.get("tagUsed").and_then(Json::string),
                Some("struct" | "union")
            )
        {
            continue;
        }
        let Some(record_name) = declaration.get("name").and_then(Json::string) else {
            continue;
        };
        if complete_records.contains(record_name) || !seen.insert(format!("record:{record_name}")) {
            continue;
        }
        let record_name = Box::leak(record_name.to_string().into_boxed_str());
        alias_types.insert(record_name.to_string(), DataType::Struct(record_name));
        output.push(ASTNode::StructDefinition {
            name: record_name,
            parameters: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
        });
    }
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("TypedefDecl") {
            continue;
        }
        if let (Some(name), Some(ty)) = (
            declaration.get("name").and_then(Json::string),
            declaration
                .get("type")
                .and_then(|ty| ty.get("desugaredQualType").or_else(|| ty.get("qualType")))
                .and_then(Json::string),
        ) && let Some(data_type) = c_type(ty, &alias_types)
        {
            alias_types.insert(name.to_string(), data_type);
        }
    }
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("EnumDecl") {
            continue;
        }
        let Some(name) = declaration.get("name").and_then(Json::string) else {
            continue;
        };
        let data_type = declaration
            .get("fixedUnderlyingType")
            .and_then(|ty| ty.get("qualType"))
            .and_then(Json::string)
            .and_then(|name| c_scalar_type(name.trim()))
            .unwrap_or(DataType::I32);
        let enum_name = Box::leak(name.to_string().into_boxed_str());
        let enum_type = DataType::CEnum(enum_name);
        output.push(ASTNode::CTypeInfo {
            name: name.to_string(),
            c_type: crate::c_type::from_clang_spelling(&format!("enum {name}")),
        });
        alias_types.insert(name.to_string(), enum_type);
        enum_underlying_types.insert(name.to_string(), data_type);
        let variants = declaration
            .get("inner")
            .and_then(Json::array)
            .into_iter()
            .flatten()
            .filter(|node| node.get("kind").and_then(Json::string) == Some("EnumConstantDecl"))
            .filter_map(|node| {
                node.get("name")?
                    .string()
                    .map(|name| (name.to_string(), None))
            })
            .collect();
        output.push(ASTNode::EnumDefinition {
            name: enum_name,
            parameters: Vec::new(),
            variants,
        });
        output.push(ASTNode::CTypeEnumRepresentation {
            name: name.to_string(),
            underlying_type: data_type,
        });
    }
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("RecordDecl")
            || declaration.get("completeDefinition").and_then(Json::bool) != Some(true)
            || !matches!(
                declaration.get("tagUsed").and_then(Json::string),
                Some("struct" | "union")
            )
        {
            continue;
        }
        let Some(record_name) = declaration.get("name").and_then(Json::string) else {
            continue;
        };
        let record_name = Box::leak(record_name.to_string().into_boxed_str());
        seen.insert(format!("record:{record_name}"));
        alias_types.insert(record_name.to_string(), DataType::Struct(record_name));
        let Some(fields) = declaration.get("inner").and_then(Json::array) else {
            continue;
        };
        let mut imported_fields = Vec::new();
        for field in fields {
            if field.get("kind").and_then(Json::string) != Some("FieldDecl") {
                continue;
            }
            let (Some(field_name), Some(type_name)) = (
                field.get("name").and_then(Json::string),
                field
                    .get("type")
                    .and_then(|ty| ty.get("qualType"))
                    .and_then(Json::string),
            ) else {
                imported_fields.clear();
                break;
            };
            let Some(data_type) = c_type(type_name, &alias_types) else {
                imported_fields.clear();
                break;
            };
            output.push(ASTNode::CTypeInfo {
                name: format!("{record_name}.{field_name}"),
                c_type: crate::c_type::from_clang_spelling(type_name),
            });
            imported_fields.push((field_name.to_string(), data_type, None, true));
        }
        let layout = record_layouts.get(record_name);
        let unsupported_bit_field = layout.is_some_and(|layout| {
            layout.bit_fields.iter().any(|(field, offset, width)| {
                imported_fields
                    .iter()
                    .find(|(name, _, _, _)| name == field)
                    .is_none_or(|(_, data_type, _, _)| {
                        let capacity = match data_type {
                            DataType::U8 | DataType::I8 => 8,
                            DataType::U16 | DataType::I16 => 16,
                            DataType::U32 | DataType::I32 => 32,
                            DataType::U64 | DataType::I64 => 64,
                            _ => 0,
                        };
                        capacity == 0
                            || *width == 0
                            || *width > capacity
                            || *offset + u64::from(*width) > layout.size_bytes * 8
                    })
            })
        });
        let fields_supported = !imported_fields.is_empty()
            && layout.is_some_and(|layout| {
                layout.fields_supported
                    && layout.field_offsets.len() == imported_fields.len()
                    && imported_fields.iter().all(|(field, _, _, _)| {
                        layout.field_offsets.iter().any(|(name, _)| name == field)
                    })
                    && !unsupported_bit_field
            });
        let fields = if fields_supported {
            imported_fields
        } else {
            Vec::new()
        };
        output.push(ASTNode::StructDefinition {
            name: record_name,
            parameters: Vec::new(),
            fields,
            methods: Vec::new(),
        });
        if let Some(layout) = layout {
            output.push(ASTNode::CTypeRecordLayout {
                name: record_name.to_string(),
                size_bytes: layout.size_bytes,
                align_bytes: layout.align_bytes,
                is_union: layout.is_union,
                field_offsets: layout.field_offsets.clone(),
                bit_fields: if fields_supported {
                    layout.bit_fields.clone()
                } else {
                    Vec::new()
                },
            });
        }
    }
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("TypedefDecl") {
            continue;
        }
        if let (Some(name), Some(ty)) = (
            declaration.get("name").and_then(Json::string),
            declaration
                .get("type")
                .and_then(|ty| ty.get("desugaredQualType").or_else(|| ty.get("qualType")))
                .and_then(Json::string),
        ) && let Some(data_type) = c_type(ty, &alias_types)
        {
            alias_types.insert(name.to_string(), data_type);
            output.push(ASTNode::CTypeInfo {
                name: name.to_string(),
                c_type: crate::c_type::from_clang_spelling(ty),
            });
            output.push(ASTNode::CTypeAlias {
                name: name.to_string(),
                data_type,
            });
        }
    }
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("FunctionDecl")
            || declaration.get("isImplicit").and_then(Json::bool) == Some(true)
        {
            continue;
        }
        let Some(name) = declaration.get("name").and_then(Json::string) else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            continue;
        }
        let children = declaration
            .get("inner")
            .and_then(Json::array)
            .unwrap_or_default();
        let mut parameters = Vec::new();
        let mut parameter_c_types = Vec::new();
        let mut parameter_qualifiers = Vec::new();
        let mut has_aggregate_parameter = false;
        let mut has_context_callback = false;
        let mut supported = true;
        let parameter_children = children
            .iter()
            .filter(|child| child.get("kind").and_then(Json::string) == Some("ParmVarDecl"))
            .collect::<Vec<_>>();
        let mut index = 0;
        while index < parameter_children.len() {
            let child = parameter_children[index];
            let Some(type_name) = child
                .get("type")
                .and_then(|ty| ty.get("qualType"))
                .and_then(Json::string)
            else {
                supported = false;
                break;
            };
            let Some(mut data_type) = c_type(type_name, &alias_types) else {
                supported = false;
                break;
            };
            let mut consumes_context = false;
            if let DataType::Pointer(function) = data_type
                && let DataType::Function(callback_parameters, result) = *function
                && callback_parameters.last().is_some_and(|parameter| {
                    matches!(parameter, DataType::Pointer(inner) if **inner == DataType::Void)
                })
                && parameter_children.get(index + 1).is_some_and(|context| {
                    context
                        .get("type")
                        .and_then(|ty| ty.get("qualType"))
                        .and_then(Json::string)
                        .and_then(|name| c_type(name, &alias_types))
                        .is_some_and(|ty| matches!(ty, DataType::Pointer(inner) if *inner == DataType::Void))
                })
            {
                data_type = DataType::Function(
                    Box::leak(callback_parameters[..callback_parameters.len() - 1].to_vec().into_boxed_slice()),
                    result,
                );
                consumes_context = true;
                has_context_callback = true;
            }
            let abi_type = c_abi_type(data_type, &enum_underlying_types);
            let is_aggregate = is_complete_c_aggregate(abi_type, record_layouts);
            has_aggregate_parameter |= is_aggregate;
            if !is_supported_c_abi_type(abi_type)
                && !is_supported_c_callback_abi_type(abi_type, record_layouts)
                && !matches!(data_type, DataType::Function(_, _))
                && !is_aggregate
            {
                supported = false;
                break;
            }
            let parameter_name = child
                .get("name")
                .and_then(Json::string)
                .map(str::to_string)
                .unwrap_or_else(|| format!("arg{index}"));
            parameters.push((parameter_name, data_type));
            parameter_c_types.push(type_name.to_string());
            parameter_qualifiers.push(c_qualifiers(type_name));
            if consumes_context {
                let context = parameter_children[index + 1];
                let context_type = context
                    .get("type")
                    .and_then(|ty| ty.get("qualType"))
                    .and_then(Json::string)
                    .expect("validated callback context parameter type");
                parameter_c_types.push(context_type.to_string());
                index += 1;
            }
            index += 1;
        }
        let Some(type_name) = declaration
            .get("type")
            .and_then(|ty| ty.get("qualType"))
            .and_then(Json::string)
        else {
            continue;
        };
        output.push(ASTNode::CTypeInfo {
            name: name.to_string(),
            c_type: crate::c_type::from_clang_spelling(type_name),
        });
        let variadic = type_name.contains("...");
        let calling_convention = declaration
            .get("callingConv")
            .and_then(Json::string)
            .unwrap_or("cdecl")
            .to_string();
        if !matches!(calling_convention.as_str(), "cdecl" | "C") {
            continue;
        }
        let Some(return_name) = function_return_type(type_name) else {
            continue;
        };
        let Some(return_type) = c_type(return_name, &alias_types) else {
            continue;
        };
        let return_qualifiers = c_qualifiers(return_name);
        let abi_return_type = c_abi_type(return_type, &enum_underlying_types);
        let aggregate_return = is_complete_c_aggregate(abi_return_type, record_layouts);
        if !supported
            || (has_context_callback && (has_aggregate_parameter || aggregate_return))
            || (!is_supported_c_abi_type(abi_return_type) && !aggregate_return)
        {
            continue;
        }
        let mut link_name = name.to_string();
        if has_aggregate_parameter || aggregate_return {
            let wrapper = c_aggregate_thunk_name(header, name);
            generated_sources.push(generate_c_aggregate_thunk(
                header,
                name,
                &wrapper,
                return_name,
                aggregate_return,
                &parameter_c_types,
                &parameters,
                record_layouts,
            ));
            link_name = wrapper;
        }
        output.push(ASTNode::ExternalFunction {
            name: name.to_string(),
            link_name: Some(link_name),
            parameters,
            return_type,
            variadic,
            calling_convention,
            parameter_qualifiers,
            return_qualifiers,
        });
    }
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("VarDecl")
            || declaration.get("storageClass").and_then(Json::string) != Some("extern")
        {
            continue;
        }
        let (Some(name), Some(type_name)) = (
            declaration.get("name").and_then(Json::string),
            declaration
                .get("type")
                .and_then(|ty| ty.get("qualType"))
                .and_then(Json::string),
        ) else {
            continue;
        };
        output.push(ASTNode::CTypeInfo {
            name: name.to_string(),
            c_type: crate::c_type::from_clang_spelling(type_name),
        });
        let Some(data_type) = c_type(type_name, &alias_types) else {
            continue;
        };
        let thread_local = declaration.get("tls").is_some();
        let abi_type = c_abi_type(data_type, &enum_underlying_types);
        let complete_aggregate = matches!(data_type, DataType::Struct(name)
            if record_layouts.contains_key(name));
        if (!is_supported_c_abi_type(abi_type) && !complete_aggregate) || abi_type == DataType::Void
        {
            continue;
        }
        let link_name = if thread_local {
            let accessor = format!("__mimi_tls_addr_{name}");
            generated_sources.push(format!(
                "#include \"{header}\"\nvoid *{accessor}(void) {{ return (void *)&{name}; }}\n"
            ));
            accessor
        } else {
            name.to_string()
        };
        output.push(ASTNode::ExternalGlobal {
            name: name.to_string(),
            link_name: Some(link_name),
            data_type,
            mutable: !c_object_is_const(type_name),
            thread_local,
        });
    }
    let _ = header;
    Ok((output, generated_sources))
}

fn c_abi_type(data_type: DataType, enum_underlying_types: &HashMap<String, DataType>) -> DataType {
    match data_type {
        DataType::Enum(name) | DataType::CEnum(name) => enum_underlying_types
            .get(name)
            .copied()
            .unwrap_or(data_type),
        DataType::Pointer(inner) => DataType::Pointer(Box::leak(Box::new(c_abi_type(
            *inner,
            enum_underlying_types,
        )))),
        _ => data_type,
    }
}

fn c_object_is_const(type_name: &str) -> bool {
    let type_name = type_name.trim();
    if let Some((_, pointer_suffix)) = type_name.rsplit_once('*') {
        pointer_suffix
            .split_whitespace()
            .any(|qualifier| qualifier == "const")
    } else {
        type_name
            .split_whitespace()
            .any(|qualifier| qualifier == "const")
    }
}

fn c_qualifiers(type_name: &str) -> Vec<String> {
    type_name
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|word| {
            matches!(
                *word,
                "const" | "volatile" | "restrict" | "_Nonnull" | "_Nullable" | "_Null_unspecified"
            )
        })
        .map(str::to_string)
        .collect()
}

fn is_complete_c_aggregate(
    data_type: DataType,
    record_layouts: &HashMap<String, CRecordLayout>,
) -> bool {
    matches!(data_type, DataType::Struct(name) if record_layouts.contains_key(name))
}

fn c_aggregate_thunk_name(header: &str, function: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    header.hash(&mut hasher);
    function.hash(&mut hasher);
    format!("mimi_cabi_{:016x}", hasher.finish())
}

fn generate_c_aggregate_thunk(
    header: &str,
    function: &str,
    wrapper: &str,
    return_c_type: &str,
    aggregate_return: bool,
    parameter_c_types: &[String],
    parameters: &[(String, DataType)],
    record_layouts: &HashMap<String, CRecordLayout>,
) -> String {
    let include = header.replace('\\', "\\\\").replace('"', "\\\"");
    let mut declarations = Vec::new();
    let mut arguments = Vec::new();
    if aggregate_return {
        declarations.push(format!("{return_c_type} *mimi_result"));
    }
    for (index, (c_type, (_, data_type))) in parameter_c_types.iter().zip(parameters).enumerate() {
        let argument = format!("mimi_arg_{index}");
        if is_complete_c_aggregate(*data_type, record_layouts) {
            declarations.push(c_parameter_declaration(&format!("{c_type} *"), &argument));
            arguments.push(format!("*{argument}"));
        } else {
            declarations.push(c_parameter_declaration(c_type, &argument));
            arguments.push(argument);
        }
    }
    let parameter_list = if declarations.is_empty() {
        "void".to_string()
    } else {
        declarations.join(", ")
    };
    let call = format!("{function}({})", arguments.join(", "));
    let body = if aggregate_return {
        format!("*mimi_result = {call};")
    } else if return_c_type == "void" {
        format!("{call};")
    } else {
        format!("return {call};")
    };
    let wrapper_return = if aggregate_return {
        "void"
    } else {
        return_c_type
    };
    format!("#include \"{include}\"\n{wrapper_return} {wrapper}({parameter_list}) {{ {body} }}\n")
}

fn c_parameter_declaration(c_type: &str, name: &str) -> String {
    if let Some(pointer) = c_type.find("(*") {
        format!("{}(*{}{}", &c_type[..pointer], name, &c_type[pointer + 2..])
    } else {
        format!("{c_type} {name}")
    }
}

fn collect_enum_constants(
    declarations: &[Json],
    output: &mut Vec<ASTNode>,
    seen: &mut HashSet<String>,
) {
    for declaration in declarations {
        match declaration.get("kind").and_then(Json::string) {
            Some("EnumDecl") => {
                let Some(constants) = declaration.get("inner").and_then(Json::array) else {
                    continue;
                };
                let mut next_value = 0i64;
                for constant in constants {
                    if constant.get("kind").and_then(Json::string) != Some("EnumConstantDecl") {
                        continue;
                    }
                    let Some(name) = constant.get("name").and_then(Json::string) else {
                        continue;
                    };
                    let value = constant
                        .get("inner")
                        .and_then(Json::array)
                        .and_then(|inner| find_integer_value(inner))
                        .unwrap_or(next_value);
                    next_value = value.saturating_add(1);
                    if value < 0 {
                        continue;
                    }
                    if !seen.insert(format!("constant:{name}")) {
                        continue;
                    }
                    let expression = Expression::LiteralNumber(value.to_string());
                    output.push(ASTNode::CompileTimeConstant {
                        name: name.to_string(),
                        data_type: Some(DataType::I32),
                        value: expression,
                    });
                }
                collect_enum_constants(constants, output, seen);
            }
            _ => {
                if let Some(inner) = declaration.get("inner").and_then(Json::array) {
                    collect_enum_constants(inner, output, seen);
                }
            }
        }
    }
}

fn find_integer_value(nodes: &[Json]) -> Option<i64> {
    for node in nodes {
        if let Some(value) = node.get("value") {
            let text = match value {
                Json::String(value) | Json::Number(value) => value,
                _ => "",
            };
            if let Ok(value) = text.parse() {
                return Some(value);
            }
        }
        if let Some(inner) = node.get("inner").and_then(Json::array)
            && let Some(value) = find_integer_value(inner)
        {
            return Some(value);
        }
    }
    None
}

fn function_return_type(signature: &str) -> Option<&str> {
    let open = signature.find('(')?;
    Some(signature[..open].trim())
}

fn c_type(type_name: &str, aliases: &HashMap<String, DataType>) -> Option<DataType> {
    let mut type_name = type_name.trim();
    for qualifier in ["const", "volatile", "restrict"] {
        if let Some(prefix) = type_name.strip_suffix(qualifier)
            && prefix.ends_with([' ', '*'])
        {
            type_name = prefix.trim_end();
        }
    }
    if let Some(open) = type_name.rfind('[')
        && let Some(length) = type_name[open + 1..]
            .strip_suffix(']')
            .and_then(|length| length.trim().parse::<u64>().ok())
    {
        let element = c_type(type_name[..open].trim(), aliases)?;
        return Some(DataType::Array(length, Box::leak(Box::new(element))));
    }
    if let Some(pointer) = type_name.find("(*") {
        let return_type = c_type(type_name[..pointer].trim(), aliases)?;
        let declarator = &type_name[pointer + 2..];
        let close = declarator.find(')')?;
        let parameters = declarator[close + 1..]
            .trim()
            .strip_prefix('(')?
            .strip_suffix(')')?;
        let parameters = if parameters.trim().is_empty() || parameters.trim() == "void" {
            Vec::new()
        } else {
            parameters
                .split(',')
                .map(|parameter| c_type(parameter.trim(), aliases))
                .collect::<Option<Vec<_>>>()?
        };
        return Some(DataType::Pointer(Box::leak(Box::new(DataType::Function(
            Box::leak(parameters.into_boxed_slice()),
            Box::leak(Box::new(return_type)),
        )))));
    }
    if let Some(inner) = type_name.strip_suffix('*') {
        return Some(DataType::Pointer(Box::leak(Box::new(c_type(
            inner, aliases,
        )?))));
    }
    let normalized = type_name
        .split_whitespace()
        .filter(|word| !matches!(*word, "const" | "volatile" | "restrict" | "_Atomic"))
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(data_type) = aliases.get(&normalized) {
        return Some(*data_type);
    }
    if let Some(name) = normalized
        .strip_prefix("struct ")
        .or_else(|| normalized.strip_prefix("union "))
        .or_else(|| normalized.strip_prefix("enum "))
    {
        return aliases.get(name).copied();
    }
    c_scalar_type(&normalized)
}

fn c_scalar_type(name: &str) -> Option<DataType> {
    Some(match name.trim() {
        "void" => DataType::Void,
        "_Bool" | "bool" => DataType::Bool,
        "char" | "signed char" => DataType::I8,
        "unsigned char" => DataType::U8,
        "short" | "short int" | "signed short" | "signed short int" => DataType::I16,
        "unsigned short" | "unsigned short int" => DataType::U16,
        "int" | "signed" | "signed int" => DataType::I32,
        "unsigned" | "unsigned int" => DataType::U32,
        "long" | "long int" | "signed long" | "signed long int" => DataType::I64,
        "unsigned long" | "unsigned long int" => DataType::U64,
        "long long" | "long long int" | "signed long long" | "signed long long int" => {
            DataType::I64
        }
        "unsigned long long" | "unsigned long long int" => DataType::U64,
        "float" => DataType::F32,
        "double" => DataType::F64,
        _ => return None,
    })
}

fn is_supported_c_abi_type(data_type: DataType) -> bool {
    match data_type {
        DataType::Pointer(inner) => match *inner {
            DataType::Function(parameters, result) => {
                parameters
                    .iter()
                    .all(|data_type| is_supported_c_scalar_abi_type(*data_type))
                    && is_supported_c_scalar_abi_type(*result)
            }
            DataType::Array(_, _) | DataType::Slice(_) => false,
            _ => true,
        },
        DataType::U8
        | DataType::U16
        | DataType::U32
        | DataType::U64
        | DataType::I8
        | DataType::I16
        | DataType::I32
        | DataType::I64
        | DataType::F16
        | DataType::F32
        | DataType::F64
        | DataType::Bool
        | DataType::Void => true,
        _ => false,
    }
}

fn is_supported_c_callback_abi_type(
    data_type: DataType,
    record_layouts: &HashMap<String, CRecordLayout>,
) -> bool {
    let DataType::Pointer(inner) = data_type else {
        return false;
    };
    let DataType::Function(parameters, result) = *inner else {
        return false;
    };
    let has_aggregate = parameters
        .iter()
        .any(|ty| is_complete_c_aggregate(*ty, record_layouts))
        || is_complete_c_aggregate(*result, record_layouts);
    has_aggregate
        && parameters.iter().all(|ty| {
            is_supported_c_scalar_abi_type(*ty) || is_complete_c_aggregate(*ty, record_layouts)
        })
        && (is_supported_c_scalar_abi_type(*result)
            || is_complete_c_aggregate(*result, record_layouts))
}

fn is_supported_c_scalar_abi_type(data_type: DataType) -> bool {
    matches!(
        data_type,
        DataType::U8
            | DataType::U16
            | DataType::U32
            | DataType::U64
            | DataType::I8
            | DataType::I16
            | DataType::I32
            | DataType::I64
            | DataType::F16
            | DataType::F32
            | DataType::F64
            | DataType::Bool
            | DataType::Void
    ) || matches!(data_type, DataType::Pointer(inner) if !matches!(*inner, DataType::Function(_, _) | DataType::Array(_, _) | DataType::Slice(_)))
}

#[cfg(test)]
mod tests {
    use super::{
        CRecordLayout, Json, JsonParser, c_type, declarations_from_ast, import_record_layouts,
        parse_record_layouts,
    };
    use crate::lir::TargetArch;
    use crate::parser::{ASTNode, DataType, Expression};
    use std::collections::HashMap;

    #[test]
    fn parses_clang_json_ast_structurally() {
        let value =
            JsonParser::parse(r#"{"kind":"FunctionDecl","name":"abs","enabled":true}"#).unwrap();
        assert_eq!(
            value.get("kind").and_then(Json::string),
            Some("FunctionDecl")
        );
        assert_eq!(value.get("enabled").and_then(Json::bool), Some(true));
    }

    #[test]
    fn maps_c_scalars_and_pointer_types() {
        assert_eq!(c_type("int", &HashMap::new()), Some(DataType::I32));
        assert_eq!(c_type("long double", &HashMap::new()), None);
        assert!(matches!(
            c_type("unsigned char[16]", &HashMap::new()),
            Some(DataType::Array(16, element)) if *element == DataType::U8
        ));
        assert_eq!(
            c_type("const char *", &HashMap::new()),
            Some(DataType::Pointer(Box::leak(Box::new(DataType::I8))))
        );
        assert!(matches!(
            c_type("int (*)(int, const char *)", &HashMap::new()),
            Some(DataType::Pointer(function))
                if matches!(*function, DataType::Function(parameters, return_type)
                    if parameters == &[DataType::I32, DataType::Pointer(Box::leak(Box::new(DataType::I8)))]
                        && *return_type == DataType::I32)
        ));
        assert!(matches!(
            c_type("void (* _Nonnull)(void)", &HashMap::new()),
            Some(DataType::Pointer(function))
                if matches!(*function, DataType::Function(parameters, return_type)
                    if parameters.is_empty() && *return_type == DataType::Void)
        ));
    }

    #[test]
    fn parses_clang_record_layouts_with_mixed_field_widths() {
        let layouts = parse_record_layouts(
            "\n         0 | struct Point\n         0 |   char x\n         4 |   int y\n         8 |   double z\n           | [sizeof=16, align=8]\n",
        )
        .unwrap();
        let point = layouts.get("Point").unwrap();
        assert_eq!(point.size_bytes, 16);
        assert_eq!(point.align_bytes, 8);
        assert_eq!(
            point.field_offsets,
            [("x".into(), 0), ("y".into(), 4), ("z".into(), 8)]
        );
    }

    #[test]
    fn queries_record_layouts_for_each_supported_target() {
        let directory =
            std::env::temp_dir().join(format!("mimi_c_layout_targets_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let header = directory.join("layout.h");
        std::fs::write(
            &header,
            "struct TargetLayout { char tag; long double value; };\n",
        )
        .unwrap();
        let clang = std::env::var_os("MIMI_CLANG").unwrap_or("clang".into());
        for (target, expected_size, expected_align, expected_offset) in [
            (TargetArch::X86_64, 32, 16, 16),
            (TargetArch::Aarch64, 16, 8, 8),
        ] {
            let layouts =
                import_record_layouts(&clang, header.to_str().unwrap(), target, &[], &[], None)
                    .unwrap();
            let record = layouts.get("TargetLayout").unwrap();
            assert_eq!(record.size_bytes, expected_size);
            assert_eq!(record.align_bytes, expected_align);
            assert_eq!(
                record.field_offsets,
                [("tag".into(), 0), ("value".into(), expected_offset)]
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn parses_nested_record_fields_without_restarting_the_outer_record() {
        let layouts = parse_record_layouts(
            "         0 | struct Outer\n         0 |   struct Inner inner\n         8 |   int tail\n           | [sizeof=12, align=4]\n",
        )
        .unwrap();
        let outer = layouts.get("Outer").unwrap();
        assert_eq!(outer.size_bytes, 12);
        assert!(outer.fields_supported);
        assert_eq!(
            outer.field_offsets,
            [("inner".into(), 0), ("tail".into(), 8)]
        );
    }

    #[test]
    fn parses_byte_local_bit_field_ranges() {
        let layouts = parse_record_layouts(
            "         0 | struct Flags\n     0:0-2 |   unsigned int low\n     0:3-7 |   unsigned int high\n           | [sizeof=4, align=4]\n",
        )
        .unwrap();
        let flags = layouts.get("Flags").unwrap();
        assert_eq!(flags.field_offsets, [("low".into(), 0), ("high".into(), 0)]);
        assert_eq!(
            flags.bit_fields,
            [("low".into(), 0, 3), ("high".into(), 3, 5)]
        );
    }

    #[test]
    fn parses_bit_fields_crossing_storage_unit_boundaries() {
        let layouts = parse_record_layouts(
            "         0 | struct Crossing\n     0:28-35 |   unsigned int value\n           | [sizeof=8, align=4]\n",
        )
        .unwrap();
        let crossing = layouts.get("Crossing").unwrap();
        assert_eq!(crossing.bit_fields, [("value".into(), 28, 8)]);
        assert!(crossing.fields_supported);
    }

    #[test]
    fn marks_zero_width_bitfields_as_unsupported() {
        let layouts = parse_record_layouts(
            "         0 | struct Bad\n     0:0-2 |   unsigned int a\n       4:- |   unsigned int\n     4:0-2 |   unsigned int b\n           | [sizeof=8, align=4]\n",
        )
        .unwrap();
        assert!(!layouts.get("Bad").unwrap().fields_supported);
    }

    #[test]
    fn preserves_named_c_enum_and_its_integer_representation() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"EnumDecl","name":"Color","inner":[{"kind":"EnumConstantDecl","name":"RED"}]},{"kind":"FunctionDecl","name":"echo_color","type":{"qualType":"enum Color (enum Color)"},"inner":[{"kind":"ParmVarDecl","name":"value","type":{"qualType":"enum Color"}}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "color.h", &HashMap::new()).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::EnumDefinition { name, variants, .. }
                if *name == "Color" && variants == &[("RED".into(), None)]
        )));
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::CTypeEnumRepresentation { name, underlying_type: DataType::I32 }
                if name == "Color"
        )));
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::CTypeInfo { name, c_type: crate::c_type::CType::Function { parameters, result, .. } }
                if name == "echo_color"
                    && parameters == &[crate::c_type::CType::Enum("Color".into())]
                    && **result == crate::c_type::CType::Enum("Color".into())
        )));
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::ExternalFunction { name, parameters, return_type: DataType::CEnum("Color"), .. }
                if name == "echo_color" && parameters == &[("value".into(), DataType::CEnum("Color"))]
        )));
    }

    #[test]
    fn preserves_c_function_calling_convention_and_type_qualifiers() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"FunctionDecl","name":"read_value","callingConv":"cdecl","type":{"qualType":"const int (const int *restrict, volatile int)"},"inner":[{"kind":"ParmVarDecl","name":"data","type":{"qualType":"const int *restrict"}},{"kind":"ParmVarDecl","name":"state","type":{"qualType":"volatile int"}}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "qualifiers.h", &HashMap::new()).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::ExternalFunction {
                name,
                calling_convention,
                parameter_qualifiers,
                return_qualifiers,
                ..
            } if name == "read_value"
                && calling_convention == "cdecl"
                && parameter_qualifiers == &[vec!["const".to_string(), "restrict".to_string()], vec!["volatile".to_string()]]
                && return_qualifiers == &["const".to_string()]
        )));
    }

    #[test]
    fn extracts_only_supported_function_declarations() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"FunctionDecl","name":"abs","mangledName":"_abs","type":{"qualType":"int (int)"},"inner":[{"kind":"ParmVarDecl","name":"value","type":{"qualType":"int"}}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "stdlib.h", &HashMap::new()).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::ExternalFunction { name, link_name: Some(link), parameters, return_type: DataType::I32, .. }
                if name == "abs" && link == "abs" && parameters == &[("value".into(), DataType::I32)]
        )));
    }

    #[test]
    fn extracts_enum_constants_with_implicit_values() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"EnumDecl","inner":[{"kind":"EnumConstantDecl","name":"first"},{"kind":"EnumConstantDecl","name":"second","inner":[{"kind":"ConstantExpr","value":"7"}]},{"kind":"EnumConstantDecl","name":"third"}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "test.h", &HashMap::new()).unwrap();
        assert!(matches!(
            &declarations[0],
            ASTNode::CompileTimeConstant { name, data_type: Some(DataType::I32), value: Expression::LiteralNumber(value) }
                if name == "first" && value == "0"
        ));
        assert!(matches!(
            &declarations[1],
            ASTNode::CompileTimeConstant { name, value: Expression::LiteralNumber(value), .. }
                if name == "second" && value == "7"
        ));
        assert!(matches!(
            &declarations[2],
            ASTNode::CompileTimeConstant { name, value: Expression::LiteralNumber(value), .. }
                if name == "third" && value == "8"
        ));
    }

    #[test]
    fn imports_forward_declared_records_for_pointer_signatures() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"RecordDecl","name":"Opaque","tagUsed":"struct"},{"kind":"FunctionDecl","name":"use_opaque","type":{"qualType":"int (struct Opaque *)"},"inner":[{"kind":"ParmVarDecl","name":"value","type":{"qualType":"struct Opaque *"}}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "opaque.h", &HashMap::new()).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::StructDefinition { name, fields, .. } if *name == "Opaque" && fields.is_empty()
        )));
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::ExternalFunction { name, parameters, .. }
                if name == "use_opaque"
                    && parameters == &[("value".into(), DataType::Pointer(Box::leak(Box::new(DataType::Struct("Opaque")))))]
        )));
    }

    #[test]
    fn keeps_unsupported_complete_records_opaque_for_pointer_apis() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"RecordDecl","name":"Event","tagUsed":"struct","completeDefinition":true,"inner":[{"kind":"FieldDecl","name":"data","type":{"qualType":"long double"}}]},{"kind":"FunctionDecl","name":"poll_event","type":{"qualType":"int (struct Event *)"},"inner":[{"kind":"ParmVarDecl","name":"event","type":{"qualType":"struct Event *"}}]}]}"#,
        )
        .unwrap();
        let layouts = HashMap::from([(
            "Event".to_string(),
            CRecordLayout {
                size_bytes: 8,
                align_bytes: 4,
                is_union: false,
                field_offsets: vec![("data".to_string(), 0)],
                bit_fields: Vec::new(),
                fields_supported: true,
            },
        )]);
        let declarations = declarations_from_ast(&json, "event.h", &layouts).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            ASTNode::StructDefinition { name, fields, .. } if *name == "Event" && fields.is_empty()
        )));
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            ASTNode::ExternalFunction { name, parameters, .. }
                if name == "poll_event" && matches!(parameters.as_slice(), [(_, DataType::Pointer(inner))] if **inner == DataType::Struct("Event"))
        )));
    }

    #[test]
    fn distinguishes_pointer_qualifiers_on_c_globals() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"VarDecl","name":"pointee_const","storageClass":"extern","type":{"qualType":"const int *"}},{"kind":"VarDecl","name":"pointer_const","storageClass":"extern","type":{"qualType":"int *const"}}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "qualifiers.h", &HashMap::new()).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            ASTNode::ExternalGlobal { name, mutable: true, .. } if name == "pointee_const"
        )));
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            ASTNode::ExternalGlobal { name, mutable: false, .. } if name == "pointer_const"
        )));
    }

    #[test]
    fn imports_scalar_callbacks_and_variadic_declarations() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"FunctionDecl","name":"take_record","type":{"qualType":"void (struct Record)"},"inner":[{"kind":"ParmVarDecl","name":"record","type":{"qualType":"struct Record"}}]},{"kind":"FunctionDecl","name":"take_callback","type":{"qualType":"void (int (*)(int))"},"inner":[{"kind":"ParmVarDecl","name":"callback","type":{"qualType":"int (*)(int)"}}]},{"kind":"FunctionDecl","name":"printf","type":{"qualType":"int (const char *, ...)"},"inner":[{"kind":"ParmVarDecl","name":"format","type":{"qualType":"const char *"}}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "abi.h", &HashMap::new()).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            ASTNode::ExternalFunction { name, parameters, .. }
                if name == "take_callback"
                    && matches!(parameters.as_slice(), [(_, DataType::Pointer(inner))]
                        if matches!(**inner, DataType::Function(args, result)
                            if args == &[DataType::I32] && *result == DataType::I32))
        )));
        assert!(!declarations.iter().any(|declaration| matches!(
            declaration,
            ASTNode::ExternalFunction { name, .. } if name == "take_record"
        )));
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            ASTNode::ExternalFunction { name, variadic: true, .. } if name == "printf"
        )));
    }
}
