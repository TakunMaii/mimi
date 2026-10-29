use crate::hir::HirType;
use crate::lir::{LirFunction, LirInstruction, LirModule};
use crate::mir::{CompareKind, FloatBinaryKind, MirTerminator, SlotId, ValueId};

pub(crate) fn emit_assembly(module: &LirModule) -> Result<String, String> {
    if module.data_layout.pointer_bytes() != 8 || module.data_layout.stack_slot_bytes() != 8 {
        return Err("x86_64 backend requires 8-byte pointers and stack slots".into());
    }
    let mut assembly = String::from(".intel_syntax noprefix\n.text\n");
    for function in &module.functions {
        emit_function(module, function, &mut assembly)?;
    }
    if !module.strings.is_empty() {
        if cfg!(target_os = "macos") {
            assembly.push_str(".section __TEXT,__const\n.p2align 3\n");
        } else {
            assembly.push_str(".section .rodata\n.p2align 3\n");
        }
        for (name, bytes) in &module.strings {
            assembly.push_str(&format!("{}:\n", symbol(name)));
            if bytes.is_empty() {
                assembly.push_str("    .byte 0\n");
            } else {
                for chunk in bytes.chunks(16) {
                    assembly.push_str("    .byte ");
                    for (index, byte) in chunk.iter().enumerate() {
                        if index != 0 {
                            assembly.push_str(", ");
                        }
                        assembly.push_str(&byte.to_string());
                    }
                    assembly.push('\n');
                }
            }
        }
    }
    Ok(assembly)
}

fn emit_function(
    module: &LirModule,
    function: &LirFunction,
    assembly: &mut String,
) -> Result<(), String> {
    if function.blocks.is_empty() {
        return Err("function has no basic blocks".into());
    }
    if function.float_params.len() > 8 {
        return Err("x86_64 backend supports at most 8 floating-point parameters".into());
    }
    let value_count = value_count(function);
    let stack_slots = u64::from(function.slot_count)
        .checked_add(u64::from(value_count))
        .ok_or_else(|| "x86_64 frame is too large".to_string())?;
    let raw_frame_size = stack_slots
        .checked_mul(8)
        .ok_or_else(|| "x86_64 frame is too large".to_string())?;
    let frame_size = raw_frame_size.div_ceil(16) * 16;
    if frame_size > i32::MAX as u64 {
        return Err("x86_64 frame exceeds the encodable stack size".into());
    }

    let name = symbol(&function.name);
    assembly.push_str(&format!(".globl {name}\n{name}:\n"));
    assembly.push_str("    push rbp\n    mov rbp, rsp\n");
    if frame_size != 0 {
        assembly.push_str(&format!("    sub rsp, {frame_size}\n"));
    }

    let integer_registers = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
    for (index, parameter) in function.params.iter().enumerate() {
        if let Some(register) = integer_registers.get(index) {
            assembly.push_str(&format!("    mov rax, {register}\n"));
        } else {
            let offset = 16 + (index as u64 - integer_registers.len() as u64) * 8;
            assembly.push_str(&format!("    mov rax, qword ptr [rbp + {offset}]\n"));
        }
        store_value(assembly, function, *parameter, "rax")?;
    }
    for (index, (parameter, width)) in function.float_params.iter().enumerate() {
        load_float_bits_from_xmm(assembly, index, *width, "rax")?;
        store_value(assembly, function, *parameter, "rax")?;
    }

    for block in &function.blocks {
        assembly.push_str(&format!(".L{}_{}:\n", function.name, block.id.0));
        for instruction in &block.instructions {
            emit_instruction(module, function, instruction, assembly)?;
        }
        match &block.terminator {
            MirTerminator::Jump(target) => {
                emit_phi_copies(function, block.id, *target, assembly)?;
                assembly.push_str(&format!("    jmp .L{}_{}\n", function.name, target.0));
            }
            MirTerminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                load_value(assembly, function, *condition, "rax")?;
                assembly.push_str("    test rax, rax\n");
                assembly.push_str(&format!(
                    "    jz .L{}_{}_else_edge\n",
                    function.name, block.id.0
                ));
                emit_phi_copies(function, block.id, *then_block, assembly)?;
                assembly.push_str(&format!("    jmp .L{}_{}\n", function.name, then_block.0));
                assembly.push_str(&format!(".L{}_{}_else_edge:\n", function.name, block.id.0));
                emit_phi_copies(function, block.id, *else_block, assembly)?;
                assembly.push_str(&format!("    jmp .L{}_{}\n", function.name, else_block.0));
            }
            MirTerminator::Return {
                value,
                slice_length,
            } => {
                if let (Some(pointer), Some(length)) = (value, slice_length) {
                    load_value(assembly, function, *pointer, "rax")?;
                    load_value(assembly, function, *length, "rdx")?;
                } else if let Some(value) = value {
                    if let HirType::Float {
                        bits: bits @ (16 | 32 | 64),
                    } = function.result
                    {
                        load_value(assembly, function, *value, "rax")?;
                        load_float_abi_bits_to_xmm(assembly, "rax", 0, bits)?;
                    } else {
                        load_value(assembly, function, *value, "rax")?;
                    }
                } else {
                    assembly.push_str("    xor eax, eax\n");
                }
                assembly.push_str("    leave\n    ret\n");
            }
            MirTerminator::Unreachable => {
                return Err("unreachable terminator reached x86_64 backend".into());
            }
        }
    }
    Ok(())
}

fn emit_instruction(
    module: &LirModule,
    function: &LirFunction,
    instruction: &LirInstruction,
    assembly: &mut String,
) -> Result<(), String> {
    match instruction {
        LirInstruction::LoadImmediate { register, value } => {
            assembly.push_str(&format!("    movabs rax, 0x{:x}\n", *value as u64));
            store_value(assembly, function, *register, "rax")?;
        }
        LirInstruction::LoadAddress {
            register,
            symbol: address,
            external,
        } => {
            let address = symbol(address);
            if *external {
                assembly.push_str(&format!(
                    "    mov rax, qword ptr [rip + {address}@GOTPCREL]\n"
                ));
            } else {
                assembly.push_str(&format!("    lea rax, [rip + {address}]\n"));
            }
            store_value(assembly, function, *register, "rax")?;
        }
        LirInstruction::Call {
            result,
            result_slice_length,
            name,
            args,
            float_args,
            result_float,
        } => emit_call(
            function,
            args,
            float_args,
            Some(&symbol(name)),
            None,
            *result,
            *result_slice_length,
            *result_float,
            assembly,
        )?,
        LirInstruction::CallIndirect {
            result,
            result_slice_length,
            callee,
            args,
            float_args,
            result_float,
        } => emit_call(
            function,
            args,
            float_args,
            None,
            Some(*callee),
            *result,
            *result_slice_length,
            *result_float,
            assembly,
        )?,
        LirInstruction::Add {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Sub {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Mul {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Div {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Rem {
            destination,
            lhs,
            rhs,
        } => {
            load_value(assembly, function, *lhs, "rax")?;
            load_value(assembly, function, *rhs, "rcx")?;
            match instruction {
                LirInstruction::Add { .. } => assembly.push_str("    add rax, rcx\n"),
                LirInstruction::Sub { .. } => assembly.push_str("    sub rax, rcx\n"),
                LirInstruction::Mul { .. } => assembly.push_str("    imul rax, rcx\n"),
                LirInstruction::Div { .. } => {
                    assembly.push_str("    cqo\n    idiv rcx\n");
                }
                LirInstruction::Rem { .. } => {
                    assembly.push_str("    cqo\n    idiv rcx\n    mov rax, rdx\n");
                }
                _ => unreachable!(),
            }
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::Neg { destination, value } => {
            load_value(assembly, function, *value, "rax")?;
            assembly.push_str("    neg rax\n");
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::Not { destination, value } => {
            load_value(assembly, function, *value, "rax")?;
            assembly.push_str("    test rax, rax\n    sete al\n    movzx eax, al\n");
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::Compare {
            destination,
            lhs,
            rhs,
            kind,
        } => {
            load_value(assembly, function, *lhs, "rax")?;
            load_value(assembly, function, *rhs, "rcx")?;
            assembly.push_str("    cmp rax, rcx\n");
            let condition = match kind {
                CompareKind::Equal => "e",
                CompareKind::NotEqual => "ne",
                CompareKind::Less => "l",
                CompareKind::LessEqual => "le",
                CompareKind::Greater => "g",
                CompareKind::GreaterEqual => "ge",
            };
            assembly.push_str(&format!("    set{condition} al\n    movzx eax, al\n"));
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::FloatBinary {
            destination,
            lhs,
            rhs,
            kind,
            width,
        } => emit_float_binary(function, *destination, *lhs, *rhs, *kind, *width, assembly)?,
        LirInstruction::FloatNeg {
            destination,
            value,
            width,
        } => {
            load_value(assembly, function, *value, "rax")?;
            match width {
                16 => assembly.push_str("    xor eax, 0x8000\n"),
                32 => assembly.push_str("    xor eax, 0x80000000\n"),
                64 => assembly.push_str("    btc rax, 63\n"),
                _ => return Err(format!("unsupported x86_64 float width {width}")),
            }
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::FloatCompare {
            destination,
            lhs,
            rhs,
            kind,
            width,
        } => emit_float_compare(function, *destination, *lhs, *rhs, *kind, *width, assembly)?,
        LirInstruction::Phi { .. } => {}
        LirInstruction::LoadSlot { destination, slot } => {
            assembly.push_str(&format!("    mov rax, {}\n", slot_mem(function, *slot)?));
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::StoreSlot { slot, value } => {
            load_value(assembly, function, *value, "rax")?;
            assembly.push_str(&format!("    mov {}, rax\n", slot_mem(function, *slot)?));
        }
        LirInstruction::SlotAddress { destination, slot } => {
            if slot.0 >= function.slot_count {
                return Err(format!("invalid x86_64 stack slot {}", slot.0));
            }
            assembly.push_str(&format!(
                "    lea rax, [rbp - {}]\n",
                slot_offset(function, *slot)?
            ));
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::IndexAddress {
            destination,
            base,
            index,
            scale,
        } => {
            load_value(assembly, function, *base, "rax")?;
            load_value(assembly, function, *index, "rcx")?;
            if *scale != 1 {
                assembly.push_str(&format!("    imul rcx, {scale}\n"));
            }
            assembly.push_str("    add rax, rcx\n");
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::LoadIndirect {
            destination,
            address,
        } => {
            load_value(assembly, function, *address, "rcx")?;
            assembly.push_str("    mov rax, qword ptr [rcx]\n");
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::LoadByteIndirect {
            destination,
            address,
        } => {
            load_value(assembly, function, *address, "rcx")?;
            assembly.push_str("    movzx eax, byte ptr [rcx]\n");
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::LoadSizedIndirect {
            destination,
            address,
            bytes,
            signed,
        } => {
            load_value(assembly, function, *address, "rcx")?;
            let operation = match (*bytes, *signed) {
                (1, true) => "movsx rax, byte ptr [rcx]",
                (1, false) => "movzx eax, byte ptr [rcx]",
                (2, true) => "movsx rax, word ptr [rcx]",
                (2, false) => "movzx eax, word ptr [rcx]",
                (4, true) => "movsxd rax, dword ptr [rcx]",
                (4, false) => "mov eax, dword ptr [rcx]",
                (8, _) => "mov rax, qword ptr [rcx]",
                _ => return Err(format!("unsupported x86_64 load width {bytes}")),
            };
            assembly.push_str(&format!("    {operation}\n"));
            store_value(assembly, function, *destination, "rax")?;
        }
        LirInstruction::StoreIndirect { address, value } => {
            load_value(assembly, function, *address, "rcx")?;
            load_value(assembly, function, *value, "rax")?;
            assembly.push_str("    mov qword ptr [rcx], rax\n");
        }
        LirInstruction::StoreByteIndirect { address, value } => {
            load_value(assembly, function, *address, "rcx")?;
            load_value(assembly, function, *value, "rax")?;
            assembly.push_str("    mov byte ptr [rcx], al\n");
        }
        LirInstruction::StoreSizedIndirect {
            address,
            value,
            bytes,
        } => {
            load_value(assembly, function, *address, "rcx")?;
            load_value(assembly, function, *value, "rax")?;
            let (size, register) = match bytes {
                1 => ("byte", "al"),
                2 => ("word", "ax"),
                4 => ("dword", "eax"),
                8 => ("qword", "rax"),
                _ => return Err(format!("unsupported x86_64 store width {bytes}")),
            };
            assembly.push_str(&format!("    mov {size} ptr [rcx], {register}\n"));
        }
    }
    let _ = module;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_call(
    function: &LirFunction,
    args: &[ValueId],
    float_args: &[(usize, u16)],
    direct_name: Option<&str>,
    indirect_callee: Option<ValueId>,
    result: Option<ValueId>,
    result_slice_length: Option<ValueId>,
    result_float: Option<u16>,
    assembly: &mut String,
) -> Result<(), String> {
    if float_args.len() > 8 {
        return Err("x86_64 backend supports at most 8 floating-point call arguments".into());
    }
    if let Some(callee) = indirect_callee {
        load_value(assembly, function, callee, "r11")?;
    }
    let float_positions = float_args
        .iter()
        .map(|(index, _)| *index)
        .collect::<std::collections::HashSet<_>>();
    let integer_args = args
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (!float_positions.contains(&index)).then_some(*value))
        .collect::<Vec<_>>();
    let register_args = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
    let stack_args = integer_args.len().saturating_sub(register_args.len());
    let stack_size = u64::try_from(stack_args)
        .ok()
        .and_then(|count| count.checked_mul(8))
        .ok_or_else(|| "too many x86_64 call arguments".to_string())?;
    let stack_size = stack_size.div_ceil(16) * 16;
    if stack_size != 0 {
        assembly.push_str(&format!("    sub rsp, {stack_size}\n"));
    }
    for (index, argument) in integer_args.iter().enumerate().skip(register_args.len()) {
        load_value(assembly, function, *argument, "rax")?;
        assembly.push_str(&format!(
            "    mov qword ptr [rsp + {}], rax\n",
            (index - register_args.len()) * 8
        ));
    }
    for (index, argument) in integer_args.iter().take(register_args.len()).enumerate() {
        load_value(assembly, function, *argument, "rax")?;
        assembly.push_str(&format!("    mov {}, rax\n", register_args[index]));
    }
    for (index, (argument, width)) in float_args.iter().enumerate() {
        load_value(assembly, function, args[*argument], "rax")?;
        load_float_abi_bits_to_xmm(assembly, "rax", index, *width)?;
    }
    if let Some(name) = direct_name {
        assembly.push_str(&format!("    call {name}\n"));
    } else {
        assembly.push_str("    call r11\n");
    }
    if stack_size != 0 {
        assembly.push_str(&format!("    add rsp, {stack_size}\n"));
    }
    if let (Some(pointer), Some(length)) = (result, result_slice_length) {
        store_value(assembly, function, pointer, "rax")?;
        store_value(assembly, function, length, "rdx")?;
    } else if let Some(result) = result {
        if let Some(width) = result_float {
            load_float_abi_bits_from_xmm(assembly, 0, width, "rax")?;
        }
        store_value(assembly, function, result, "rax")?;
    }
    Ok(())
}

fn emit_float_binary(
    function: &LirFunction,
    destination: ValueId,
    lhs: ValueId,
    rhs: ValueId,
    kind: FloatBinaryKind,
    width: u16,
    assembly: &mut String,
) -> Result<(), String> {
    load_float_operands(function, lhs, rhs, width, assembly)?;
    let operation = match (kind, width) {
        (FloatBinaryKind::Add, 16 | 32) => "addss",
        (FloatBinaryKind::Subtract, 16 | 32) => "subss",
        (FloatBinaryKind::Multiply, 16 | 32) => "mulss",
        (FloatBinaryKind::Divide, 16 | 32) => "divss",
        (FloatBinaryKind::Add, 64) => "addsd",
        (FloatBinaryKind::Subtract, 64) => "subsd",
        (FloatBinaryKind::Multiply, 64) => "mulsd",
        (FloatBinaryKind::Divide, 64) => "divsd",
        _ => return Err(format!("unsupported x86_64 float width {width}")),
    };
    assembly.push_str(&format!("    {operation} xmm0, xmm1\n"));
    store_float_result(function, destination, width, assembly)
}

fn emit_float_compare(
    function: &LirFunction,
    destination: ValueId,
    lhs: ValueId,
    rhs: ValueId,
    kind: CompareKind,
    width: u16,
    assembly: &mut String,
) -> Result<(), String> {
    load_float_operands(function, lhs, rhs, width, assembly)?;
    let compare = if width == 64 { "ucomisd" } else { "ucomiss" };
    if !matches!(width, 16 | 32 | 64) {
        return Err(format!("unsupported x86_64 float width {width}"));
    }
    assembly.push_str(&format!("    {compare} xmm0, xmm1\n"));
    match kind {
        CompareKind::Equal => {
            assembly.push_str("    sete al\n    setnp cl\n    and al, cl\n    movzx eax, al\n")
        }
        CompareKind::NotEqual => {
            assembly.push_str("    setne al\n    setp cl\n    or al, cl\n    movzx eax, al\n")
        }
        CompareKind::Less => {
            assembly.push_str("    setb al\n    setnp cl\n    and al, cl\n    movzx eax, al\n")
        }
        CompareKind::LessEqual => {
            assembly.push_str("    setbe al\n    setnp cl\n    and al, cl\n    movzx eax, al\n")
        }
        CompareKind::Greater => assembly.push_str("    seta al\n    movzx eax, al\n"),
        CompareKind::GreaterEqual => assembly.push_str("    setae al\n    movzx eax, al\n"),
    }
    store_value(assembly, function, destination, "rax")
}

fn load_float_operands(
    function: &LirFunction,
    lhs: ValueId,
    rhs: ValueId,
    width: u16,
    assembly: &mut String,
) -> Result<(), String> {
    load_value(assembly, function, lhs, "rax")?;
    load_float_bits_to_xmm(assembly, "rax", 0, width)?;
    load_value(assembly, function, rhs, "rax")?;
    load_float_bits_to_xmm(assembly, "rax", 1, width)
}

fn load_float_bits_to_xmm(
    assembly: &mut String,
    gpr: &str,
    xmm: usize,
    width: u16,
) -> Result<(), String> {
    let low = dword_register(gpr)?;
    match width {
        16 => assembly.push_str(&format!(
            "    movd xmm{xmm}, {low}\n    vcvtph2ps xmm{xmm}, xmm{xmm}\n"
        )),
        32 => assembly.push_str(&format!("    movd xmm{xmm}, {low}\n")),
        64 => assembly.push_str(&format!("    movq xmm{xmm}, {gpr}\n")),
        _ => return Err(format!("unsupported x86_64 float width {width}")),
    }
    Ok(())
}

fn load_float_abi_bits_to_xmm(
    assembly: &mut String,
    gpr: &str,
    xmm: usize,
    width: u16,
) -> Result<(), String> {
    let low = dword_register(gpr)?;
    match width {
        16 | 32 => assembly.push_str(&format!("    movd xmm{xmm}, {low}\n")),
        64 => assembly.push_str(&format!("    movq xmm{xmm}, {gpr}\n")),
        _ => return Err(format!("unsupported x86_64 float ABI width {width}")),
    }
    Ok(())
}

fn load_float_bits_from_xmm(
    assembly: &mut String,
    xmm: usize,
    width: u16,
    gpr: &str,
) -> Result<(), String> {
    let low = dword_register(gpr)?;
    match width {
        16 => assembly.push_str(&format!("    pextrw {low}, xmm{xmm}, 0\n")),
        32 => assembly.push_str(&format!("    movd {low}, xmm{xmm}\n")),
        64 => assembly.push_str(&format!("    movq {gpr}, xmm{xmm}\n")),
        _ => return Err(format!("unsupported x86_64 float width {width}")),
    }
    Ok(())
}

fn load_float_abi_bits_from_xmm(
    assembly: &mut String,
    xmm: usize,
    width: u16,
    gpr: &str,
) -> Result<(), String> {
    let low = dword_register(gpr)?;
    match width {
        16 => assembly.push_str(&format!("    pextrw {low}, xmm{xmm}, 0\n")),
        32 => assembly.push_str(&format!("    movd {low}, xmm{xmm}\n")),
        64 => assembly.push_str(&format!("    movq {gpr}, xmm{xmm}\n")),
        _ => return Err(format!("unsupported x86_64 float ABI width {width}")),
    }
    Ok(())
}

fn dword_register(register: &str) -> Result<&'static str, String> {
    match register {
        "rax" => Ok("eax"),
        "rcx" => Ok("ecx"),
        "rdx" => Ok("edx"),
        "rsi" => Ok("esi"),
        "rdi" => Ok("edi"),
        "r8" => Ok("r8d"),
        "r9" => Ok("r9d"),
        "r10" => Ok("r10d"),
        "r11" => Ok("r11d"),
        _ => Err(format!("no 32-bit x86_64 register for '{register}'")),
    }
}

fn store_float_result(
    function: &LirFunction,
    destination: ValueId,
    width: u16,
    assembly: &mut String,
) -> Result<(), String> {
    if width == 16 {
        assembly.push_str("    vcvtps2ph xmm0, xmm0, 0\n    pextrw eax, xmm0, 0\n");
        store_value(assembly, function, destination, "rax")
    } else {
        load_float_bits_from_xmm(assembly, 0, width, "rax")?;
        store_value(assembly, function, destination, "rax")
    }
}

fn emit_phi_copies(
    function: &LirFunction,
    from: crate::mir::BlockId,
    to: crate::mir::BlockId,
    assembly: &mut String,
) -> Result<(), String> {
    let copies = crate::codegen::phi_copies(function, from, to);
    if copies.is_empty() {
        return Ok(());
    }
    let scratch_size = copies.len() as u64 * 8;
    assembly.push_str(&format!("    sub rsp, {scratch_size}\n"));
    for (index, (_, source)) in copies.iter().enumerate() {
        load_value(assembly, function, *source, "rax")?;
        assembly.push_str(&format!("    mov qword ptr [rsp + {}], rax\n", index * 8));
    }
    for (index, (destination, _)) in copies.iter().enumerate() {
        assembly.push_str(&format!("    mov rax, qword ptr [rsp + {}]\n", index * 8));
        store_value(assembly, function, *destination, "rax")?;
    }
    assembly.push_str(&format!("    add rsp, {scratch_size}\n"));
    Ok(())
}

fn value_count(function: &LirFunction) -> u32 {
    let mut maximum = function
        .params
        .iter()
        .chain(function.float_params.iter().map(|(value, _)| value))
        .map(|value| value.0)
        .max();
    let mut values = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            instruction_values(instruction, &mut values);
        }
        match block.terminator {
            MirTerminator::Return {
                value,
                slice_length,
            } => values.extend(value.into_iter().chain(slice_length)),
            MirTerminator::Branch { condition, .. } => values.push(condition),
            MirTerminator::Jump(_) | MirTerminator::Unreachable => {}
        }
    }
    for value in values {
        maximum = Some(maximum.map_or(value.0, |current| current.max(value.0)));
    }
    maximum.map_or(0, |value| value.saturating_add(1))
}

fn instruction_values(instruction: &LirInstruction, values: &mut Vec<ValueId>) {
    match instruction {
        LirInstruction::LoadImmediate { register, .. }
        | LirInstruction::LoadAddress { register, .. } => values.push(*register),
        LirInstruction::Call {
            result,
            result_slice_length,
            args,
            ..
        } => {
            values.extend(result.iter().chain(result_slice_length.iter()).copied());
            values.extend(args);
        }
        LirInstruction::CallIndirect {
            result,
            result_slice_length,
            callee,
            args,
            ..
        } => {
            values.extend(result.iter().chain(result_slice_length.iter()).copied());
            values.push(*callee);
            values.extend(args);
        }
        LirInstruction::Add {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Sub {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Mul {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Div {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Rem {
            destination,
            lhs,
            rhs,
        }
        | LirInstruction::Compare {
            destination,
            lhs,
            rhs,
            ..
        }
        | LirInstruction::FloatBinary {
            destination,
            lhs,
            rhs,
            ..
        }
        | LirInstruction::FloatCompare {
            destination,
            lhs,
            rhs,
            ..
        } => values.extend([*destination, *lhs, *rhs]),
        LirInstruction::Neg { destination, value }
        | LirInstruction::Not { destination, value }
        | LirInstruction::FloatNeg {
            destination, value, ..
        } => {
            values.extend([*destination, *value]);
        }
        LirInstruction::Phi {
            destination,
            incoming,
        } => {
            values.push(*destination);
            values.extend(incoming.iter().map(|(_, value)| *value));
        }
        LirInstruction::LoadSlot { destination, .. }
        | LirInstruction::SlotAddress { destination, .. } => values.push(*destination),
        LirInstruction::StoreSlot { value, .. } => values.push(*value),
        LirInstruction::IndexAddress {
            destination,
            base,
            index,
            ..
        } => values.extend([*destination, *base, *index]),
        LirInstruction::LoadIndirect {
            destination,
            address,
        }
        | LirInstruction::LoadByteIndirect {
            destination,
            address,
        }
        | LirInstruction::LoadSizedIndirect {
            destination,
            address,
            ..
        } => values.extend([*destination, *address]),
        LirInstruction::StoreIndirect { address, value }
        | LirInstruction::StoreByteIndirect { address, value }
        | LirInstruction::StoreSizedIndirect { address, value, .. } => {
            values.extend([*address, *value]);
        }
    }
}

fn value_offset(function: &LirFunction, value: ValueId) -> Result<u64, String> {
    u64::from(function.slot_count)
        .checked_add(u64::from(value.0))
        .and_then(|index| index.checked_add(1))
        .and_then(|index| index.checked_mul(8))
        .ok_or_else(|| "x86_64 value slot offset overflow".into())
}

fn slot_offset(function: &LirFunction, slot: SlotId) -> Result<u64, String> {
    if slot.0 >= function.slot_count {
        return Err(format!("invalid x86_64 stack slot {}", slot.0));
    }
    u64::from(function.slot_count - slot.0)
        .checked_mul(8)
        .ok_or_else(|| "x86_64 stack slot offset overflow".into())
}

fn load_value(
    assembly: &mut String,
    function: &LirFunction,
    value: ValueId,
    register: &str,
) -> Result<(), String> {
    assembly.push_str(&format!(
        "    mov {register}, qword ptr [rbp - {}]\n",
        value_offset(function, value)?
    ));
    Ok(())
}

fn store_value(
    assembly: &mut String,
    function: &LirFunction,
    value: ValueId,
    register: &str,
) -> Result<(), String> {
    assembly.push_str(&format!(
        "    mov qword ptr [rbp - {}], {register}\n",
        value_offset(function, value)?
    ));
    Ok(())
}

fn slot_mem(function: &LirFunction, slot: SlotId) -> Result<String, String> {
    if slot.0 >= function.slot_count {
        return Err(format!("invalid x86_64 stack slot {}", slot.0));
    }
    Ok(format!(
        "qword ptr [rbp - {}]",
        slot_offset(function, slot)?
    ))
}

fn symbol(name: &str) -> String {
    if cfg!(target_os = "macos") {
        if name.starts_with('_') {
            name.to_string()
        } else {
            format!("_{name}")
        }
    } else {
        name.strip_prefix('_').unwrap_or(name).to_string()
    }
}
