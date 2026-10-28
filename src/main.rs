mod codegen;
mod hir;
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
            let mut import_paths = Vec::new();
            let mut library_paths = Vec::new();
            let mut libraries = Vec::new();
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
                } else if input.is_none() {
                    input = Some(argument);
                } else {
                    return Err(
                        "usage: mimi build <source> -o <output> [-Ipath] [-Lpath] [-lname]".into(),
                    );
                }
            }
            build_file(
                &input.ok_or("missing source path")?,
                &output.ok_or("missing output path")?,
                &import_paths,
                &library_paths,
                &libraries,
            )
        }
        _ => Err("usage: mimi build <source> -o <output>".into()),
    }
}

fn build_file(
    input: &str,
    output: &str,
    import_paths: &[std::path::PathBuf],
    library_paths: &[std::path::PathBuf],
    libraries: &[String],
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
    let mir = mir::lower_hir(&hir);
    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    codegen::compile_executable_with_options(
        &lir,
        std::path::Path::new(output),
        &codegen::LinkOptions {
            library_paths: library_paths.to_vec(),
            libraries: libraries.to_vec(),
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
                let mut function = declaration.clone();
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
                let mut external = declaration.clone();
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
        }
        collect_imported_functions(&module_nodes, &module_paths, &package, visiting, output)?;
        visiting.remove(&visit_key);
    }
    Ok(())
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
