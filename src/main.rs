mod c_import;
mod codegen;
mod codegen_x86_64;
mod float8;
mod hir;
mod layout;
mod lexer;
mod lir;
mod mir;
mod parser;
mod regalloc;
mod type_checker;

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run_cli() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("build") => {
            let mut input = None;
            let mut output = None;
            let mut target = <lir::HostTarget as lir::Target>::arch(&lir::HostTarget);
            let mut import_paths = Vec::new();
            let mut library_paths = Vec::new();
            let mut libraries = Vec::new();
            let mut defines = Vec::new();
            let mut sysroot = None;
            let mut c_sources = Vec::new();
            while let Some(argument) = args.next() {
                if argument == "-o" {
                    output = Some(args.next().ok_or("missing output path after -o")?);
                } else if let Some(path) = argument.strip_prefix("-I") {
                    import_paths.push(std::path::PathBuf::from(if path.is_empty() {
                        args.next().ok_or("missing path after -I")?
                    } else {
                        path.to_string()
                    }));
                } else if let Some(path) = argument.strip_prefix("-L") {
                    library_paths.push(std::path::PathBuf::from(if path.is_empty() {
                        args.next().ok_or("missing path after -L")?
                    } else {
                        path.to_string()
                    }));
                } else if let Some(library) = argument.strip_prefix("-l") {
                    libraries.push(if library.is_empty() {
                        args.next().ok_or("missing library name after -l")?
                    } else {
                        library.to_string()
                    });
                } else if let Some(define) = argument.strip_prefix("-D") {
                    defines.push(if define.is_empty() {
                        args.next().ok_or("missing definition after -D")?
                    } else {
                        define.to_string()
                    });
                } else if argument == "--sysroot" {
                    sysroot = Some(std::path::PathBuf::from(
                        args.next().ok_or("missing path after --sysroot")?,
                    ));
                } else if let Some(path) = argument.strip_prefix("--sysroot=") {
                    sysroot = Some(std::path::PathBuf::from(path));
                } else if argument == "--c-source" {
                    c_sources.push(std::path::PathBuf::from(
                        args.next()
                            .ok_or("missing C source path after --c-source")?,
                    ));
                } else if let Some(path) = argument.strip_prefix("--c-source=") {
                    c_sources.push(std::path::PathBuf::from(path));
                } else if argument == "--target" {
                    target = parse_target(&args.next().ok_or("missing value after --target")?)?;
                } else if let Some(value) = argument.strip_prefix("--target=") {
                    target = parse_target(value)?;
                } else if input.is_none() {
                    input = Some(argument);
                } else {
                    return Err(
                        "usage: mimi build <source> -o <output> [--target host|aarch64|x86_64] [-Ipath] [-Lpath] [-lname] [-Dname[=value]] [--sysroot path] [--c-source file.c]".into(),
                    );
                }
            }
            build_file_for_target_with_c_options(
                &input.ok_or("missing source path")?,
                &output.ok_or("missing output path")?,
                &import_paths,
                &library_paths,
                &libraries,
                target,
                &defines,
                sysroot.as_deref(),
                &c_sources,
            )
        }
        _ => Err("usage: mimi build <source> -o <output>".into()),
    }
}

fn parse_target(value: &str) -> Result<lir::TargetArch, String> {
    match value {
        "host" => Ok(<lir::HostTarget as lir::Target>::arch(&lir::HostTarget)),
        "aarch64" | "arm64" => Ok(lir::TargetArch::Aarch64),
        "x86_64" | "amd64" => Ok(lir::TargetArch::X86_64),
        _ => Err(format!(
            "unsupported target '{value}'; expected host, aarch64, or x86_64"
        )),
    }
}

fn build_file(
    input: &str,
    output: &str,
    import_paths: &[std::path::PathBuf],
    library_paths: &[std::path::PathBuf],
    libraries: &[String],
) -> Result<(), String> {
    build_file_for_target(
        input,
        output,
        import_paths,
        library_paths,
        libraries,
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget),
    )
}

fn build_file_for_target(
    input: &str,
    output: &str,
    import_paths: &[std::path::PathBuf],
    library_paths: &[std::path::PathBuf],
    libraries: &[String],
    target: lir::TargetArch,
) -> Result<(), String> {
    build_file_for_target_with_c_options(
        input,
        output,
        import_paths,
        library_paths,
        libraries,
        target,
        &[],
        None,
        &[],
    )
}

fn build_file_for_target_with_c_options(
    input: &str,
    output: &str,
    import_paths: &[std::path::PathBuf],
    library_paths: &[std::path::PathBuf],
    libraries: &[String],
    target: lir::TargetArch,
    defines: &[String],
    sysroot: Option<&std::path::Path>,
    c_sources: &[std::path::PathBuf],
) -> Result<(), String> {
    let source = std::fs::read_to_string(input)
        .map_err(|error| format!("cannot read '{input}': {error}"))?;
    let mut import_paths = import_paths.to_vec();
    if let Some(parent) = std::path::Path::new(input).parent()
        && !import_paths.iter().any(|path| path == parent)
    {
        import_paths.push(parent.to_path_buf());
    }
    let ast = parser::parse(lexer::tokenize(source.chars()));
    let ast = c_import::expand(&ast, target, &import_paths, defines, sysroot)?;
    type_checker::check_with_import_paths(&ast, &import_paths).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let mut compilation_ast = ast.clone();
    collect_imported_functions(
        &ast,
        &import_paths,
        "",
        &mut std::collections::HashSet::new(),
        &mut compilation_ast,
    )?;
    let hir = hir::lower_ast(&compilation_ast);
    let layout = <lir::TargetArch as lir::Target>::data_layout(&target);
    let mir = mir::lower_hir_with_layout(&hir, layout);
    let lir = lir::lower_mir(&mir, &target);
    codegen::compile_executable_with_options(
        &lir,
        std::path::Path::new(output),
        &codegen::LinkOptions {
            library_paths: library_paths.to_vec(),
            libraries: libraries.to_vec(),
            c_sources: c_sources.to_vec(),
            include_paths: import_paths.clone(),
            defines: defines.to_vec(),
            sysroot: sysroot.map(std::path::Path::to_path_buf),
        },
    )
}

fn collect_imported_functions(
    nodes: &[parser::ASTNode],
    import_paths: &[std::path::PathBuf],
    namespace: &str,
    visiting: &mut std::collections::HashSet<std::path::PathBuf>,
    output: &mut Vec<parser::ASTNode>,
) -> Result<(), String> {
    for node in nodes {
        let parser::ASTNode::Import { alias, path } = node else {
            continue;
        };
        let source_path = type_checker::resolve_import_path(path, import_paths)?;
        let visit_key = source_path
            .canonicalize()
            .unwrap_or_else(|_| source_path.clone());
        if !visiting.insert(visit_key.clone()) {
            continue;
        }
        let source = std::fs::read_to_string(&source_path)
            .map_err(|error| format!("cannot read '{}': {error}", source_path.display()))?;
        let module_nodes = parser::parse(lexer::tokenize(source.chars()));
        let local_type_names = module_nodes
            .iter()
            .map(|node| match node {
                parser::ASTNode::Public(declaration) => declaration.as_ref(),
                declaration => declaration,
            })
            .filter_map(|node| match node {
                parser::ASTNode::StructDefinition { name, .. }
                | parser::ASTNode::EnumDefinition { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        let mut module_paths = import_paths.to_vec();
        if let Some(parent) = source_path.parent()
            && !module_paths.iter().any(|path| path == parent)
        {
            module_paths.push(parent.to_path_buf());
        }
        type_checker::check_with_import_paths(&module_nodes, &module_paths).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("\n")
        })?;

        let package = if namespace.is_empty() {
            alias.clone()
        } else {
            format!("{namespace}.{alias}")
        };
        for module_node in &module_nodes {
            let declaration = match module_node {
                parser::ASTNode::Public(declaration) => declaration.as_ref(),
                declaration => declaration,
            };
            if let parser::ASTNode::FunctionDefinition { name, .. } = declaration {
                let mut function =
                    qualify_imported_node(declaration.clone(), &package, &local_type_names);
                if let parser::ASTNode::FunctionDefinition {
                    name: function_name,
                    ..
                } = &mut function
                {
                    *function_name = format!("{package}.{name}");
                }
                output.push(function);
            }
            if let parser::ASTNode::ExternalFunction { name, .. } = declaration {
                let mut external =
                    qualify_imported_node(declaration.clone(), &package, &local_type_names);
                if let parser::ASTNode::ExternalFunction {
                    name: imported_name,
                    link_name,
                    ..
                } = &mut external
                {
                    *imported_name = format!("{package}.{name}");
                    *link_name = Some(name.clone());
                }
                output.push(external);
            }
            if let parser::ASTNode::CompileTimeConstant { name, .. } = declaration {
                let mut constant =
                    qualify_imported_node(declaration.clone(), &package, &local_type_names);
                if let parser::ASTNode::CompileTimeConstant {
                    name: imported_name,
                    ..
                } = &mut constant
                {
                    *imported_name = format!("{package}.{name}");
                }
                output.push(constant);
            }
            if matches!(
                declaration,
                parser::ASTNode::StructDefinition { .. } | parser::ASTNode::EnumDefinition { .. }
            ) {
                output.push(qualify_imported_node(
                    declaration.clone(),
                    &package,
                    &local_type_names,
                ));
            }
        }
        output.extend(imported_function_aliases(&module_nodes, &package));
        collect_imported_functions(&module_nodes, &module_paths, &package, visiting, output)?;
        visiting.remove(&visit_key);
    }
    Ok(())
}

fn qualify_imported_type(
    data_type: parser::DataType,
    package: &str,
    local_type_names: &std::collections::HashSet<String>,
) -> parser::DataType {
    match data_type {
        parser::DataType::GenericStruct(name, arguments) => {
            let name = if local_type_names.contains(name) || name.contains('.') {
                Box::leak(format!("{package}.{name}").into_boxed_str())
            } else {
                name
            };
            parser::DataType::GenericStruct(
                name,
                Box::leak(
                    arguments
                        .iter()
                        .map(|argument| {
                            qualify_imported_generic_argument(*argument, package, local_type_names)
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            )
        }
        parser::DataType::GenericEnum(name, arguments) => {
            let name = if local_type_names.contains(name) || name.contains('.') {
                Box::leak(format!("{package}.{name}").into_boxed_str())
            } else {
                name
            };
            parser::DataType::GenericEnum(
                name,
                Box::leak(
                    arguments
                        .iter()
                        .map(|argument| {
                            qualify_imported_generic_argument(*argument, package, local_type_names)
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            )
        }
        parser::DataType::Struct(name) if local_type_names.contains(name) || name.contains('.') => {
            parser::DataType::Struct(Box::leak(format!("{package}.{name}").into_boxed_str()))
        }
        parser::DataType::Enum(name) if local_type_names.contains(name) || name.contains('.') => {
            parser::DataType::Enum(Box::leak(format!("{package}.{name}").into_boxed_str()))
        }
        parser::DataType::Array(length, element) => parser::DataType::Array(
            length,
            Box::leak(Box::new(qualify_imported_type(
                *element,
                package,
                local_type_names,
            ))),
        ),
        parser::DataType::Slice(element) => parser::DataType::Slice(Box::leak(Box::new(
            qualify_imported_type(*element, package, local_type_names),
        ))),
        parser::DataType::Pointer(element) => parser::DataType::Pointer(Box::leak(Box::new(
            qualify_imported_type(*element, package, local_type_names),
        ))),
        parser::DataType::Function(parameters, result) => parser::DataType::Function(
            Box::leak(
                parameters
                    .iter()
                    .copied()
                    .map(|parameter| qualify_imported_type(parameter, package, local_type_names))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            Box::leak(Box::new(qualify_imported_type(
                *result,
                package,
                local_type_names,
            ))),
        ),
        other => other,
    }
}

fn qualify_imported_generic_argument(
    argument: parser::GenericArgument,
    package: &str,
    local_type_names: &std::collections::HashSet<String>,
) -> parser::GenericArgument {
    match argument {
        parser::GenericArgument::Type(data_type) => parser::GenericArgument::Type(
            qualify_imported_type(data_type, package, local_type_names),
        ),
        other => other,
    }
}

fn qualify_imported_node(
    mut node: parser::ASTNode,
    package: &str,
    local_type_names: &std::collections::HashSet<String>,
) -> parser::ASTNode {
    use parser::ASTNode;
    match &mut node {
        ASTNode::VariableDefinition {
            data_type, value, ..
        }
        | ASTNode::CompileTimeConstant {
            data_type, value, ..
        } => {
            *data_type = data_type.map(|ty| qualify_imported_type(ty, package, local_type_names));
            qualify_imported_expression(value, package, local_type_names);
        }
        ASTNode::Assignment { value, .. } | ASTNode::Expression(value) => {
            qualify_imported_expression(value, package, local_type_names);
        }
        ASTNode::IndexAssignment {
            object,
            index,
            value,
        } => {
            qualify_imported_expression(object, package, local_type_names);
            qualify_imported_expression(index, package, local_type_names);
            qualify_imported_expression(value, package, local_type_names);
        }
        ASTNode::PointerAssignment { pointer, value } => {
            qualify_imported_expression(pointer, package, local_type_names);
            qualify_imported_expression(value, package, local_type_names);
        }
        ASTNode::MemberAssignment { object, value, .. } => {
            qualify_imported_expression(object, package, local_type_names);
            qualify_imported_expression(value, package, local_type_names);
        }
        ASTNode::FunctionDefinition {
            parameters,
            return_type,
            body,
            ..
        } => {
            for (_, ty) in parameters {
                *ty = qualify_imported_type(*ty, package, local_type_names);
            }
            *return_type = qualify_imported_type(*return_type, package, local_type_names);
            qualify_imported_expression(body, package, local_type_names);
        }
        ASTNode::ExternalFunction {
            parameters,
            return_type,
            ..
        } => {
            for (_, ty) in parameters {
                *ty = qualify_imported_type(*ty, package, local_type_names);
            }
            *return_type = qualify_imported_type(*return_type, package, local_type_names);
        }
        ASTNode::Return(value) => {
            if let Some(value) = value {
                qualify_imported_expression(value, package, local_type_names);
            }
        }
        ASTNode::StructDefinition {
            name,
            fields,
            methods,
            ..
        } => {
            *name = Box::leak(format!("{package}.{name}").into_boxed_str());
            for (_, ty, default, _) in fields {
                *ty = qualify_imported_type(*ty, package, local_type_names);
                if let Some(default) = default {
                    qualify_imported_expression(default, package, local_type_names);
                }
            }
            for method in methods {
                for (_, ty) in &mut method.parameters {
                    *ty = qualify_imported_type(*ty, package, local_type_names);
                }
                method.return_type =
                    qualify_imported_type(method.return_type, package, local_type_names);
                qualify_imported_expression(&mut method.body, package, local_type_names);
            }
        }
        ASTNode::If {
            condition,
            then_branch,
            else_branch,
        } => {
            qualify_imported_expression(condition, package, local_type_names);
            qualify_imported_expression(then_branch, package, local_type_names);
            if let Some(branch) = else_branch {
                qualify_imported_expression(branch, package, local_type_names);
            }
        }
        ASTNode::While { condition, body } | ASTNode::DoWhile { body, condition } => {
            qualify_imported_expression(condition, package, local_type_names);
            qualify_imported_expression(body, package, local_type_names);
        }
        ASTNode::For { iterable, body, .. } => {
            qualify_imported_expression(iterable, package, local_type_names);
            qualify_imported_expression(body, package, local_type_names);
        }
        ASTNode::Loop { body } | ASTNode::Defer { body } => {
            qualify_imported_expression(body, package, local_type_names);
        }
        ASTNode::EnumDefinition { name, variants, .. } => {
            *name = Box::leak(format!("{package}.{name}").into_boxed_str());
            for (_, payload) in variants {
                *payload = payload.map(|ty| qualify_imported_type(ty, package, local_type_names));
            }
        }
        ASTNode::IfLet {
            value,
            then_branch,
            else_branch,
            ..
        } => {
            qualify_imported_expression(value, package, local_type_names);
            qualify_imported_expression(then_branch, package, local_type_names);
            if let Some(branch) = else_branch {
                qualify_imported_expression(branch, package, local_type_names);
            }
        }
        ASTNode::Public(declaration) => {
            **declaration =
                qualify_imported_node((**declaration).clone(), package, local_type_names);
        }
        ASTNode::Break | ASTNode::Continue | ASTNode::Import { .. } | ASTNode::CImport { .. } => {}
    }
    node
}

fn qualify_imported_expression(
    expression: &mut parser::Expression,
    package: &str,
    local_type_names: &std::collections::HashSet<String>,
) {
    use parser::Expression;
    match expression {
        Expression::Negate(inner)
        | Expression::Not(inner)
        | Expression::AddressOf(inner)
        | Expression::Dereference(inner) => {
            qualify_imported_expression(inner, package, local_type_names);
        }
        Expression::Add(lhs, rhs)
        | Expression::Subtract(lhs, rhs)
        | Expression::Multiply(lhs, rhs)
        | Expression::Divide(lhs, rhs)
        | Expression::Remainder(lhs, rhs)
        | Expression::Equal(lhs, rhs)
        | Expression::NotEqual(lhs, rhs)
        | Expression::LessThan(lhs, rhs)
        | Expression::LessThanOrEqual(lhs, rhs)
        | Expression::GreaterThan(lhs, rhs)
        | Expression::GreaterThanOrEqual(lhs, rhs)
        | Expression::And(lhs, rhs)
        | Expression::Or(lhs, rhs) => {
            qualify_imported_expression(lhs, package, local_type_names);
            qualify_imported_expression(rhs, package, local_type_names);
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            qualify_imported_expression(condition, package, local_type_names);
            qualify_imported_expression(then_branch, package, local_type_names);
            qualify_imported_expression(else_branch, package, local_type_names);
        }
        Expression::Block(nodes) => {
            for node in nodes {
                *node = qualify_imported_node(node.clone(), package, local_type_names);
            }
        }
        Expression::Call { arguments, .. } => {
            for argument in arguments {
                qualify_imported_expression(argument, package, local_type_names);
            }
        }
        Expression::CallValue { callee, arguments } => {
            qualify_imported_expression(callee, package, local_type_names);
            for argument in arguments {
                qualify_imported_expression(argument, package, local_type_names);
            }
        }
        Expression::StructLiteral { type_name, fields } => {
            if let Some(name) = type_name
                && (local_type_names.contains(*name) || name.contains('.'))
            {
                *name = Box::leak(format!("{package}.{name}").into_boxed_str());
            }
            for (_, value) in fields {
                qualify_imported_expression(value, package, local_type_names);
            }
        }
        Expression::MemberAccess { object, .. } => {
            qualify_imported_expression(object, package, local_type_names);
        }
        Expression::ArrayLiteral(values) => {
            for value in values {
                qualify_imported_expression(value, package, local_type_names);
            }
        }
        Expression::Index { object, index } => {
            qualify_imported_expression(object, package, local_type_names);
            qualify_imported_expression(index, package, local_type_names);
        }
        Expression::Slice { object, start, end } => {
            qualify_imported_expression(object, package, local_type_names);
            if let Some(start) = start {
                qualify_imported_expression(start, package, local_type_names);
            }
            if let Some(end) = end {
                qualify_imported_expression(end, package, local_type_names);
            }
        }
        Expression::Match { value, arms } => {
            qualify_imported_expression(value, package, local_type_names);
            for (_, expression) in arms {
                qualify_imported_expression(expression, package, local_type_names);
            }
        }
        Expression::Lambda {
            parameters,
            return_type,
            body,
        } => {
            for (_, ty) in parameters {
                *ty = qualify_imported_type(*ty, package, local_type_names);
            }
            *return_type = qualify_imported_type(*return_type, package, local_type_names);
            qualify_imported_expression(body, package, local_type_names);
        }
        Expression::StaticAccess { type_name, .. } => {
            if local_type_names.contains(type_name) || type_name.contains('.') {
                *type_name = format!("{package}.{type_name}");
            }
        }
        Expression::Identifier(_)
        | Expression::LiteralNumber(_)
        | Expression::LiteralString(_)
        | Expression::LiteralBool(_) => {}
    }
}

fn imported_function_aliases(nodes: &[parser::ASTNode], package: &str) -> Vec<parser::ASTNode> {
    type Signature = (Vec<(String, parser::DataType)>, parser::DataType);

    let mut signatures = std::collections::HashMap::<String, Signature>::new();
    let mut aliases = Vec::new();
    for node in nodes {
        let declaration = match node {
            parser::ASTNode::Public(declaration) => declaration.as_ref(),
            declaration => declaration,
        };
        match declaration {
            parser::ASTNode::FunctionDefinition {
                name,
                parameters,
                return_type,
                ..
            }
            | parser::ASTNode::ExternalFunction {
                name,
                parameters,
                return_type,
                ..
            } => {
                signatures.insert(name.clone(), (parameters.clone(), *return_type));
            }
            parser::ASTNode::CompileTimeConstant {
                name,
                value: parser::Expression::Identifier(target),
                ..
            } => aliases.push((name.clone(), target.clone())),
            _ => {}
        }
    }

    let mut functions = Vec::new();
    loop {
        let mut resolved_alias = false;
        for (name, target) in &aliases {
            if signatures.contains_key(name) {
                continue;
            }
            let Some((parameters, return_type)) = signatures.get(target).cloned() else {
                continue;
            };
            let arguments = parameters
                .iter()
                .map(|(parameter, _)| parser::Expression::Identifier(parameter.clone()))
                .collect();
            functions.push(parser::ASTNode::FunctionDefinition {
                name: format!("{package}.{name}"),
                parameters: parameters.clone(),
                return_type,
                body: parser::Expression::Call {
                    name: target.clone(),
                    arguments,
                },
            });
            signatures.insert(name.clone(), (parameters, return_type));
            resolved_alias = true;
        }
        if !resolved_alias {
            break;
        }
    }
    functions
}

#[cfg(test)]
#[path = "../tests/parser/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/type_checker/mod.rs"]
mod type_checker_tests;

#[cfg(test)]
#[path = "../tests/ir/mod.rs"]
mod ir_tests;
