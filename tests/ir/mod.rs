use super::{hir, lexer::tokenize, lir, mir, parser::parse};

#[test]
fn lowers_function_through_all_ir_layers() {
    let ast = parse(tokenize(
        "add :: fn (x : i32, y : i32) i32 { x + y }".chars(),
    ));
    let hir = hir::lower_ast(&ast);
    assert_eq!(hir.functions.len(), 1);
    assert_eq!(hir.functions[0].name, "add");

    let mir = mir::lower_hir(&hir);
    assert_eq!(mir.functions.len(), 1);
    assert!(
        mir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, mir::MirInstruction::Add { .. }))
    );

    let lir = lir::lower_mir(&mir, &lir::HostTarget);
    assert_eq!(
        lir.target,
        <lir::HostTarget as lir::Target>::arch(&lir::HostTarget)
    );
    assert!(
        lir.functions[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, lir::LirInstruction::Add { .. }))
    );
}
