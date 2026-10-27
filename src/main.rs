mod codegen;
mod hir;
mod lexer;
mod lir;
mod mir;
mod parser;
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
            let input = args
                .next()
                .ok_or("usage: mimi build <source> -o <output>")?;
            if args.next().as_deref() != Some("-o") {
                return Err("usage: mimi build <source> -o <output>".into());
            }
            let output = args.next().ok_or("missing output path")?;
            if args.next().is_some() {
                return Err("usage: mimi build <source> -o <output>".into());
            }
            build_file(&input, &output)
        }
        _ => Err("usage: mimi build <source> -o <output>".into()),
    }
}

fn build_file(input: &str, output: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(input)
        .map_err(|error| format!("cannot read '{input}': {error}"))?;
    let ast = parser::parse(lexer::tokenize(source.chars()));
    type_checker::check(&ast).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let hir = hir::lower_ast(&ast);
    let mir = mir::lower_hir(&hir);
    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    codegen::compile_executable(&lir, std::path::Path::new(output))
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
