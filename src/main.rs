mod c_import;
mod c_type;
mod codegen;
mod codegen_x86_64;
mod diagnostic;
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
    std::panic::set_hook(Box::new(|info| {
        let is_reported_compiler_failure = COMPILER_DIAGNOSTIC_PANIC.with(std::cell::Cell::get);
        if !is_reported_compiler_failure {
            eprintln!("internal compiler error: {info}");
        }
    }));
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

thread_local! {
    static COMPILER_DIAGNOSTIC_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn catch_compiler_failure<T>(
    source_name: &str,
    source: &str,
    phase: &str,
    operation: impl FnOnce() -> T,
) -> Result<T, String> {
    let previous = COMPILER_DIAGNOSTIC_PANIC.with(|flag| flag.replace(true));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
    COMPILER_DIAGNOSTIC_PANIC.with(|flag| flag.set(previous));
    result.map_err(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|message| (*message).to_string())
            })
            .unwrap_or_else(|| "compiler operation failed unexpectedly".to_string());
        diagnostic::render(source_name, source, phase, &message)
    })
}

fn parse_source(source_name: &str, source: &str) -> Result<Vec<parser::ASTNode>, String> {
    let tokens = catch_compiler_failure(source_name, source, "lex", || {
        lexer::tokenize_spanned(source)
    })?
    .map_err(|error| {
        diagnostic::render_span(source_name, source, "lex", &error.message, error.span)
    })?;
    catch_compiler_failure(source_name, source, "parse", || {
        parser::parse_spanned(tokens, source.len())
    })?
    .map_err(|error| {
        diagnostic::render_span(source_name, source, "parse", &error.message, error.span)
    })
}

fn render_type_errors(
    source_name: &str,
    source: &str,
    errors: Vec<type_checker::TypeError>,
) -> String {
    let tokens = lexer::tokenize_spanned(source).ok();
    errors
        .into_iter()
        .map(|error| {
            if error.message.contains("error[lex]:") || error.message.contains("error[parse]:") {
                return error.message;
            }
            if let Some(span) = tokens
                .as_deref()
                .and_then(|tokens| diagnostic::find_token_span(tokens, &error.message))
            {
                diagnostic::render_span(source_name, source, "type", &error.message, span)
            } else {
                diagnostic::render(source_name, source, "type", &error.message)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
            let mut cache_enabled = true;
            while let Some(argument) = args.next() {
                if argument == "--no-cache" {
                    cache_enabled = false;
                } else if argument == "-o" {
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
                        "usage: mimi build <source> -o <output> [--target host|aarch64|x86_64] [--no-cache] [-Ipath] [-Lpath] [-lname] [-Dname[=value]] [--sysroot path] [--c-source file.c]".into(),
                    );
                }
            }
            build_file_for_target_with_c_options_and_cache(
                &input.ok_or("missing source path")?,
                &output.ok_or("missing output path")?,
                &import_paths,
                &library_paths,
                &libraries,
                target,
                &defines,
                sysroot.as_deref(),
                &c_sources,
                cache_enabled,
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
    build_file_for_target_with_c_options_and_cache(
        input,
        output,
        import_paths,
        library_paths,
        libraries,
        target,
        defines,
        sysroot,
        c_sources,
        true,
    )
}

fn build_file_for_target_with_c_options_and_cache(
    input: &str,
    output: &str,
    import_paths: &[std::path::PathBuf],
    library_paths: &[std::path::PathBuf],
    libraries: &[String],
    target: lir::TargetArch,
    defines: &[String],
    sysroot: Option<&std::path::Path>,
    c_sources: &[std::path::PathBuf],
    cache_enabled: bool,
) -> Result<(), String> {
    let source = std::fs::read_to_string(input)
        .map_err(|error| format!("cannot read '{input}': {error}"))?;
    let mut import_paths = import_paths.to_vec();
    if let Some(parent) = std::path::Path::new(input).parent()
        && !import_paths.iter().any(|path| path == parent)
    {
        import_paths.push(parent.to_path_buf());
    }
    let ast = parse_source(input, &source)?;
    let build_cache_key = if cache_enabled {
        build_cache_key(
            input,
            &source,
            &ast,
            target,
            &import_paths,
            library_paths,
            libraries,
            defines,
            sysroot,
            c_sources,
        )
        .ok()
    } else {
        None
    };
    let build_cache_path = build_cache_key.map(|key| {
        std::env::current_dir()
            .unwrap_or_default()
            .join(".mimi")
            .join("build")
            .join(format!("{key:016x}.bin"))
    });
    if let Some(cache_path) = &build_cache_path
        && cache_path.is_file()
        && restore_build_cache(cache_path, output)
    {
        return Ok(());
    }
    let cache_dir = cache_enabled
        .then(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.join(".mimi").join("cimport"))
        })
        .flatten();
    let (ast, generated_c_sources) = catch_compiler_failure(input, &source, "C import", || {
        c_import::expand_with_shims(
            &ast,
            target,
            &import_paths,
            defines,
            sysroot,
            cache_dir.as_deref(),
        )
    })??;
    catch_compiler_failure(input, &source, "type check", || {
        type_checker::check_with_import_paths(&ast, &import_paths)
    })?
    .map_err(|errors| render_type_errors(input, &source, errors))?;
    let mut compilation_ast = ast.clone();
    catch_compiler_failure(input, &source, "import resolution", || {
        collect_imported_functions(
            &ast,
            &import_paths,
            "",
            &mut std::collections::HashSet::new(),
            &mut compilation_ast,
        )
    })??;
    let hir = catch_compiler_failure(input, &source, "HIR lowering", || {
        hir::lower_ast(&compilation_ast)
    })?;
    let layout = <lir::TargetArch as lir::Target>::data_layout(&target);
    let mir = catch_compiler_failure(input, &source, "MIR lowering", || {
        mir::lower_hir_with_layout(&hir, layout)
    })?;
    let lir = catch_compiler_failure(input, &source, "LIR lowering", || {
        lir::lower_mir(&mir, &target)
    })?;
    let result = catch_compiler_failure(input, &source, "code generation", || {
        codegen::compile_executable_with_options(
            &lir,
            std::path::Path::new(output),
            &codegen::LinkOptions {
                library_paths: library_paths.to_vec(),
                libraries: libraries.to_vec(),
                c_sources: c_sources.to_vec(),
                generated_c_sources,
                include_paths: import_paths.clone(),
                defines: defines.to_vec(),
                sysroot: sysroot.map(std::path::Path::to_path_buf),
            },
        )
    })?;
    if result.is_ok()
        && let Some(cache_path) = build_cache_path
        && let Some(parent) = cache_path.parent()
        && std::fs::create_dir_all(parent).is_ok()
    {
        store_build_cache(&cache_path, output);
    }
    result
}

fn restore_build_cache(cache_path: &std::path::Path, output: &str) -> bool {
    use std::hash::{Hash, Hasher};
    let Ok(bytes) = std::fs::read(cache_path) else {
        return false;
    };
    if bytes.get(..8) != Some(&b"MIMIBLD1"[..]) || bytes.len() < 28 {
        return false;
    }
    let length = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let checksum = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let mode = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let executable = &bytes[28..];
    if length != executable.len() as u64 {
        return false;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    executable.hash(&mut hasher);
    if checksum != hasher.finish() {
        return false;
    }
    if std::fs::write(output, executable).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::set_permissions(output, std::fs::Permissions::from_mode(mode)).is_err() {
            return false;
        }
    }
    true
}

fn store_build_cache(cache_path: &std::path::Path, output: &str) {
    use std::hash::{Hash, Hasher};
    let Ok(executable) = std::fs::read(output) else {
        return;
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    executable.hash(&mut hasher);
    let mode = std::fs::metadata(output)
        .map(|metadata| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o7777
            }
            #[cfg(not(unix))]
            {
                let _ = metadata;
                0
            }
        })
        .unwrap_or(0);
    let mut bytes = Vec::with_capacity(executable.len() + 28);
    bytes.extend_from_slice(b"MIMIBLD1");
    bytes.extend_from_slice(&(executable.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&hasher.finish().to_le_bytes());
    bytes.extend_from_slice(&mode.to_le_bytes());
    bytes.extend_from_slice(&executable);
    let temporary = cache_path.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(temporary, cache_path);
    }
}

fn build_cache_key(
    input: &str,
    source: &str,
    nodes: &[parser::ASTNode],
    target: lir::TargetArch,
    import_paths: &[std::path::PathBuf],
    library_paths: &[std::path::PathBuf],
    libraries: &[String],
    defines: &[String],
    sysroot: Option<&std::path::Path>,
    c_sources: &[std::path::PathBuf],
) -> Result<u64, String> {
    use std::hash::{Hash, Hasher};
    if !c_sources.is_empty() {
        return Err("build caching with additional C sources is not available".into());
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    input.hash(&mut hasher);
    source.hash(&mut hasher);
    format!("{target:?}").hash(&mut hasher);
    import_paths.hash(&mut hasher);
    library_paths.hash(&mut hasher);
    libraries.hash(&mut hasher);
    defines.hash(&mut hasher);
    sysroot.hash(&mut hasher);
    if let Ok(executable) = std::env::current_exe()
        && let Ok(contents) = std::fs::read(executable)
    {
        contents.hash(&mut hasher);
    }
    let mut build_tools = vec![("cc", vec!["--version"]), ("ld", vec!["-v"])];
    if cfg!(target_os = "macos") {
        build_tools.push(("xcrun", vec!["--sdk", "macosx", "--show-sdk-version"]));
    }
    for (tool, arguments) in build_tools {
        let output = std::process::Command::new(tool)
            .args(arguments)
            .output()
            .map_err(|error| format!("cannot run build tool '{tool}': {error}"))?;
        if !output.status.success() {
            return Err(format!("cannot identify build tool '{tool}'"));
        }
        output.stdout.hash(&mut hasher);
        output.stderr.hash(&mut hasher);
    }
    hash_imported_sources(
        nodes,
        import_paths,
        target,
        defines,
        sysroot,
        &mut std::collections::HashSet::new(),
        &mut hasher,
    )?;
    c_import::cache_fingerprint(nodes, target, import_paths, defines, sysroot)?.hash(&mut hasher);
    hash_link_libraries(libraries, library_paths, &mut hasher)?;
    if let Some(sysroot) = sysroot {
        sysroot.hash(&mut hasher);
    }
    Ok(hasher.finish())
}

fn hash_imported_sources(
    nodes: &[parser::ASTNode],
    import_paths: &[std::path::PathBuf],
    target: lir::TargetArch,
    defines: &[String],
    sysroot: Option<&std::path::Path>,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    hasher: &mut impl std::hash::Hasher,
) -> Result<(), String> {
    use std::hash::Hash;
    for node in nodes {
        let parser::ASTNode::Import { path, .. } = node else {
            continue;
        };
        let source_path = type_checker::resolve_import_path(path, import_paths)?;
        let canonical = source_path
            .canonicalize()
            .unwrap_or_else(|_| source_path.clone());
        if !visited.insert(canonical.clone()) {
            continue;
        }
        canonical.hash(hasher);
        let source = std::fs::read_to_string(&source_path)
            .map_err(|error| format!("cannot read '{}': {error}", source_path.display()))?;
        source.hash(hasher);
        let module_nodes = parse_source(&source_path.display().to_string(), &source)?;
        let mut module_paths = import_paths.to_vec();
        if let Some(parent) = source_path.parent()
            && !module_paths.iter().any(|candidate| candidate == parent)
        {
            module_paths.push(parent.to_path_buf());
        }
        c_import::cache_fingerprint(&module_nodes, target, &module_paths, defines, sysroot)?
            .hash(hasher);
        hash_imported_sources(
            &module_nodes,
            &module_paths,
            target,
            defines,
            sysroot,
            visited,
            hasher,
        )?;
    }
    Ok(())
}

fn hash_link_libraries(
    libraries: &[String],
    library_paths: &[std::path::PathBuf],
    hasher: &mut impl std::hash::Hasher,
) -> Result<(), String> {
    use std::hash::Hash;
    let mut search_paths = library_paths.to_vec();
    if let Ok(sdk) = std::process::Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        && sdk.status.success()
    {
        let sdk = String::from_utf8_lossy(&sdk.stdout).trim().to_string();
        sdk.hash(hasher);
        search_paths.push(std::path::PathBuf::from(sdk).join("usr/lib"));
    }
    search_paths.extend([
        std::path::PathBuf::from("/opt/homebrew/lib"),
        std::path::PathBuf::from("/usr/local/lib"),
        std::path::PathBuf::from("/usr/lib"),
    ]);
    for library in libraries {
        library.hash(hasher);
        let candidates = [
            format!("lib{library}.dylib"),
            format!("lib{library}.tbd"),
            format!("lib{library}.a"),
            format!("lib{library}.so"),
        ];
        let mut found = false;
        for path in &search_paths {
            for candidate in &candidates {
                let path = path.join(candidate);
                if let Ok(contents) = std::fs::read(&path) {
                    path.hash(hasher);
                    contents.hash(hasher);
                    found = true;
                }
            }
        }
        if !found {
            return Err(format!("cannot fingerprint linked library '{library}'"));
        }
    }
    Ok(())
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
        let module_nodes = parse_source(&source_path.display().to_string(), &source)?;
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
        let module_name = source_path.display().to_string();
        catch_compiler_failure(&module_name, &source, "type check", || {
            type_checker::check_with_import_paths(&module_nodes, &module_paths)
        })?
        .map_err(|errors| render_type_errors(&module_name, &source, errors))?;

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
        parser::DataType::Tuple(elements) => parser::DataType::Tuple(Box::leak(
            elements
                .iter()
                .copied()
                .map(|element| qualify_imported_type(element, package, local_type_names))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )),
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
        ASTNode::TupleDestructuring { value, .. } => {
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
        ASTNode::ExternalGlobal {
            name, data_type, ..
        } => {
            *name = format!("{package}.{name}");
            *data_type = qualify_imported_type(*data_type, package, local_type_names);
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
        ASTNode::ForRange {
            start, end, body, ..
        } => {
            qualify_imported_expression(start, package, local_type_names);
            qualify_imported_expression(end, package, local_type_names);
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
        ASTNode::CTypeEnumRepresentation { name, .. } => {
            *name = format!("{package}.{name}");
        }
        ASTNode::CTypeInfo { name, .. } => {
            *name = format!("{package}.{name}");
        }
        ASTNode::Break
        | ASTNode::Continue
        | ASTNode::Import { .. }
        | ASTNode::CImport { .. }
        | ASTNode::CDefine { .. }
        | ASTNode::CTypeAlias { .. }
        | ASTNode::CTypeRecordLayout { .. } => {}
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
        Expression::Cast { value, data_type } => {
            qualify_imported_expression(value, package, local_type_names);
            *data_type = qualify_imported_type(*data_type, package, local_type_names);
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
        Expression::ArrayLiteral(values) | Expression::TupleLiteral(values) => {
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

#[cfg(test)]
mod build_cache_tests {
    use super::{restore_build_cache, store_build_cache};

    #[test]
    fn validates_cached_executable_and_restores_its_permissions() {
        let directory =
            std::env::temp_dir().join(format!("mimi_build_cache_test_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let executable = directory.join("program");
        let cache = directory.join("program.cache");
        let restored = directory.join("restored");
        std::fs::write(&executable, b"executable contents").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        store_build_cache(&cache, executable.to_str().unwrap());
        assert!(restore_build_cache(&cache, restored.to_str().unwrap()));
        assert_eq!(std::fs::read(&restored).unwrap(), b"executable contents");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&restored).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        let mut corrupted = std::fs::read(&cache).unwrap();
        corrupted.pop();
        std::fs::write(&cache, corrupted).unwrap();
        assert!(!restore_build_cache(&cache, restored.to_str().unwrap()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
