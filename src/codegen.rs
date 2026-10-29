use std::path::Path;
use std::process::Command;

use crate::hir::HirType;
use crate::lir::{LirFunction, LirInstruction, LirModule, TargetArch};
use crate::mir::{CompareKind, FloatBinaryKind, MirTerminator, ValueId};
use crate::regalloc::allocate_registers;

#[derive(Clone, Copy)]
enum RelocationKind {
    Branch26,
    Page21,
    PageOff12,
    GotPage21,
    GotPageOff12,
}

struct Relocation {
    offset: u32,
    symbol: String,
    kind: RelocationKind,
}

struct ObjectSymbol {
    name: String,
    section: u8,
    value: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LinkOptions {
    pub(crate) library_paths: Vec<std::path::PathBuf>,
    pub(crate) libraries: Vec<String>,
}

pub(crate) fn emit_assembly(module: &LirModule) -> Result<String, String> {
    match module.target {
        TargetArch::Aarch64 => {
            validate_data_layout(module)?;
            emit_aarch64(module)
        }
        TargetArch::X86_64 => {
            validate_data_layout(module)?;
            crate::codegen_x86_64::emit_assembly(module)
        }
    }
}

pub(crate) fn compile_executable(module: &LirModule, output: &Path) -> Result<(), String> {
    compile_executable_with_options(module, output, &LinkOptions::default())
}

pub(crate) fn compile_executable_with_options(
    module: &LirModule,
    output: &Path,
    options: &LinkOptions,
) -> Result<(), String> {
    validate_data_layout(module)?;
    if module.target == TargetArch::X86_64 {
        return compile_x86_64(module, output, options);
    }
    let (text, data, symbols, relocations) = encode_module(module)?;
    let object = write_macho_object(&text, &data, &symbols, &relocations);
    let object_path = output.with_extension("o");
    std::fs::write(&object_path, object)
        .map_err(|error| format!("cannot write object file: {error}"))?;
    let float8_object = match compile_float8_runtime(module, output) {
        Ok(object) => object,
        Err(error) => {
            let _ = std::fs::remove_file(&object_path);
            return Err(error);
        }
    };
    let sdk = Command::new("xcrun")
        .arg("--show-sdk-path")
        .output()
        .map_err(|error| format!("cannot locate macOS SDK: {error}"))?;
    if !sdk.status.success() {
        let _ = std::fs::remove_file(&object_path);
        return Err(String::from_utf8_lossy(&sdk.stderr).into_owned());
    }
    let sdk_path = String::from_utf8_lossy(&sdk.stdout).trim().to_string();
    let mut command = Command::new("ld");
    command.args([
        "-arch",
        "arm64",
        "-platform_version",
        "macos",
        "15.0",
        "15.0",
        "-e",
        "_main",
        "-syslibroot",
        &sdk_path,
        "-lSystem",
    ]);
    for path in &options.library_paths {
        command.arg("-L").arg(path);
    }
    for library in &options.libraries {
        command.arg(format!("-l{library}"));
    }
    command.arg(&object_path);
    if let Some(runtime) = &float8_object {
        command.arg(runtime);
    }
    let result = command.arg("-o").arg(output).output();
    let _ = std::fs::remove_file(&object_path);
    if let Some(runtime) = float8_object {
        let _ = std::fs::remove_file(runtime);
    }
    let result = result.map_err(|error| format!("cannot invoke ld: {error}"))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).into_owned())
    }
}

fn compile_x86_64(module: &LirModule, output: &Path, options: &LinkOptions) -> Result<(), String> {
    let assembly = emit_assembly(module)?;
    let assembly_path = output.with_extension("s");
    let object_path = output.with_extension("o");
    std::fs::write(&assembly_path, assembly)
        .map_err(|error| format!("cannot write x86_64 assembly: {error}"))?;

    let mut assembler = Command::new("cc");
    if cfg!(target_os = "macos") {
        assembler.args(["-arch", "x86_64"]);
    }
    let assembled = assembler
        .arg("-c")
        .arg(&assembly_path)
        .arg("-o")
        .arg(&object_path)
        .output()
        .map_err(|error| format!("cannot invoke x86_64 assembler: {error}"))?;
    if !assembled.status.success() {
        let _ = std::fs::remove_file(&assembly_path);
        let _ = std::fs::remove_file(&object_path);
        return Err(String::from_utf8_lossy(&assembled.stderr).into_owned());
    }

    let float8_object = match compile_float8_runtime(module, output) {
        Ok(object) => object,
        Err(error) => {
            let _ = std::fs::remove_file(&assembly_path);
            let _ = std::fs::remove_file(&object_path);
            return Err(error);
        }
    };

    let mut linker = Command::new("cc");
    if cfg!(target_os = "macos") {
        linker.args(["-arch", "x86_64"]);
    }
    for path in &options.library_paths {
        linker.arg("-L").arg(path);
    }
    linker.arg(&object_path);
    if let Some(runtime) = &float8_object {
        linker.arg(runtime);
    }
    for library in &options.libraries {
        linker.arg(format!("-l{library}"));
    }
    let linked = linker.arg("-o").arg(output).output();
    let _ = std::fs::remove_file(&assembly_path);
    let _ = std::fs::remove_file(&object_path);
    if let Some(runtime) = float8_object {
        let _ = std::fs::remove_file(runtime);
    }
    let linked = linked.map_err(|error| format!("cannot invoke x86_64 linker: {error}"))?;
    if linked.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&linked.stderr).into_owned())
    }
}

fn compile_float8_runtime(
    module: &LirModule,
    output: &Path,
) -> Result<Option<std::path::PathBuf>, String> {
    let needed = module.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(instruction, LirInstruction::Call { name, .. } if name.starts_with("mimi_f8_"))
            })
        })
    });
    if !needed {
        return Ok(None);
    }
    let source_path = output.with_extension("f8.c");
    let object_path = output.with_extension("f8.o");
    std::fs::write(&source_path, include_str!("../runtime/f8.c"))
        .map_err(|error| format!("cannot write f8 runtime source: {error}"))?;
    let mut compiler = Command::new("cc");
    if cfg!(target_os = "macos") {
        compiler.args([
            "-arch",
            match module.target {
                TargetArch::Aarch64 => "arm64",
                TargetArch::X86_64 => "x86_64",
            },
        ]);
    }
    let result = compiler
        .args(["-std=c11", "-O2", "-c"])
        .arg(&source_path)
        .arg("-o")
        .arg(&object_path)
        .output();
    let _ = std::fs::remove_file(&source_path);
    let result = result.map_err(|error| format!("cannot compile f8 runtime: {error}"))?;
    if !result.status.success() {
        let _ = std::fs::remove_file(&object_path);
        return Err(String::from_utf8_lossy(&result.stderr).into_owned());
    }
    Ok(Some(object_path))
}

fn validate_data_layout(module: &LirModule) -> Result<(), String> {
    if module.data_layout.pointer_bytes() != 8 || module.data_layout.stack_slot_bytes() != 8 {
        let target = match module.target {
            TargetArch::Aarch64 => "ARM64",
            TargetArch::X86_64 => "x86_64",
        };
        return Err(format!(
            "{target} backend requires 8-byte pointers and stack slots"
        ));
    }
    Ok(())
}

fn encode_module(
    module: &LirModule,
) -> Result<(Vec<u8>, Vec<u8>, Vec<ObjectSymbol>, Vec<Relocation>), String> {
    let functions = module
        .functions
        .iter()
        .map(allocate_registers)
        .collect::<Result<Vec<_>, _>>()?;
    let mut offsets = std::collections::HashMap::new();
    let mut offset = 0u64;
    for function in &functions {
        offsets.insert(format!("_{}", function.name), offset);
        offset += encoded_function_size(function)? as u64;
    }
    let mut text = Vec::new();
    let mut symbols = Vec::new();
    let mut relocations = Vec::new();
    for function in &functions {
        symbols.push(ObjectSymbol {
            name: format!("_{}", function.name),
            section: 1,
            value: text.len() as u64,
        });
        encode_function(function, &mut text, &offsets, &mut relocations)?;
    }
    let mut data = Vec::new();
    for (name, bytes) in &module.strings {
        symbols.push(ObjectSymbol {
            name: name.clone(),
            section: 2,
            value: data.len() as u64,
        });
        data.extend_from_slice(bytes);
    }
    for relocation in &relocations {
        if !symbols
            .iter()
            .any(|symbol| symbol.name == relocation.symbol)
        {
            symbols.push(ObjectSymbol {
                name: relocation.symbol.clone(),
                section: 0,
                value: 0,
            });
        }
    }
    Ok((text, data, symbols, relocations))
}

fn encoded_function_size(function: &LirFunction) -> Result<usize, String> {
    if function.blocks.is_empty() {
        return Err("function has no basic blocks".into());
    }
    let mut size = 0;
    let has_calls = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(
                instruction,
                LirInstruction::Call { .. } | LirInstruction::CallIndirect { .. }
            )
        });
    let saved_registers = callee_saved_registers(function);
    let frame_size = stack_frame_size(function);
    let save_frame = saves_frame_pointer(has_calls);
    size += function.params.len().saturating_sub(8) * 4;
    size += function.float_params.len() * 4;
    size += saved_registers
        .iter()
        .enumerate()
        .map(|(index, _)| stack_access_words(saved_register_offset(function, index)) * 4)
        .sum::<usize>();
    for block in &function.blocks {
        for instruction in &block.instructions {
            size += match instruction {
                LirInstruction::LoadImmediate { value, .. } => {
                    let mut chunks = 0;
                    for shift in 0..4 {
                        if ((*value as u64 >> (shift * 16)) & 0xffff) != 0 {
                            chunks += 1;
                        }
                    }
                    if chunks == 0 { 1 } else { chunks }
                }
                LirInstruction::LoadAddress { .. } => 2,
                LirInstruction::Add { .. }
                | LirInstruction::Sub { .. }
                | LirInstruction::Mul { .. }
                | LirInstruction::Div { .. }
                | LirInstruction::Neg { .. } => 1,
                LirInstruction::FloatBinary { .. } => 4,
                LirInstruction::FloatNeg { .. } => 3,
                LirInstruction::LoadSlot { .. } | LirInstruction::StoreSlot { .. } => 1,
                LirInstruction::SlotAddress { slot, .. } => slot_address_words(*slot),
                LirInstruction::IndexAddress { scale, .. } => index_address_words(*scale)?,
                LirInstruction::LoadIndirect { .. }
                | LirInstruction::LoadByteIndirect { .. }
                | LirInstruction::LoadSizedIndirect { .. }
                | LirInstruction::StoreIndirect { .. }
                | LirInstruction::StoreByteIndirect { .. }
                | LirInstruction::StoreSizedIndirect { .. } => 1,
                LirInstruction::Not { .. } => 2,
                LirInstruction::Rem { .. } | LirInstruction::Compare { .. } => 2,
                LirInstruction::FloatCompare { .. } => 4,
                LirInstruction::Phi { .. } => 0,
                LirInstruction::Call {
                    args,
                    result,
                    result_slice_length,
                    float_args,
                    result_float,
                    ..
                } => encoded_call_words(
                    args,
                    *result,
                    *result_slice_length,
                    float_args,
                    *result_float,
                    false,
                )?,
                LirInstruction::CallIndirect {
                    args,
                    result,
                    result_slice_length,
                    float_args,
                    result_float,
                    ..
                } => encoded_call_words(
                    args,
                    *result,
                    *result_slice_length,
                    float_args,
                    *result_float,
                    true,
                )?,
            } * 4;
        }
        size += match block.terminator {
            MirTerminator::Return {
                value,
                slice_length,
            } => {
                function_return_moves(value, slice_length, &function.result) * 4
                    + if function.name == "main" {
                        8 + usize::from(value.is_none()) * 4
                    } else {
                        (1 + usize::from(save_frame)
                            + saved_registers
                                .iter()
                                .enumerate()
                                .map(|(index, _)| {
                                    stack_access_words(saved_register_offset(function, index))
                                })
                                .sum::<usize>()
                            + stack_adjust_words(frame_size))
                            * 4
                    }
            }
            MirTerminator::Jump(target) => 4 + phi_move_count(function, block.id, target) * 4,
            MirTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                12 + (phi_move_count(function, block.id, then_block)
                    + phi_move_count(function, block.id, else_block))
                    * 4
            }
            MirTerminator::Unreachable => {
                return Err("unreachable terminator reached ARM64 backend".into());
            }
        };
    }
    if save_frame {
        size += 4;
    }
    size += stack_adjust_words(frame_size) * 4;
    Ok(size)
}

fn stack_frame_size(function: &LirFunction) -> u32 {
    function
        .slot_count
        .saturating_add(callee_saved_registers(function).len() as u32)
        .saturating_mul(8)
        .next_multiple_of(16)
}

fn callee_saved_registers(function: &LirFunction) -> Vec<u32> {
    if function.name == "main" {
        return Vec::new();
    }
    let mut registers = function
        .params
        .iter()
        .map(|value| value.0)
        .chain(
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .flat_map(instruction_registers)
                .map(|value| value.0),
        )
        .filter(|register| (19..=28).contains(register))
        .collect::<Vec<_>>();
    registers.sort_unstable();
    registers.dedup();
    registers
}

fn saves_frame_pointer(has_calls: bool) -> bool {
    has_calls
}

fn saved_register_offset(function: &LirFunction, index: usize) -> u32 {
    function
        .slot_count
        .saturating_mul(8)
        .saturating_add((index as u32).saturating_mul(8))
}

fn stack_access_words(offset: u32) -> usize {
    if offset <= 32760 {
        1
    } else {
        slot_address_words(crate::mir::SlotId(offset)) + 1
    }
}

fn stack_adjust_words(size: u32) -> usize {
    size.div_ceil(4095) as usize
}

fn stack_argument_size(argument_count: usize) -> u32 {
    (argument_count.saturating_sub(8) as u32 * 8).next_multiple_of(16)
}

fn integer_arguments(
    args: &[ValueId],
    float_args: &[(usize, u16)],
) -> Result<Vec<ValueId>, String> {
    let mut float_positions = std::collections::HashSet::new();
    for (position, width) in float_args {
        if *position >= args.len() || !float_positions.insert(*position) {
            return Err("invalid floating-point argument metadata".into());
        }
        validate_float_width(*width)?;
    }
    if float_args.len() > 8 {
        return Err("ARM64 backend currently supports at most 8 floating-point arguments".into());
    }
    Ok(args
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (!float_positions.contains(&index)).then_some(*value))
        .collect())
}

fn encoded_call_words(
    args: &[ValueId],
    result: Option<ValueId>,
    result_slice_length: Option<ValueId>,
    float_args: &[(usize, u16)],
    result_float: Option<u16>,
    indirect: bool,
) -> Result<usize, String> {
    if let Some(width) = result_float {
        validate_float_width(width)?;
    }
    let integer_args = integer_arguments(args, float_args)?;
    if integer_args.len() > 16 {
        return Err("ARM64 backend currently supports at most 16 integer arguments".into());
    }
    let stack_args = integer_args.len().saturating_sub(8);
    Ok(float_args.len()
        + argument_moves(&integer_args[..integer_args.len().min(8)]).len()
        + stack_args
        + stack_adjust_words(stack_argument_size(integer_args.len())) * 2
        + 1
        + usize::from(indirect)
        + call_result_moves(result, result_slice_length, result_float))
}

fn call_result_moves(
    result: Option<ValueId>,
    result_slice_length: Option<ValueId>,
    result_float: Option<u16>,
) -> usize {
    if let (Some(pointer), Some(length)) = (result, result_slice_length) {
        parallel_moves(&[(pointer.0, 0), (length.0, 1)]).len()
    } else {
        usize::from(result.is_some_and(|value| value.0 != 0 || result_float.is_some()))
    }
}

fn function_return_moves(
    value: Option<ValueId>,
    slice_length: Option<ValueId>,
    result_type: &HirType,
) -> usize {
    if let (Some(pointer), Some(length)) = (value, slice_length) {
        argument_moves(&[pointer, length]).len()
    } else {
        usize::from(value.is_some_and(|value| value.0 != 0 || float_width(result_type).is_some()))
    }
}

fn validate_float_width(width: u16) -> Result<(), String> {
    match width {
        16 | 32 | 64 => Ok(()),
        _ => Err(format!(
            "ARM64 backend does not support f{width} values yet"
        )),
    }
}

fn index_address_words(scale: u64) -> Result<usize, String> {
    if scale == 0 {
        return Err("ARM64 index scale cannot be zero".into());
    }
    if scale.is_power_of_two() {
        return Ok(1);
    }
    let immediate_words = (0..4)
        .filter(|shift| ((scale >> (shift * 16)) & 0xffff) != 0)
        .count()
        .max(1);
    Ok(immediate_words + 2)
}

fn encode_index_address(
    bytes: &mut Vec<u8>,
    destination: ValueId,
    base: ValueId,
    index: ValueId,
    scale: u64,
) -> Result<(), String> {
    if scale == 0 {
        return Err("ARM64 index scale cannot be zero".into());
    }
    if scale.is_power_of_two() {
        push_word(
            bytes,
            0x8b00_0000
                | (index.0 << 16)
                | (scale.trailing_zeros() << 10)
                | (base.0 << 5)
                | destination.0,
        );
    } else {
        encode_immediate(16, scale, bytes)?;
        push_word(bytes, 0x9b00_7c00 | (16 << 16) | (index.0 << 5) | 17);
        push_word(
            bytes,
            0x8b00_0000 | (17 << 16) | (base.0 << 5) | destination.0,
        );
    }
    Ok(())
}

fn slot_address_words(slot: crate::mir::SlotId) -> usize {
    (slot.0 * 8).div_ceil(4095).max(1) as usize
}

pub(crate) fn phi_copies(
    function: &LirFunction,
    from: crate::mir::BlockId,
    to: crate::mir::BlockId,
) -> Vec<(ValueId, ValueId)> {
    function
        .blocks
        .iter()
        .find(|block| block.id == to)
        .into_iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            LirInstruction::Phi {
                destination,
                incoming,
            } => incoming
                .iter()
                .find(|(block, _)| *block == from)
                .map(|(_, source)| (*destination, *source)),
            _ => None,
        })
        .filter(|(destination, source)| destination != source)
        .collect()
}

fn phi_move_count(
    function: &LirFunction,
    from: crate::mir::BlockId,
    to: crate::mir::BlockId,
) -> usize {
    let moves = phi_copies(function, from, to)
        .into_iter()
        .map(|(destination, source)| (destination.0, source.0))
        .collect::<Vec<_>>();
    parallel_moves(&moves).len()
}

fn emit_phi_copies(
    function: &LirFunction,
    from: crate::mir::BlockId,
    to: crate::mir::BlockId,
    bytes: &mut Vec<u8>,
) {
    let moves = phi_copies(function, from, to)
        .into_iter()
        .map(|(destination, source)| (destination.0, source.0))
        .collect::<Vec<_>>();
    for (destination, source) in parallel_moves(&moves) {
        push_word(bytes, 0xaa00_03e0 | (source << 16) | destination);
    }
}

fn encode_function(
    function: &LirFunction,
    bytes: &mut Vec<u8>,
    offsets: &std::collections::HashMap<String, u64>,
    relocations: &mut Vec<Relocation>,
) -> Result<(), String> {
    if function.params.len() > 16 {
        return Err("ARM64 backend currently supports at most 16 integer parameters".into());
    }
    if function.float_params.len() > 8 {
        return Err("ARM64 backend currently supports at most 8 floating-point parameters".into());
    }
    let has_calls = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(
                instruction,
                LirInstruction::Call { .. } | LirInstruction::CallIndirect { .. }
            )
        });
    let saved_registers = callee_saved_registers(function);
    let frame_size = stack_frame_size(function);
    let save_frame = saves_frame_pointer(has_calls);
    if save_frame {
        push_word(bytes, 0xa9bf_7bfd);
    }
    encode_stack_adjust(bytes, frame_size, true)?;
    for (index, register) in saved_registers.iter().enumerate() {
        encode_stack_store(bytes, *register, saved_register_offset(function, index));
    }
    for (index, parameter) in function.params.iter().enumerate().skip(8) {
        let offset = frame_size + u32::from(save_frame) * 16 + (index as u32 - 8) * 8;
        if offset > 32760 {
            return Err("ARM64 stack-passed parameter offset exceeds the load range".into());
        }
        push_word(bytes, 0xf940_03e0 | ((offset / 8) << 10) | parameter.0);
    }
    for (index, (parameter, width)) in function.float_params.iter().enumerate() {
        encode_fp_to_gpr(bytes, parameter.0, index as u32, *width)?;
    }
    let mut block_offsets = std::collections::HashMap::new();
    let mut cursor = bytes.len();
    for block in &function.blocks {
        block_offsets.insert(block.id, cursor);
        cursor += block_encoded_size(function, block)?;
    }
    let mut branches = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let LirInstruction::IndexAddress {
                destination,
                base,
                index,
                scale,
            } = instruction
            {
                encode_index_address(bytes, *destination, *base, *index, *scale)?;
                continue;
            }
            let word = match instruction {
                LirInstruction::LoadImmediate { register, value } => {
                    encode_immediate(register.0, *value as u64, bytes)?;
                    continue;
                }
                LirInstruction::LoadAddress {
                    register,
                    symbol,
                    external,
                } => {
                    let page = bytes.len() as u32;
                    push_word(bytes, 0x9000_0000 | register.0);
                    let low = bytes.len() as u32;
                    if *external {
                        push_word(bytes, 0xf940_0000 | (register.0 << 5) | register.0);
                    } else {
                        push_word(bytes, 0x9100_0000 | (register.0 << 5) | register.0);
                    }
                    relocations.push(Relocation {
                        offset: page,
                        symbol: symbol.clone(),
                        kind: if *external {
                            RelocationKind::GotPage21
                        } else {
                            RelocationKind::Page21
                        },
                    });
                    relocations.push(Relocation {
                        offset: low,
                        symbol: symbol.clone(),
                        kind: if *external {
                            RelocationKind::GotPageOff12
                        } else {
                            RelocationKind::PageOff12
                        },
                    });
                    continue;
                }
                LirInstruction::Add {
                    destination,
                    lhs,
                    rhs,
                } => 0x8b00_0000 | (rhs.0 << 16) | (lhs.0 << 5) | destination.0,
                LirInstruction::FloatBinary {
                    destination,
                    lhs,
                    rhs,
                    kind,
                    width,
                } => {
                    encode_float_binary(*destination, *lhs, *rhs, *kind, *width, bytes)?;
                    continue;
                }
                LirInstruction::Sub {
                    destination,
                    lhs,
                    rhs,
                } => 0xcb00_0000 | (rhs.0 << 16) | (lhs.0 << 5) | destination.0,
                LirInstruction::Mul {
                    destination,
                    lhs,
                    rhs,
                } => 0x9b00_7c00 | (rhs.0 << 16) | (lhs.0 << 5) | destination.0,
                LirInstruction::Div {
                    destination,
                    lhs,
                    rhs,
                } => 0x9ac0_0c00 | (rhs.0 << 16) | (lhs.0 << 5) | destination.0,
                LirInstruction::Not { destination, value } => {
                    push_word(bytes, 0xf100_001f | (value.0 << 5));
                    push_word(bytes, 0x9a9f_17e0 | destination.0);
                    continue;
                }
                LirInstruction::FloatNeg {
                    destination,
                    value,
                    width,
                } => {
                    encode_float_neg(*destination, *value, *width, bytes)?;
                    continue;
                }
                LirInstruction::Neg { destination, value } => {
                    push_word(bytes, 0xcb00_03e0 | (value.0 << 16) | destination.0);
                    continue;
                }
                LirInstruction::LoadSlot { destination, slot } => {
                    let offset = slot.0 * 8;
                    if offset > 32760 {
                        return Err("ARM64 stack slot offset exceeds the load/store range".into());
                    }
                    push_word(bytes, 0xf940_03e0 | ((offset / 8) << 10) | destination.0);
                    continue;
                }
                LirInstruction::SlotAddress { destination, slot } => {
                    encode_slot_address(bytes, destination.0, slot.0 * 8);
                    continue;
                }
                LirInstruction::IndexAddress { .. } => unreachable!(),
                LirInstruction::LoadIndirect {
                    destination,
                    address,
                } => 0xf940_0000 | (address.0 << 5) | destination.0,
                LirInstruction::LoadByteIndirect {
                    destination,
                    address,
                } => 0x3940_0000 | (address.0 << 5) | destination.0,
                LirInstruction::LoadSizedIndirect {
                    destination,
                    address,
                    bytes,
                    signed,
                } => {
                    let base = match (bytes, signed) {
                        (1, false) => 0x3940_0000,
                        (1, true) => 0x3980_0000,
                        (2, false) => 0x7940_0000,
                        (2, true) => 0x7980_0000,
                        (4, false) => 0xb940_0000,
                        (4, true) => 0xb980_0000,
                        _ => return Err(format!("unsupported ARM64 load width {bytes}")),
                    };
                    base | (address.0 << 5) | destination.0
                }
                LirInstruction::StoreIndirect { address, value } => {
                    0xf900_0000 | (address.0 << 5) | value.0
                }
                LirInstruction::StoreByteIndirect { address, value } => {
                    0x3900_0000 | (address.0 << 5) | value.0
                }
                LirInstruction::StoreSizedIndirect {
                    address,
                    value,
                    bytes,
                } => {
                    let base = match bytes {
                        2 => 0x7900_0000,
                        4 => 0xb900_0000,
                        _ => return Err(format!("unsupported ARM64 store width {bytes}")),
                    };
                    base | (address.0 << 5) | value.0
                }
                LirInstruction::StoreSlot { slot, value } => {
                    let offset = slot.0 * 8;
                    if offset > 32760 {
                        return Err("ARM64 stack slot offset exceeds the load/store range".into());
                    }
                    push_word(bytes, 0xf900_03e0 | ((offset / 8) << 10) | value.0);
                    continue;
                }
                LirInstruction::Rem {
                    destination,
                    lhs,
                    rhs,
                } => {
                    push_word(bytes, 0x9ac0_0c10 | (rhs.0 << 16) | (lhs.0 << 5));
                    push_word(
                        bytes,
                        0x9b00_8000 | (rhs.0 << 16) | (lhs.0 << 10) | (16 << 5) | destination.0,
                    );
                    continue;
                }
                LirInstruction::Compare {
                    destination,
                    lhs,
                    rhs,
                    kind,
                } => {
                    push_word(bytes, 0xeb00_001f | (rhs.0 << 16) | (lhs.0 << 5));
                    let condition = match kind {
                        CompareKind::Equal => 0,
                        CompareKind::NotEqual => 1,
                        CompareKind::GreaterEqual => 10,
                        CompareKind::Less => 11,
                        CompareKind::Greater => 12,
                        CompareKind::LessEqual => 13,
                    };
                    push_word(
                        bytes,
                        0x9a9f_07e0 | (((condition ^ 1) as u32) << 12) | destination.0,
                    );
                    continue;
                }
                LirInstruction::FloatCompare {
                    destination,
                    lhs,
                    rhs,
                    kind,
                    width,
                } => {
                    encode_float_compare(*destination, *lhs, *rhs, *kind, *width, bytes)?;
                    continue;
                }
                LirInstruction::Call {
                    result,
                    result_slice_length,
                    name,
                    args,
                    float_args,
                    result_float,
                } => {
                    let integer_args = integer_arguments(args, float_args)?;
                    if integer_args.len() > 16 {
                        return Err(
                            "ARM64 backend currently supports at most 16 integer arguments".into(),
                        );
                    }
                    let stack_size = stack_argument_size(integer_args.len());
                    encode_stack_adjust(bytes, stack_size, true)?;
                    for (index, argument) in integer_args.iter().enumerate().skip(8) {
                        let offset = (index as u32 - 8) * 8;
                        if offset > 32760 {
                            return Err(
                                "ARM64 outgoing argument offset exceeds the store range".into()
                            );
                        }
                        push_word(bytes, 0xf900_03e0 | ((offset / 8) << 10) | argument.0);
                    }
                    encode_float_argument_moves(args, float_args, bytes)?;
                    encode_parallel_argument_moves(
                        &integer_args[..integer_args.len().min(8)],
                        bytes,
                    );
                    let call_address = bytes.len() as u64;
                    let target_name = format!("_{name}");
                    if let Some(target) = offsets.get(&target_name) {
                        let delta = (*target as i64 - call_address as i64) / 4;
                        if !(-(1 << 25)..(1 << 25)).contains(&delta) {
                            return Err(format!(
                                "call target '{name}' is out of ARM64 branch range"
                            ));
                        }
                        push_word(bytes, 0x9400_0000 | (delta as u32 & 0x03ff_ffff));
                    } else {
                        relocations.push(Relocation {
                            offset: call_address as u32,
                            symbol: target_name,
                            kind: RelocationKind::Branch26,
                        });
                        push_word(bytes, 0x9400_0000);
                    }
                    encode_stack_adjust(bytes, stack_size, false)?;
                    if let (Some(pointer), Some(length)) = (result, result_slice_length) {
                        encode_register_moves(
                            bytes,
                            &parallel_moves(&[(pointer.0, 0), (length.0, 1)]),
                        );
                    } else if let Some(result) = result {
                        if let Some(width) = result_float {
                            encode_fp_to_gpr(bytes, result.0, 0, *width)?;
                        } else if result.0 != 0 {
                            push_word(bytes, 0xaa00_03e0 | result.0);
                        }
                    }
                    continue;
                }
                LirInstruction::CallIndirect {
                    result,
                    result_slice_length,
                    callee,
                    args,
                    float_args,
                    result_float,
                } => {
                    let integer_args = integer_arguments(args, float_args)?;
                    if integer_args.len() > 16 {
                        return Err(
                            "ARM64 backend currently supports at most 16 integer arguments".into(),
                        );
                    }
                    let stack_size = stack_argument_size(integer_args.len());
                    encode_stack_adjust(bytes, stack_size, true)?;
                    for (index, argument) in integer_args.iter().enumerate().skip(8) {
                        let offset = (index as u32 - 8) * 8;
                        if offset > 32760 {
                            return Err(
                                "ARM64 outgoing argument offset exceeds the store range".into()
                            );
                        }
                        push_word(bytes, 0xf900_03e0 | ((offset / 8) << 10) | argument.0);
                    }
                    encode_float_argument_moves(args, float_args, bytes)?;
                    push_word(bytes, 0xaa00_03e0 | (callee.0 << 16) | 17);
                    encode_parallel_argument_moves(
                        &integer_args[..integer_args.len().min(8)],
                        bytes,
                    );
                    push_word(bytes, 0xd63f_0000 | (17 << 5));
                    encode_stack_adjust(bytes, stack_size, false)?;
                    if let (Some(pointer), Some(length)) = (result, result_slice_length) {
                        encode_register_moves(
                            bytes,
                            &parallel_moves(&[(pointer.0, 0), (length.0, 1)]),
                        );
                    } else if let Some(result) = result {
                        if let Some(width) = result_float {
                            encode_fp_to_gpr(bytes, result.0, 0, *width)?;
                        } else if result.0 != 0 {
                            push_word(bytes, 0xaa00_03e0 | result.0);
                        }
                    }
                    continue;
                }
                LirInstruction::Phi { .. } => {
                    continue;
                }
            };
            push_word(bytes, word);
        }
        match block.terminator {
            MirTerminator::Jump(target) => {
                emit_phi_copies(function, block.id, target, bytes);
                let at = bytes.len();
                push_word(bytes, 0x1400_0000);
                branches.push((at, block_offsets[&target], false));
            }
            MirTerminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let conditional_at = bytes.len();
                push_word(bytes, 0x3400_0000 | condition.0);
                emit_phi_copies(function, block.id, then_block, bytes);
                let at = bytes.len();
                push_word(bytes, 0x1400_0000);
                branches.push((at, block_offsets[&then_block], false));
                let else_stub = bytes.len();
                branches.push((conditional_at, else_stub, true));
                emit_phi_copies(function, block.id, else_block, bytes);
                let at = bytes.len();
                push_word(bytes, 0x1400_0000);
                branches.push((at, block_offsets[&else_block], false));
            }
            MirTerminator::Return {
                value: result,
                slice_length,
            } => {
                if let (Some(pointer), Some(length)) = (result, slice_length) {
                    encode_parallel_argument_moves(&[pointer, length], bytes);
                } else if let Some(value) = result {
                    if let Some(width) = float_width(&function.result) {
                        encode_gpr_to_fp(bytes, 0, value.0, width)?;
                    } else if value.0 != 0 {
                        push_word(bytes, 0xaa00_03e0 | (value.0 << 16));
                    }
                }
                if function.name == "main" {
                    if result.is_none() {
                        encode_immediate(0, 0, bytes)?;
                    }
                    encode_immediate(16, 1, bytes)?;
                    push_word(bytes, 0xd400_1001);
                } else {
                    for (index, register) in saved_registers.iter().enumerate() {
                        encode_stack_load(bytes, *register, saved_register_offset(function, index));
                    }
                    encode_stack_adjust(bytes, frame_size, false)?;
                    if save_frame {
                        push_word(bytes, 0xa8c1_7bfd);
                    }
                    push_word(bytes, 0xd65f_03c0);
                }
            }
            MirTerminator::Unreachable => {
                return Err("unreachable terminator reached ARM64 backend".into());
            }
        }
    }
    for (at, target, conditional) in branches {
        let delta = (target as i64 - at as i64) / 4;
        if conditional {
            if !(-(1 << 18)..(1 << 18)).contains(&delta) {
                return Err("conditional branch exceeds ARM64 range".into());
            }
            let register = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) & 31;
            bytes[at..at + 4].copy_from_slice(
                &(0x3400_0000 | (((delta as u32) & 0x7ffff) << 5) | register).to_le_bytes(),
            );
        } else {
            if !(-(1 << 25)..(1 << 25)).contains(&delta) {
                return Err("branch exceeds ARM64 range".into());
            }
            bytes[at..at + 4]
                .copy_from_slice(&(0x1400_0000 | ((delta as u32) & 0x03ff_ffff)).to_le_bytes());
        }
    }
    Ok(())
}

fn block_encoded_size(
    function: &LirFunction,
    block: &crate::lir::LirBlock,
) -> Result<usize, String> {
    let mut size = 0;
    for instruction in &block.instructions {
        size += match instruction {
            LirInstruction::LoadImmediate { value, .. } => {
                let n = (0..4)
                    .filter(|shift| ((*value as u64 >> (shift * 16)) & 0xffff) != 0)
                    .count();
                n.max(1)
            }
            LirInstruction::LoadAddress { .. } => 2,
            LirInstruction::Add { .. }
            | LirInstruction::Sub { .. }
            | LirInstruction::Mul { .. }
            | LirInstruction::Div { .. }
            | LirInstruction::Neg { .. } => 1,
            LirInstruction::FloatBinary { .. } => 4,
            LirInstruction::FloatNeg { .. } => 3,
            LirInstruction::LoadSlot { .. } | LirInstruction::StoreSlot { .. } => 1,
            LirInstruction::SlotAddress { slot, .. } => slot_address_words(*slot),
            LirInstruction::IndexAddress { scale, .. } => index_address_words(*scale)?,
            LirInstruction::LoadIndirect { .. }
            | LirInstruction::LoadByteIndirect { .. }
            | LirInstruction::LoadSizedIndirect { .. }
            | LirInstruction::StoreIndirect { .. }
            | LirInstruction::StoreByteIndirect { .. }
            | LirInstruction::StoreSizedIndirect { .. } => 1,
            LirInstruction::Not { .. } => 2,
            LirInstruction::Rem { .. } | LirInstruction::Compare { .. } => 2,
            LirInstruction::FloatCompare { .. } => 4,
            LirInstruction::Phi { .. } => 0,
            LirInstruction::Call {
                args,
                result,
                result_slice_length,
                float_args,
                result_float,
                ..
            } => encoded_call_words(
                args,
                *result,
                *result_slice_length,
                float_args,
                *result_float,
                false,
            )?,
            LirInstruction::CallIndirect {
                args,
                result,
                result_slice_length,
                float_args,
                result_float,
                ..
            } => encoded_call_words(
                args,
                *result,
                *result_slice_length,
                float_args,
                *result_float,
                true,
            )?,
        } * 4;
    }
    size += match block.terminator {
        MirTerminator::Return {
            value,
            slice_length,
        } => {
            let has_calls = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|instruction| {
                    matches!(
                        instruction,
                        LirInstruction::Call { .. } | LirInstruction::CallIndirect { .. }
                    )
                });
            let frame_size = stack_frame_size(function);
            let saved_registers = callee_saved_registers(function);
            let save_frame = saves_frame_pointer(has_calls);
            function_return_moves(value, slice_length, &function.result) * 4
                + if function.name == "main" {
                    8 + usize::from(value.is_none()) * 4
                } else {
                    (1 + usize::from(save_frame)
                        + saved_registers
                            .iter()
                            .enumerate()
                            .map(|(index, _)| {
                                stack_access_words(saved_register_offset(function, index))
                            })
                            .sum::<usize>()
                        + stack_adjust_words(frame_size))
                        * 4
                }
        }
        MirTerminator::Jump(target) => 4 + phi_move_count(function, block.id, target) * 4,
        MirTerminator::Branch {
            then_block,
            else_block,
            ..
        } => {
            12 + (phi_move_count(function, block.id, then_block)
                + phi_move_count(function, block.id, else_block))
                * 4
        }
        MirTerminator::Unreachable => {
            return Err("unreachable terminator reached ARM64 backend".into());
        }
    };
    Ok(size)
}

fn instruction_registers(instruction: &LirInstruction) -> Vec<ValueId> {
    match instruction {
        LirInstruction::LoadImmediate { register, .. } => vec![*register],
        LirInstruction::LoadAddress { register, .. } => vec![*register],
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
        | LirInstruction::FloatBinary {
            destination,
            lhs,
            rhs,
            ..
        } => vec![*destination, *lhs, *rhs],
        LirInstruction::Rem {
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
        | LirInstruction::FloatCompare {
            destination,
            lhs,
            rhs,
            ..
        } => {
            vec![*destination, *lhs, *rhs]
        }
        LirInstruction::Neg { destination, value } => vec![*destination, *value],
        LirInstruction::FloatNeg {
            destination, value, ..
        } => vec![*destination, *value],
        LirInstruction::Not { destination, value } => vec![*destination, *value],
        LirInstruction::LoadSlot { destination, .. } => vec![*destination],
        LirInstruction::StoreSlot { value, .. } => vec![*value],
        LirInstruction::SlotAddress { destination, .. } => vec![*destination],
        LirInstruction::IndexAddress {
            destination,
            base,
            index,
            ..
        } => vec![*destination, *base, *index],
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
        } => vec![*destination, *address],
        LirInstruction::StoreIndirect { address, value }
        | LirInstruction::StoreByteIndirect { address, value }
        | LirInstruction::StoreSizedIndirect { address, value, .. } => {
            vec![*address, *value]
        }
        LirInstruction::Call {
            result,
            result_slice_length,
            args,
            ..
        } => result
            .iter()
            .chain(result_slice_length.iter())
            .copied()
            .chain(args.iter().copied())
            .collect(),
        LirInstruction::CallIndirect {
            result,
            result_slice_length,
            callee,
            args,
            ..
        } => std::iter::once(*callee)
            .chain(result.iter().copied())
            .chain(result_slice_length.iter().copied())
            .chain(args.iter().copied())
            .collect(),
        LirInstruction::Phi {
            destination,
            incoming,
        } => std::iter::once(*destination)
            .chain(incoming.iter().map(|(_, value)| *value))
            .collect(),
    }
}

fn encode_stack_adjust(bytes: &mut Vec<u8>, size: u32, subtract: bool) -> Result<(), String> {
    let mut remaining = size;
    while remaining != 0 {
        let immediate = remaining.min(4095);
        let opcode = if subtract { 0xd100_03ff } else { 0x9100_03ff };
        push_word(bytes, opcode | (immediate << 10));
        remaining -= immediate;
    }
    Ok(())
}

fn encode_stack_load(bytes: &mut Vec<u8>, register: u32, offset: u32) {
    if offset <= 32760 {
        push_word(bytes, 0xf940_03e0 | ((offset / 8) << 10) | register);
    } else {
        encode_slot_address(bytes, 16, offset);
        push_word(bytes, 0xf940_0000 | (16 << 5) | register);
    }
}

fn encode_stack_store(bytes: &mut Vec<u8>, register: u32, offset: u32) {
    if offset <= 32760 {
        push_word(bytes, 0xf900_03e0 | ((offset / 8) << 10) | register);
    } else {
        encode_slot_address(bytes, 16, offset);
        push_word(bytes, 0xf900_0000 | (16 << 5) | register);
    }
}

fn encode_slot_address(bytes: &mut Vec<u8>, destination: u32, offset: u32) {
    let mut remaining = offset;
    let first = remaining.min(4095);
    push_word(bytes, 0x9100_03e0 | (first << 10) | destination);
    remaining -= first;
    while remaining != 0 {
        let immediate = remaining.min(4095);
        push_word(
            bytes,
            0x9100_0000 | (immediate << 10) | (destination << 5) | destination,
        );
        remaining -= immediate;
    }
}

fn encode_immediate(register: u32, value: u64, bytes: &mut Vec<u8>) -> Result<(), String> {
    if register > 30 {
        return Err(format!(
            "virtual register x{register} exceeds ARM64 general registers"
        ));
    }
    let chunks: Vec<_> = (0..4)
        .map(|shift| ((value >> (shift * 16)) & 0xffff) as u32)
        .collect();
    let Some(first) = chunks.iter().position(|chunk| *chunk != 0) else {
        push_word(bytes, 0xd280_0000 | register);
        return Ok(());
    };
    push_word(
        bytes,
        0xd280_0000 | (chunks[first] << 5) | ((first as u32) << 21) | register,
    );
    for (shift, chunk) in chunks.iter().enumerate().skip(first + 1) {
        if *chunk != 0 {
            push_word(
                bytes,
                0xf280_0000 | (*chunk << 5) | ((shift as u32) << 21) | register,
            );
        }
    }
    Ok(())
}

fn push_word(bytes: &mut Vec<u8>, word: u32) {
    bytes.extend_from_slice(&word.to_le_bytes());
}

fn float_width(data_type: &HirType) -> Option<u16> {
    match data_type {
        HirType::Float { bits } if *bits != 8 => Some(*bits),
        _ => None,
    }
}

fn encode_gpr_to_fp(
    bytes: &mut Vec<u8>,
    destination: u32,
    source: u32,
    width: u16,
) -> Result<(), String> {
    validate_float_width(width)?;
    let instruction = match width {
        16 => 0x1ee7_0000 | (source << 5) | destination,
        32 => 0x1e27_0000 | (source << 5) | destination,
        64 => 0x9e67_0000 | (source << 5) | destination,
        _ => return Err(format!("unsupported ARM64 floating-point width {width}")),
    };
    push_word(bytes, instruction);
    Ok(())
}

fn encode_fp_to_gpr(
    bytes: &mut Vec<u8>,
    destination: u32,
    source: u32,
    width: u16,
) -> Result<(), String> {
    validate_float_width(width)?;
    let instruction = match width {
        16 => 0x1ee6_0000 | (source << 5) | destination,
        32 => 0x1e26_0000 | (source << 5) | destination,
        64 => 0x9e66_0000 | (source << 5) | destination,
        _ => return Err(format!("unsupported ARM64 floating-point width {width}")),
    };
    push_word(bytes, instruction);
    Ok(())
}

fn encode_float_argument_moves(
    args: &[ValueId],
    float_args: &[(usize, u16)],
    bytes: &mut Vec<u8>,
) -> Result<(), String> {
    for (index, (argument, width)) in float_args.iter().enumerate() {
        encode_gpr_to_fp(bytes, index as u32, args[*argument].0, *width)?;
    }
    Ok(())
}

fn encode_float_binary(
    destination: ValueId,
    lhs: ValueId,
    rhs: ValueId,
    kind: FloatBinaryKind,
    width: u16,
    bytes: &mut Vec<u8>,
) -> Result<(), String> {
    encode_gpr_to_fp(bytes, 16, lhs.0, width)?;
    encode_gpr_to_fp(bytes, 17, rhs.0, width)?;
    let base = match (kind, width) {
        (FloatBinaryKind::Add, 16) => 0x1ee0_2800,
        (FloatBinaryKind::Subtract, 16) => 0x1ee0_3800,
        (FloatBinaryKind::Multiply, 16) => 0x1ee0_0800,
        (FloatBinaryKind::Divide, 16) => 0x1ee0_1800,
        (FloatBinaryKind::Add, 32) => 0x1e20_2800,
        (FloatBinaryKind::Subtract, 32) => 0x1e20_3800,
        (FloatBinaryKind::Multiply, 32) => 0x1e20_0800,
        (FloatBinaryKind::Divide, 32) => 0x1e20_1800,
        (FloatBinaryKind::Add, 64) => 0x1e60_2800,
        (FloatBinaryKind::Subtract, 64) => 0x1e60_3800,
        (FloatBinaryKind::Multiply, 64) => 0x1e60_0800,
        (FloatBinaryKind::Divide, 64) => 0x1e60_1800,
        _ => {
            return Err(format!(
                "ARM64 backend does not support f{width} arithmetic yet"
            ));
        }
    };
    push_word(bytes, base | (17 << 16) | (16 << 5) | 16);
    encode_fp_to_gpr(bytes, destination.0, 16, width)
}

fn encode_float_neg(
    destination: ValueId,
    value: ValueId,
    width: u16,
    bytes: &mut Vec<u8>,
) -> Result<(), String> {
    encode_gpr_to_fp(bytes, 16, value.0, width)?;
    let base = match width {
        16 => 0x1ee1_4000,
        32 => 0x1e21_4000,
        64 => 0x1e61_4000,
        _ => {
            return Err(format!(
                "ARM64 backend does not support f{width} negation yet"
            ));
        }
    };
    push_word(bytes, base | (16 << 5) | 16);
    encode_fp_to_gpr(bytes, destination.0, 16, width)
}

fn encode_float_compare(
    destination: ValueId,
    lhs: ValueId,
    rhs: ValueId,
    kind: CompareKind,
    width: u16,
    bytes: &mut Vec<u8>,
) -> Result<(), String> {
    encode_gpr_to_fp(bytes, 16, lhs.0, width)?;
    encode_gpr_to_fp(bytes, 17, rhs.0, width)?;
    let base = match width {
        16 => 0x1ee0_2000,
        32 => 0x1e20_2000,
        64 => 0x1e60_2000,
        _ => {
            return Err(format!(
                "ARM64 backend does not support f{width} comparisons yet"
            ));
        }
    };
    push_word(bytes, base | (17 << 16) | (16 << 5));
    let condition = match kind {
        CompareKind::Equal => 0,
        CompareKind::NotEqual => 1,
        CompareKind::GreaterEqual => 10,
        CompareKind::Less => 3,
        CompareKind::Greater => 12,
        CompareKind::LessEqual => 9,
    };
    push_word(
        bytes,
        0x9a9f_07e0 | (((condition ^ 1) as u32) << 12) | destination.0,
    );
    Ok(())
}

fn encode_parallel_argument_moves(args: &[ValueId], bytes: &mut Vec<u8>) {
    for (destination, source) in argument_moves(args) {
        push_word(bytes, 0xaa00_03e0 | (source << 16) | destination);
    }
}

fn encode_register_moves(bytes: &mut Vec<u8>, moves: &[(u32, u32)]) {
    for (destination, source) in moves {
        push_word(bytes, 0xaa00_03e0 | (source << 16) | destination);
    }
}

fn argument_moves(args: &[ValueId]) -> Vec<(u32, u32)> {
    let moves = args
        .iter()
        .enumerate()
        .filter_map(|(destination, source)| {
            (source.0 != destination as u32).then_some((destination as u32, source.0))
        })
        .collect::<Vec<_>>();
    parallel_moves(&moves)
}

fn parallel_moves(moves: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut pending = moves.to_vec();
    let mut output = Vec::new();
    while !pending.is_empty() {
        let sources = pending
            .iter()
            .map(|(_, source)| *source)
            .collect::<std::collections::HashSet<_>>();
        if let Some(index) = pending
            .iter()
            .position(|(destination, _)| !sources.contains(destination))
        {
            output.push(pending.remove(index));
        } else {
            let source = pending[0].1;
            output.push((16, source));
            for (_, pending_source) in &mut pending {
                if *pending_source == source {
                    *pending_source = 16;
                }
            }
        }
    }
    output
}

fn write_macho_object(
    text: &[u8],
    data: &[u8],
    symbols: &[ObjectSymbol],
    relocations: &[Relocation],
) -> Vec<u8> {
    const HEADER_SIZE: usize = 32;
    const SEGMENT_COMMAND_SIZE: usize = 232;
    const SYMTAB_COMMAND_SIZE: usize = 24;
    let commands_size = SEGMENT_COMMAND_SIZE + SYMTAB_COMMAND_SIZE;
    let text_offset = HEADER_SIZE + commands_size;
    let data_offset = (text_offset + text.len() + 7) & !7;
    let relocation_offset = data_offset + data.len();
    let symbol_offset = relocation_offset + relocations.len() * 8;
    let string_offset = symbol_offset + symbols.len() * 16;
    let mut strings = vec![0u8];
    let mut string_indexes = Vec::new();
    for symbol in symbols {
        string_indexes.push(strings.len() as u32);
        strings.extend_from_slice(symbol.name.as_bytes());
        strings.push(0);
    }
    let symbol_indexes = symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| (symbol.name.as_str(), index as u32))
        .collect::<std::collections::HashMap<_, _>>();

    let mut output = Vec::new();
    for value in [
        0xfeed_facfu32,
        0x0100_000c,
        0,
        1,
        2,
        commands_size as u32,
        0,
        0,
    ] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(&0x19u32.to_le_bytes());
    output.extend_from_slice(&(SEGMENT_COMMAND_SIZE as u32).to_le_bytes());
    write_name(&mut output, "__TEXT");
    let vm_size = data_offset + data.len() - text_offset;
    for value in [0u64, vm_size as u64, 0, vm_size as u64] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    for value in [7u32, 5, 2, 0] {
        output.extend_from_slice(&value.to_le_bytes());
    }

    write_name(&mut output, "__text");
    write_name(&mut output, "__TEXT");
    output.extend_from_slice(&0u64.to_le_bytes());
    output.extend_from_slice(&(text.len() as u64).to_le_bytes());
    for value in [
        text_offset as u32,
        2,
        relocation_offset as u32,
        relocations.len() as u32,
        0x8000_0400u32,
        0,
        0,
        0,
    ] {
        output.extend_from_slice(&value.to_le_bytes());
    }

    write_name(&mut output, "__data");
    write_name(&mut output, "__TEXT");
    output.extend_from_slice(&(text.len() as u64).to_le_bytes());
    output.extend_from_slice(&(data.len() as u64).to_le_bytes());
    for value in [data_offset as u32, 0, 0, 0, 0u32, 0, 0, 0] {
        output.extend_from_slice(&value.to_le_bytes());
    }

    output.extend_from_slice(&2u32.to_le_bytes());
    output.extend_from_slice(&(SYMTAB_COMMAND_SIZE as u32).to_le_bytes());
    for value in [
        symbol_offset as u32,
        symbols.len() as u32,
        string_offset as u32,
        strings.len() as u32,
    ] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(text);
    output.resize(data_offset, 0);
    output.extend_from_slice(data);
    output.resize(relocation_offset, 0);
    for relocation in relocations {
        output.extend_from_slice(&(relocation.offset as i32).to_le_bytes());
        let symbol_index = symbol_indexes[relocation.symbol.as_str()];
        let (kind, pcrel) = match relocation.kind {
            RelocationKind::Branch26 => (2u32, 1u32),
            RelocationKind::Page21 => (3, 1),
            RelocationKind::PageOff12 => (4, 0),
            RelocationKind::GotPage21 => (5, 1),
            RelocationKind::GotPageOff12 => (6, 0),
        };
        let info = symbol_index | (pcrel << 24) | (2 << 25) | (1 << 27) | (kind << 28);
        output.extend_from_slice(&info.to_le_bytes());
    }
    output.resize(symbol_offset, 0);
    for (symbol, string_index) in symbols.iter().zip(string_indexes) {
        output.extend_from_slice(&string_index.to_le_bytes());
        output.push(if symbol.section == 0 { 0x01 } else { 0x0f });
        output.push(symbol.section);
        output.extend_from_slice(&0u16.to_le_bytes());
        let value = if symbol.section == 2 {
            text.len() as u64 + symbol.value
        } else {
            symbol.value
        };
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(&strings);
    output
}

fn write_name(output: &mut Vec<u8>, name: &str) {
    let mut bytes = [0u8; 16];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    output.extend_from_slice(&bytes);
}

fn emit_aarch64(module: &LirModule) -> Result<String, String> {
    let mut assembly = String::from(".text\n");
    for function in &module.functions {
        let function = allocate_registers(function)?;
        emit_function(&function, &mut assembly)?;
    }
    if !module.strings.is_empty() {
        assembly.push_str(".section __TEXT,__const\n.p2align 3\n");
        for (symbol, bytes) in &module.strings {
            assembly.push_str(&format!("{symbol}:\n"));
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

fn emit_function(function: &LirFunction, assembly: &mut String) -> Result<(), String> {
    if function.blocks.is_empty() {
        return Err("function has no basic blocks".into());
    }
    if function.params.len() > 16 {
        return Err("ARM64 backend currently supports at most 16 integer parameters".into());
    }
    let label = format!("_{}", function.name);
    assembly.push_str(&format!(".globl {label}\n{label}:\n"));
    let frame_size = stack_frame_size(function);
    let has_calls = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(
                instruction,
                LirInstruction::Call { .. } | LirInstruction::CallIndirect { .. }
            )
        });
    let saved_registers = callee_saved_registers(function);
    let save_frame = saves_frame_pointer(has_calls);
    if save_frame {
        assembly.push_str("    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n");
    }
    emit_stack_adjust(assembly, frame_size, true);
    for (index, register) in saved_registers.iter().enumerate() {
        emit_stack_store(assembly, *register, saved_register_offset(function, index));
    }
    for (index, parameter) in function.params.iter().enumerate().skip(8) {
        let offset = frame_size + u32::from(save_frame) * 16 + (index as u32 - 8) * 8;
        emit_stack_load(assembly, parameter.0, offset);
    }
    for (index, (parameter, width)) in function.float_params.iter().enumerate() {
        emit_fp_to_gpr(assembly, parameter.0, index as u32, *width)?;
    }

    for block in &function.blocks {
        assembly.push_str(&format!(".L{}_{}:\n", function.name, block.id.0));
        for instruction in &block.instructions {
            match instruction {
                LirInstruction::LoadImmediate { register, value } => {
                    emit_immediate(assembly, register.0, *value as u64);
                }
                LirInstruction::LoadAddress {
                    register,
                    symbol,
                    external,
                } => {
                    if *external {
                        assembly.push_str(&format!(
                            "    adrp x{}, {symbol}@GOTPAGE\n    ldr x{}, [x{}, {symbol}@GOTPAGEOFF]\n",
                            register.0, register.0, register.0
                        ));
                    } else {
                        assembly.push_str(&format!(
                            "    adrp x{}, {symbol}@PAGE\n    add x{}, x{}, {symbol}@PAGEOFF\n",
                            register.0, register.0, register.0
                        ));
                    }
                }
                LirInstruction::Add {
                    destination,
                    lhs,
                    rhs,
                } => assembly.push_str(&format!(
                    "    add x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                )),
                LirInstruction::FloatBinary {
                    destination,
                    lhs,
                    rhs,
                    kind,
                    width,
                } => emit_float_binary_assembly(assembly, *destination, *lhs, *rhs, *kind, *width)?,
                LirInstruction::Sub {
                    destination,
                    lhs,
                    rhs,
                } => assembly.push_str(&format!(
                    "    sub x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                )),
                LirInstruction::Mul {
                    destination,
                    lhs,
                    rhs,
                } => assembly.push_str(&format!(
                    "    mul x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                )),
                LirInstruction::Div {
                    destination,
                    lhs,
                    rhs,
                } => assembly.push_str(&format!(
                    "    sdiv x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                )),
                LirInstruction::Rem {
                    destination,
                    lhs,
                    rhs,
                } => assembly.push_str(&format!(
                    "    sdiv x16, x{}, x{}\n    msub x{}, x16, x{}, x{}\n",
                    lhs.0, rhs.0, destination.0, rhs.0, lhs.0
                )),
                LirInstruction::Neg { destination, value } => {
                    assembly.push_str(&format!("    neg x{}, x{}\n", destination.0, value.0))
                }
                LirInstruction::FloatNeg {
                    destination,
                    value,
                    width,
                } => emit_float_neg_assembly(assembly, *destination, *value, *width)?,
                LirInstruction::Not { destination, value } => assembly.push_str(&format!(
                    "    cmp x{}, #0\n    cset x{}, eq\n",
                    value.0, destination.0
                )),
                LirInstruction::Compare {
                    destination,
                    lhs,
                    rhs,
                    kind,
                } => {
                    let condition = match kind {
                        CompareKind::Equal => "eq",
                        CompareKind::NotEqual => "ne",
                        CompareKind::GreaterEqual => "ge",
                        CompareKind::Less => "lt",
                        CompareKind::Greater => "gt",
                        CompareKind::LessEqual => "le",
                    };
                    assembly.push_str(&format!(
                        "    cmp x{}, x{}\n    cset x{}, {condition}\n",
                        lhs.0, rhs.0, destination.0
                    ));
                }
                LirInstruction::FloatCompare {
                    destination,
                    lhs,
                    rhs,
                    kind,
                    width,
                } => {
                    emit_float_compare_assembly(assembly, *destination, *lhs, *rhs, *kind, *width)?
                }
                LirInstruction::Call {
                    result,
                    result_slice_length,
                    name,
                    args,
                    float_args,
                    result_float,
                } => {
                    let integer_args = integer_arguments(args, float_args)?;
                    if integer_args.len() > 16 {
                        return Err(
                            "ARM64 assembly emitter supports at most 16 integer arguments".into(),
                        );
                    }
                    let stack_size = stack_argument_size(integer_args.len());
                    emit_stack_adjust(assembly, stack_size, true);
                    for (index, argument) in integer_args.iter().enumerate().skip(8) {
                        emit_stack_store(assembly, argument.0, (index as u32 - 8) * 8);
                    }
                    emit_float_argument_moves_assembly(assembly, args, float_args)?;
                    emit_register_moves(
                        assembly,
                        &argument_moves(&integer_args[..integer_args.len().min(8)]),
                    );
                    assembly.push_str(&format!("    bl _{name}\n"));
                    emit_stack_adjust(assembly, stack_size, false);
                    if let (Some(pointer), Some(length)) = (result, result_slice_length) {
                        emit_register_moves(
                            assembly,
                            &parallel_moves(&[(pointer.0, 0), (length.0, 1)]),
                        );
                    } else if let Some(result) = result {
                        if let Some(width) = result_float {
                            emit_fp_to_gpr(assembly, result.0, 0, *width)?;
                        } else if result.0 != 0 {
                            assembly.push_str(&format!("    mov x{}, x0\n", result.0));
                        }
                    }
                }
                LirInstruction::CallIndirect {
                    result,
                    result_slice_length,
                    callee,
                    args,
                    float_args,
                    result_float,
                } => {
                    let integer_args = integer_arguments(args, float_args)?;
                    if integer_args.len() > 16 {
                        return Err(
                            "ARM64 assembly emitter supports at most 16 integer arguments".into(),
                        );
                    }
                    let stack_size = stack_argument_size(integer_args.len());
                    emit_stack_adjust(assembly, stack_size, true);
                    for (index, argument) in integer_args.iter().enumerate().skip(8) {
                        emit_stack_store(assembly, argument.0, (index as u32 - 8) * 8);
                    }
                    emit_float_argument_moves_assembly(assembly, args, float_args)?;
                    assembly.push_str(&format!("    mov x17, x{}\n", callee.0));
                    emit_register_moves(
                        assembly,
                        &argument_moves(&integer_args[..integer_args.len().min(8)]),
                    );
                    assembly.push_str("    blr x17\n");
                    emit_stack_adjust(assembly, stack_size, false);
                    if let (Some(pointer), Some(length)) = (result, result_slice_length) {
                        emit_register_moves(
                            assembly,
                            &parallel_moves(&[(pointer.0, 0), (length.0, 1)]),
                        );
                    } else if let Some(result) = result {
                        if let Some(width) = result_float {
                            emit_fp_to_gpr(assembly, result.0, 0, *width)?;
                        } else if result.0 != 0 {
                            assembly.push_str(&format!("    mov x{}, x0\n", result.0));
                        }
                    }
                }
                LirInstruction::Phi { .. } => {}
                LirInstruction::LoadSlot { destination, slot } => {
                    emit_stack_load(assembly, destination.0, slot.0 * 8);
                }
                LirInstruction::StoreSlot { slot, value } => {
                    emit_stack_store(assembly, value.0, slot.0 * 8);
                }
                LirInstruction::SlotAddress { destination, slot } => {
                    emit_sp_address(assembly, destination.0, slot.0 * 8);
                }
                LirInstruction::IndexAddress {
                    destination,
                    base,
                    index,
                    scale,
                } => emit_index_address_assembly(assembly, *destination, *base, *index, *scale)?,
                LirInstruction::LoadIndirect {
                    destination,
                    address,
                } => assembly.push_str(&format!("    ldr x{}, [x{}]\n", destination.0, address.0)),
                LirInstruction::LoadByteIndirect {
                    destination,
                    address,
                } => assembly.push_str(&format!("    ldrb w{}, [x{}]\n", destination.0, address.0)),
                LirInstruction::LoadSizedIndirect {
                    destination,
                    address,
                    bytes,
                    signed,
                } => {
                    let (operation, register) = match (bytes, signed) {
                        (1, false) => ("ldrb", "w"),
                        (1, true) => ("ldrsb", "x"),
                        (2, false) => ("ldrh", "w"),
                        (2, true) => ("ldrsh", "x"),
                        (4, false) => ("ldr", "w"),
                        (4, true) => ("ldrsw", "x"),
                        _ => return Err(format!("unsupported ARM64 load width {bytes}")),
                    };
                    assembly.push_str(&format!(
                        "    {operation} {register}{}, [x{}]\n",
                        destination.0, address.0
                    ));
                }
                LirInstruction::StoreIndirect { address, value } => {
                    assembly.push_str(&format!("    str x{}, [x{}]\n", value.0, address.0))
                }
                LirInstruction::StoreByteIndirect { address, value } => {
                    assembly.push_str(&format!("    strb w{}, [x{}]\n", value.0, address.0))
                }
                LirInstruction::StoreSizedIndirect {
                    address,
                    value,
                    bytes,
                } => {
                    let (operation, register) = match bytes {
                        2 => ("strh", "w"),
                        4 => ("str", "w"),
                        _ => return Err(format!("unsupported ARM64 store width {bytes}")),
                    };
                    assembly.push_str(&format!(
                        "    {operation} {register}{}, [x{}]\n",
                        value.0, address.0
                    ));
                }
            }
        }
        match block.terminator {
            MirTerminator::Jump(target) => {
                emit_assembly_phi_copies(assembly, function, block.id, target);
                assembly.push_str(&format!("    b .L{}_{}\n", function.name, target.0));
            }
            MirTerminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                assembly.push_str(&format!(
                    "    cbz x{}, .L{}_{}_else_from_{}\n",
                    condition.0, function.name, block.id.0, else_block.0
                ));
                emit_assembly_phi_copies(assembly, function, block.id, then_block);
                assembly.push_str(&format!("    b .L{}_{}\n", function.name, then_block.0));
                assembly.push_str(&format!(
                    ".L{}_{}_else_from_{}:\n",
                    function.name, block.id.0, else_block.0
                ));
                emit_assembly_phi_copies(assembly, function, block.id, else_block);
                assembly.push_str(&format!("    b .L{}_{}\n", function.name, else_block.0));
            }
            MirTerminator::Return {
                value,
                slice_length,
            } => {
                if let (Some(pointer), Some(length)) = (value, slice_length) {
                    emit_register_moves(assembly, &argument_moves(&[pointer, length]));
                } else if let Some(value) = value {
                    if let Some(width) = float_width(&function.result) {
                        emit_gpr_to_fp(assembly, 0, value.0, width)?;
                    } else if value.0 != 0 {
                        assembly.push_str(&format!("    mov x0, x{}\n", value.0));
                    }
                }
                for (index, register) in saved_registers.iter().enumerate() {
                    emit_stack_load(assembly, *register, saved_register_offset(function, index));
                }
                emit_stack_adjust(assembly, frame_size, false);
                if save_frame {
                    assembly.push_str("    ldp x29, x30, [sp], #16\n");
                }
                assembly.push_str("    ret\n");
            }
            MirTerminator::Unreachable => {
                return Err("unreachable terminator reached ARM64 assembly emitter".into());
            }
        }
    }
    Ok(())
}

fn emit_immediate(assembly: &mut String, register: u32, value: u64) {
    let chunks = (0..4)
        .map(|shift| ((value >> (shift * 16)) & 0xffff) as u16)
        .collect::<Vec<_>>();
    let first = chunks.iter().position(|chunk| *chunk != 0).unwrap_or(0);
    assembly.push_str(&format!(
        "    movz x{register}, #{}, lsl #{}\n",
        chunks[first],
        first * 16
    ));
    for (shift, chunk) in chunks.iter().enumerate().skip(first + 1) {
        if *chunk != 0 {
            assembly.push_str(&format!(
                "    movk x{register}, #{chunk}, lsl #{}\n",
                shift * 16
            ));
        }
    }
}

fn emit_index_address_assembly(
    assembly: &mut String,
    destination: ValueId,
    base: ValueId,
    index: ValueId,
    scale: u64,
) -> Result<(), String> {
    if scale == 0 {
        return Err("ARM64 index scale cannot be zero".into());
    }
    if scale.is_power_of_two() {
        assembly.push_str(&format!(
            "    add x{}, x{}, x{}, lsl #{}\n",
            destination.0,
            base.0,
            index.0,
            scale.trailing_zeros()
        ));
    } else {
        emit_immediate(assembly, 16, scale);
        assembly.push_str(&format!(
            "    mul x17, x{}, x16\n    add x{}, x{}, x17\n",
            index.0, destination.0, base.0
        ));
    }
    Ok(())
}

fn emit_stack_adjust(assembly: &mut String, size: u32, subtract: bool) {
    let instruction = if subtract { "sub" } else { "add" };
    let mut remaining = size;
    while remaining != 0 {
        let immediate = remaining.min(4095);
        assembly.push_str(&format!("    {instruction} sp, sp, #{immediate}\n"));
        remaining -= immediate;
    }
}

fn emit_sp_address(assembly: &mut String, destination: u32, offset: u32) {
    if offset == 0 {
        assembly.push_str(&format!("    mov x{destination}, sp\n"));
        return;
    }
    let first = offset.min(4095);
    assembly.push_str(&format!("    add x{destination}, sp, #{first}\n"));
    let mut remaining = offset - first;
    while remaining != 0 {
        let immediate = remaining.min(4095);
        assembly.push_str(&format!(
            "    add x{destination}, x{destination}, #{immediate}\n"
        ));
        remaining -= immediate;
    }
}

fn emit_stack_load(assembly: &mut String, destination: u32, offset: u32) {
    if offset <= 32760 {
        assembly.push_str(&format!("    ldr x{destination}, [sp, #{offset}]\n"));
    } else {
        emit_sp_address(assembly, 16, offset);
        assembly.push_str(&format!("    ldr x{destination}, [x16]\n"));
    }
}

fn emit_stack_store(assembly: &mut String, value: u32, offset: u32) {
    if offset <= 32760 {
        assembly.push_str(&format!("    str x{value}, [sp, #{offset}]\n"));
    } else {
        emit_sp_address(assembly, 16, offset);
        assembly.push_str(&format!("    str x{value}, [x16]\n"));
    }
}

fn emit_register_moves(assembly: &mut String, moves: &[(u32, u32)]) {
    for (destination, source) in moves {
        assembly.push_str(&format!("    mov x{destination}, x{source}\n"));
    }
}

fn emit_gpr_to_fp(
    assembly: &mut String,
    destination: u32,
    source: u32,
    width: u16,
) -> Result<(), String> {
    validate_float_width(width)?;
    match width {
        16 => assembly.push_str(&format!("    fmov h{destination}, w{source}\n")),
        32 => assembly.push_str(&format!("    fmov s{destination}, w{source}\n")),
        64 => assembly.push_str(&format!("    fmov d{destination}, x{source}\n")),
        _ => return Err(format!("unsupported ARM64 floating-point width {width}")),
    }
    Ok(())
}

fn emit_fp_to_gpr(
    assembly: &mut String,
    destination: u32,
    source: u32,
    width: u16,
) -> Result<(), String> {
    validate_float_width(width)?;
    match width {
        16 => assembly.push_str(&format!("    fmov w{destination}, h{source}\n")),
        32 => assembly.push_str(&format!("    fmov w{destination}, s{source}\n")),
        64 => assembly.push_str(&format!("    fmov x{destination}, d{source}\n")),
        _ => return Err(format!("unsupported ARM64 floating-point width {width}")),
    }
    Ok(())
}

fn emit_float_argument_moves_assembly(
    assembly: &mut String,
    args: &[ValueId],
    float_args: &[(usize, u16)],
) -> Result<(), String> {
    for (index, (argument, width)) in float_args.iter().enumerate() {
        emit_gpr_to_fp(assembly, index as u32, args[*argument].0, *width)?;
    }
    Ok(())
}

fn emit_float_binary_assembly(
    assembly: &mut String,
    destination: ValueId,
    lhs: ValueId,
    rhs: ValueId,
    kind: FloatBinaryKind,
    width: u16,
) -> Result<(), String> {
    let register = match width {
        16 => "h",
        32 => "s",
        64 => "d",
        _ => {
            return Err(format!(
                "ARM64 backend does not support f{width} arithmetic yet"
            ));
        }
    };
    emit_gpr_to_fp(assembly, 16, lhs.0, width)?;
    emit_gpr_to_fp(assembly, 17, rhs.0, width)?;
    let operation = match kind {
        FloatBinaryKind::Add => "fadd",
        FloatBinaryKind::Subtract => "fsub",
        FloatBinaryKind::Multiply => "fmul",
        FloatBinaryKind::Divide => "fdiv",
    };
    assembly.push_str(&format!(
        "    {operation} {register}16, {register}16, {register}17\n"
    ));
    emit_fp_to_gpr(assembly, destination.0, 16, width)
}

fn emit_float_neg_assembly(
    assembly: &mut String,
    destination: ValueId,
    value: ValueId,
    width: u16,
) -> Result<(), String> {
    let register = match width {
        16 => "h",
        32 => "s",
        64 => "d",
        _ => {
            return Err(format!(
                "ARM64 backend does not support f{width} negation yet"
            ));
        }
    };
    emit_gpr_to_fp(assembly, 16, value.0, width)?;
    assembly.push_str(&format!("    fneg {register}16, {register}16\n"));
    emit_fp_to_gpr(assembly, destination.0, 16, width)
}

fn emit_float_compare_assembly(
    assembly: &mut String,
    destination: ValueId,
    lhs: ValueId,
    rhs: ValueId,
    kind: CompareKind,
    width: u16,
) -> Result<(), String> {
    let register = match width {
        16 => "h",
        32 => "s",
        64 => "d",
        _ => {
            return Err(format!(
                "ARM64 backend does not support f{width} comparisons yet"
            ));
        }
    };
    emit_gpr_to_fp(assembly, 16, lhs.0, width)?;
    emit_gpr_to_fp(assembly, 17, rhs.0, width)?;
    let condition = match kind {
        CompareKind::Equal => "eq",
        CompareKind::NotEqual => "ne",
        CompareKind::GreaterEqual => "ge",
        CompareKind::Less => "lo",
        CompareKind::Greater => "gt",
        CompareKind::LessEqual => "ls",
    };
    assembly.push_str(&format!(
        "    fcmp {register}16, {register}17\n    cset x{}, {condition}\n",
        destination.0
    ));
    Ok(())
}

fn emit_assembly_phi_copies(
    assembly: &mut String,
    function: &LirFunction,
    from: crate::mir::BlockId,
    to: crate::mir::BlockId,
) {
    let moves = phi_copies(function, from, to)
        .into_iter()
        .map(|(destination, source)| (destination.0, source.0))
        .collect::<Vec<_>>();
    emit_register_moves(assembly, &parallel_moves(&moves));
}
