use std::collections::{HashMap, HashSet};
use std::process::Command;

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

pub(crate) fn expand(
    nodes: &[ASTNode],
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> Result<Vec<ASTNode>, String> {
    let mut output = nodes.to_vec();
    let mut imports = Vec::new();
    for node in nodes {
        if let ASTNode::CImport { alias, path } = node {
            imports.push((alias.clone(), path.clone()));
        }
    }
    for (alias, path) in imports {
        let declarations = import_header(&path, target, include_paths, defines, sysroot)?;
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
                ASTNode::StructDefinition { name, fields, .. } => {
                    *name = Box::leak(format!("{alias}.{}", *name).into_boxed_str());
                    for (_, data_type, _, _) in fields {
                        *data_type = qualify_c_type(*data_type, &alias);
                    }
                }
                ASTNode::CompileTimeConstant { name, .. } => {
                    *name = format!("{alias}.{name}");
                }
                _ => {}
            }
            node
        }));
    }
    Ok(output)
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

fn import_header(
    path: &str,
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> Result<Vec<ASTNode>, String> {
    let mut command = Command::new(std::env::var_os("MIMI_CLANG").unwrap_or("clang".into()));
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
    let mut declarations = declarations_from_ast(&json, path)?;
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
    Ok(declarations)
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

fn declarations_from_ast(root: &Json, header: &str) -> Result<Vec<ASTNode>, String> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let mut alias_types = HashMap::new();
    let Some(inner) = root.get("inner").and_then(Json::array) else {
        return Err("clang AST has no translation unit declarations".into());
    };
    collect_enum_constants(inner, &mut output, &mut seen);
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("RecordDecl")
            || declaration.get("completeDefinition").and_then(Json::bool) != Some(true)
        {
            continue;
        }
        let Some(record_name) = declaration.get("name").and_then(Json::string) else {
            continue;
        };
        let record_name = Box::leak(record_name.to_string().into_boxed_str());
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
                continue;
            };
            let Some(data_type) = c_type(type_name, &alias_types) else {
                imported_fields.clear();
                break;
            };
            imported_fields.push((field_name.to_string(), data_type, None, true));
        }
        if !imported_fields.is_empty() {
            output.push(ASTNode::StructDefinition {
                name: record_name,
                parameters: Vec::new(),
                fields: imported_fields,
                methods: Vec::new(),
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
        ) && let Some(data_type) = c_scalar_type(ty)
        {
            alias_types.insert(name.to_string(), data_type);
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
        let Some(children) = declaration.get("inner").and_then(Json::array) else {
            continue;
        };
        let mut parameters = Vec::new();
        let mut supported = true;
        for (index, child) in children
            .iter()
            .filter(|child| child.get("kind").and_then(Json::string) == Some("ParmVarDecl"))
            .enumerate()
        {
            let Some(type_name) = child
                .get("type")
                .and_then(|ty| ty.get("qualType"))
                .and_then(Json::string)
            else {
                supported = false;
                break;
            };
            let Some(data_type) = c_type(type_name, &alias_types) else {
                supported = false;
                break;
            };
            let parameter_name = child
                .get("name")
                .and_then(Json::string)
                .map(str::to_string)
                .unwrap_or_else(|| format!("arg{index}"));
            parameters.push((parameter_name, data_type));
        }
        let Some(type_name) = declaration
            .get("type")
            .and_then(|ty| ty.get("qualType"))
            .and_then(Json::string)
        else {
            continue;
        };
        let Some(return_name) = function_return_type(type_name) else {
            continue;
        };
        let Some(return_type) = c_type(return_name, &alias_types) else {
            continue;
        };
        if !supported || !is_supported_c_abi_type(return_type) {
            continue;
        }
        if parameters
            .iter()
            .any(|(_, ty)| !is_supported_c_abi_type(*ty))
        {
            continue;
        }
        output.push(ASTNode::ExternalFunction {
            name: name.to_string(),
            link_name: Some(name.to_string()),
            parameters,
            return_type,
        });
    }
    let _ = header;
    Ok(output)
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
    let type_name = type_name.trim();
    if let Some(pointer) = type_name.find("(*") {
        let return_type = c_type(type_name[..pointer].trim(), aliases)?;
        let parameters = type_name[pointer + 2..]
            .strip_prefix(")(")?
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
        "double" | "long double" => DataType::F64,
        _ => return None,
    })
}

fn is_supported_c_abi_type(data_type: DataType) -> bool {
    !matches!(data_type, DataType::Pointer(inner) if matches!(*inner, DataType::Void))
}

#[cfg(test)]
mod tests {
    use super::{Json, JsonParser, c_type, declarations_from_ast};
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
    }

    #[test]
    fn extracts_only_supported_function_declarations() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"FunctionDecl","name":"abs","mangledName":"_abs","type":{"qualType":"int (int)"},"inner":[{"kind":"ParmVarDecl","name":"value","type":{"qualType":"int"}}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "stdlib.h").unwrap();
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::ExternalFunction { name, link_name: Some(link), parameters, return_type: DataType::I32 }
                if name == "abs" && link == "abs" && parameters == &[("value".into(), DataType::I32)]
        )));
    }

    #[test]
    fn extracts_enum_constants_with_implicit_values() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"EnumDecl","inner":[{"kind":"EnumConstantDecl","name":"first"},{"kind":"EnumConstantDecl","name":"second","inner":[{"kind":"ConstantExpr","value":"7"}]},{"kind":"EnumConstantDecl","name":"third"}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "test.h").unwrap();
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
}
