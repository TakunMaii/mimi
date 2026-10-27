mod hir;
mod lexer;
mod lir;
mod mir;
mod parser;
mod type_checker;

fn main() {}

#[cfg(test)]
#[path = "../tests/parser/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/type_checker/mod.rs"]
mod type_checker_tests;

#[cfg(test)]
#[path = "../tests/ir/mod.rs"]
mod ir_tests;
