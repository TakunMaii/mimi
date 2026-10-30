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

pub(crate) fn expand(
    nodes: &[ASTNode],
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> Result<Vec<ASTNode>, String> {
    let mut output = nodes.to_vec();
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
    for (alias, path) in imports {
        let declarations = import_header(&path, target, include_paths, &defines, sysroot)?;
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
                ASTNode::CompileTimeConstant { name, .. } => {
                    *name = format!("{alias}.{name}");
                }
                ASTNode::CTypeAlias { name, data_type } => {
                    *name = format!("{alias}.{name}");
                    *data_type = qualify_c_type(*data_type, &alias);
                }
                ASTNode::CTypeRecordLayout { name, .. } => {
                    *name = format!("{alias}.{name}");
                }
                _ => {}
            }
            node
        }));
    }
    let aliases = output
        .iter()
        .filter_map(|node| match node {
            ASTNode::CTypeAlias { name, data_type } => Some((name.clone(), *data_type)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    rewrite_c_aliases(&mut output, &aliases);
    Ok(output)
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

fn import_header(
    path: &str,
    target: TargetArch,
    include_paths: &[std::path::PathBuf],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
) -> Result<Vec<ASTNode>, String> {
    let clang = std::env::var_os("MIMI_CLANG").unwrap_or("clang".into());
    let cache_key = header_cache_key(&clang, path, target, include_paths, defines, sysroot);
    static CACHE: OnceLock<Mutex<HashMap<u64, Vec<ASTNode>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(declarations) = cache.get(&cache_key)
    {
        return Ok(declarations.clone());
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
    let mut declarations = declarations_from_ast(&json, path, &record_layouts)?;
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
        cache.insert(cache_key, declarations.clone());
    }
    Ok(declarations)
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
    bit_fields: Vec<(String, u8, u8)>,
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
        Vec<(String, u8, u8)>,
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
                && let (Ok(start), Ok(end)) = (start.parse::<u8>(), end.parse::<u8>())
            {
                fields.push((field_name.to_string(), byte_offset));
                if let Some(width) = end
                    .checked_sub(start)
                    .and_then(|width| width.checked_add(1))
                {
                    bit_fields.push((field_name.to_string(), start, width));
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
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let mut alias_types = HashMap::new();
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
        alias_types.insert(name.to_string(), data_type);
        output.push(ASTNode::CTypeAlias {
            name: name.to_string(),
            data_type,
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
            imported_fields.push((field_name.to_string(), data_type, None, true));
        }
        if !imported_fields.is_empty() {
            let Some(layout) = record_layouts.get(record_name) else {
                continue;
            };
            if !layout.fields_supported || layout.field_offsets.len() != imported_fields.len() {
                continue;
            }
            let unsupported_bit_field = layout.bit_fields.iter().any(|(field, offset, width)| {
                *width == 0
                    || offset.saturating_add(*width) > 8
                    || imported_fields
                        .iter()
                        .find(|(name, _, _, _)| name == field)
                        .is_none_or(|(_, data_type, _, _)| {
                            !matches!(
                                data_type,
                                DataType::U8
                                    | DataType::U16
                                    | DataType::U32
                                    | DataType::U64
                                    | DataType::I8
                                    | DataType::I16
                                    | DataType::I32
                                    | DataType::I64
                            )
                        })
            });
            if unsupported_bit_field {
                continue;
            }
            if imported_fields
                .iter()
                .any(|(field, _, _, _)| !layout.field_offsets.iter().any(|(name, _)| name == field))
            {
                continue;
            }
            output.push(ASTNode::StructDefinition {
                name: record_name,
                parameters: Vec::new(),
                fields: imported_fields,
                methods: Vec::new(),
            });
            output.push(ASTNode::CTypeRecordLayout {
                name: record_name.to_string(),
                size_bytes: layout.size_bytes,
                align_bytes: layout.align_bytes,
                is_union: layout.is_union,
                field_offsets: layout.field_offsets.clone(),
                bit_fields: layout.bit_fields.clone(),
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
    for declaration in inner {
        if declaration.get("kind").and_then(Json::string) != Some("VarDecl")
            || declaration.get("storageClass").and_then(Json::string) != Some("extern")
            || declaration.get("tls").is_some()
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
        let Some(data_type) = c_type(type_name, &alias_types) else {
            continue;
        };
        if !is_supported_c_abi_type(data_type) || data_type == DataType::Void {
            continue;
        }
        output.push(ASTNode::ExternalGlobal {
            name: name.to_string(),
            link_name: Some(name.to_string()),
            data_type,
            mutable: !type_name.trim_start().starts_with("const "),
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
        DataType::Pointer(inner) => !matches!(
            *inner,
            DataType::Function(_, _) | DataType::Array(_, _) | DataType::Slice(_)
        ),
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

#[cfg(test)]
mod tests {
    use super::{Json, JsonParser, c_type, declarations_from_ast, parse_record_layouts};
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
    fn marks_zero_width_bitfields_as_unsupported() {
        let layouts = parse_record_layouts(
            "         0 | struct Bad\n     0:0-2 |   unsigned int a\n       4:- |   unsigned int\n     4:0-2 |   unsigned int b\n           | [sizeof=8, align=4]\n",
        )
        .unwrap();
        assert!(!layouts.get("Bad").unwrap().fields_supported);
    }

    #[test]
    fn imports_named_c_enum_as_its_default_integer_representation() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"EnumDecl","name":"Color","inner":[{"kind":"EnumConstantDecl","name":"RED"}]},{"kind":"FunctionDecl","name":"echo_color","type":{"qualType":"enum Color (enum Color)"},"inner":[{"kind":"ParmVarDecl","name":"value","type":{"qualType":"enum Color"}}]}]}"#,
        )
        .unwrap();
        let declarations = declarations_from_ast(&json, "color.h", &HashMap::new()).unwrap();
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::CTypeAlias { name, data_type: DataType::I32 } if name == "Color"
        )));
        assert!(declarations.iter().any(|declaration| matches!(declaration,
            ASTNode::ExternalFunction { name, parameters, return_type: DataType::I32, .. }
                if name == "echo_color" && parameters == &[("value".into(), DataType::I32)]
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
    fn skips_signatures_without_a_supported_c_abi() {
        let json = JsonParser::parse(
            r#"{"inner":[{"kind":"FunctionDecl","name":"take_record","type":{"qualType":"void (struct Record)"},"inner":[{"kind":"ParmVarDecl","name":"record","type":{"qualType":"struct Record"}}]},{"kind":"FunctionDecl","name":"take_callback","type":{"qualType":"void (int (*)(int))"},"inner":[{"kind":"ParmVarDecl","name":"callback","type":{"qualType":"int (*)(int)"}}]}]}"#,
        )
        .unwrap();
        assert!(
            declarations_from_ast(&json, "abi.h", &HashMap::new())
                .unwrap()
                .is_empty()
        );
    }
}
