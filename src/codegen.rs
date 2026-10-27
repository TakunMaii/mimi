use std::path::Path;
use std::process::Command;

use crate::lir::{LirFunction, LirInstruction, LirModule, TargetArch};
use crate::mir::{CompareKind, MirTerminator, ValueId};

#[derive(Clone, Copy)]
enum RelocationKind {
    Branch26,
    Page21,
    PageOff12,
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
        TargetArch::Aarch64 => emit_aarch64(module),
        TargetArch::X86_64 => Err("x86_64 code generation is not implemented yet".into()),
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
    if module.target != TargetArch::Aarch64 {
        return Err("machine code emission is currently implemented for ARM64 only".into());
    }
    let (text, data, symbols, relocations) = encode_module(module)?;
    let object = write_macho_object(&text, &data, &symbols, &relocations);
    let object_path = output.with_extension("o");
    std::fs::write(&object_path, object)
        .map_err(|error| format!("cannot write object file: {error}"))?;
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
    let result = command
        .arg(&object_path)
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|error| format!("cannot invoke ld: {error}"))?;
    let _ = std::fs::remove_file(&object_path);
    if result.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).into_owned())
    }
}

fn encode_module(
    module: &LirModule,
) -> Result<(Vec<u8>, Vec<u8>, Vec<ObjectSymbol>, Vec<Relocation>), String> {
    let mut offsets = std::collections::HashMap::new();
    let mut offset = 0u64;
    for function in &module.functions {
        offsets.insert(format!("_{}", function.name), offset);
        offset += encoded_function_size(function)? as u64;
    }
    let mut text = Vec::new();
    let mut symbols = Vec::new();
    let mut relocations = Vec::new();
    for function in &module.functions {
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
    if function.blocks.len() > 1 {
        return Err("multi-block LIR code generation is not implemented yet".into());
    }
    let block = &function.blocks[0];
    let mut size = 0;
    let has_calls = block
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, LirInstruction::Call { .. }));
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
            LirInstruction::Rem { .. } | LirInstruction::Compare { .. } => 2,
            LirInstruction::Phi { .. } => 0,
            LirInstruction::Call { args, result, .. } => {
                args.iter()
                    .enumerate()
                    .filter(|(index, argument)| argument.0 != *index as u32)
                    .count()
                    + 1
                    + usize::from(result.is_some_and(|value| value.0 != 0))
            }
        } * 4;
    }
    if matches!(block.terminator, MirTerminator::Return(Some(value)) if value.0 != 0) {
        size += 4;
    }
    if has_calls && function.name != "main" {
        size += 4;
    }
    size += if function.name == "main" {
        8 + usize::from(matches!(block.terminator, MirTerminator::Return(None))) * 4
    } else {
        4 + usize::from(has_calls) * 4
    };
    Ok(size)
}

fn encode_function(
    function: &LirFunction,
    bytes: &mut Vec<u8>,
    offsets: &std::collections::HashMap<String, u64>,
    relocations: &mut Vec<Relocation>,
) -> Result<(), String> {
    if function.params.len() > 8 {
        return Err("ARM64 backend currently supports at most 8 integer parameters".into());
    }
    let block = &function.blocks[0];
    if block
        .instructions
        .iter()
        .flat_map(instruction_registers)
        .any(|register| register.0 > 15)
    {
        return Err("ARM64 prototype backend supports virtual registers x0 through x15".into());
    }
    let has_calls = block
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, LirInstruction::Call { .. }));
    if has_calls && function.name != "main" {
        push_word(bytes, 0xa9bf_7bfd);
    }
    for instruction in &block.instructions {
        let word = match instruction {
            LirInstruction::LoadImmediate { register, value } => {
                encode_immediate(register.0, *value as u64, bytes)?;
                continue;
            }
            LirInstruction::LoadAddress { symbol, .. } => {
                let register = match instruction {
                    LirInstruction::LoadAddress { register, .. } => *register,
                    _ => unreachable!(),
                };
                let page = bytes.len() as u32;
                push_word(bytes, 0x9000_0000 | register.0);
                let low = bytes.len() as u32;
                push_word(bytes, 0x9100_0000 | (register.0 << 5) | register.0);
                relocations.push(Relocation {
                    offset: page,
                    symbol: symbol.clone(),
                    kind: RelocationKind::Page21,
                });
                relocations.push(Relocation {
                    offset: low,
                    symbol: symbol.clone(),
                    kind: RelocationKind::PageOff12,
                });
                continue;
            }
            LirInstruction::Add {
                destination,
                lhs,
                rhs,
            } => 0x8b00_0000 | (rhs.0 << 16) | (lhs.0 << 5) | destination.0,
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
            LirInstruction::Neg { destination, value } => {
                push_word(bytes, 0xcb00_03e0 | (value.0 << 16) | destination.0);
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
            LirInstruction::Call { result, name, args } => {
                for (index, argument) in args.iter().enumerate() {
                    if argument.0 != index as u32 {
                        push_word(bytes, 0xaa00_03e0 | (argument.0 << 16) | index as u32);
                    }
                }
                let call_address = bytes.len() as u64;
                let target_name = format!("_{name}");
                if let Some(target) = offsets.get(&target_name) {
                    let delta = (*target as i64 - call_address as i64) / 4;
                    if !(-(1 << 25)..(1 << 25)).contains(&delta) {
                        return Err(format!("call target '{name}' is out of ARM64 branch range"));
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
                if let Some(result) = result
                    && result.0 != 0
                {
                    push_word(bytes, 0xaa00_03e0 | (result.0 << 16));
                }
                continue;
            }
            LirInstruction::Phi { .. } => {
                return Err("phi nodes require multi-block code generation".into());
            }
        };
        push_word(bytes, word);
    }
    let result = match block.terminator {
        MirTerminator::Return(value) => value,
        _ => return Err("ARM64 backend currently requires a single return block".into()),
    };
    if let Some(value) = result {
        if value.0 != 0 {
            push_word(bytes, 0xaa00_03e0 | (value.0 << 16)); // mov x0, xn
        }
    }
    if function.name == "main" {
        if result.is_none() {
            encode_immediate(0, 0, bytes)?;
        }
        encode_immediate(16, 1, bytes)?;
        push_word(bytes, 0xd400_1001); // svc #0x80
    } else {
        if has_calls {
            push_word(bytes, 0xa8c1_7bfd);
        }
        push_word(bytes, 0xd65f_03c0); // ret
    }
    Ok(())
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
        } => {
            vec![*destination, *lhs, *rhs]
        }
        LirInstruction::Neg { destination, value } => vec![*destination, *value],
        LirInstruction::Call { result, args, .. } => {
            result.iter().copied().chain(args.iter().copied()).collect()
        }
        LirInstruction::Phi {
            destination,
            incoming,
        } => std::iter::once(*destination)
            .chain(incoming.iter().map(|(_, value)| *value))
            .collect(),
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
        emit_function(function, &mut assembly)?;
    }
    Ok(assembly)
}

fn emit_function(function: &LirFunction, assembly: &mut String) -> Result<(), String> {
    if function.params.len() > 8 {
        return Err("ARM64 backend currently supports at most 8 integer parameters".into());
    }
    let label = format!("_{}", function.name);
    assembly.push_str(&format!(".globl {label}\n{label}:\n"));
    for instruction in &function.blocks[0].instructions {
        match instruction {
            LirInstruction::LoadImmediate { register, value } => {
                if !(-4095..=4095).contains(value) {
                    return Err(format!(
                        "integer immediate {value} is too large for ARM64 prototype backend"
                    ));
                }
                assembly.push_str(&format!("    mov x{}, #{}\n", register.0, value));
            }
            LirInstruction::LoadAddress { symbol, .. } => {
                return Err(format!(
                    "string address '{symbol}' requires data-section relocation support"
                ));
            }
            LirInstruction::Add {
                destination,
                lhs,
                rhs,
            } => {
                assembly.push_str(&format!(
                    "    add x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                ));
            }
            LirInstruction::Sub {
                destination,
                lhs,
                rhs,
            } => {
                assembly.push_str(&format!(
                    "    sub x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                ));
            }
            LirInstruction::Mul {
                destination,
                lhs,
                rhs,
            } => {
                assembly.push_str(&format!(
                    "    mul x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                ));
            }
            LirInstruction::Div {
                destination,
                lhs,
                rhs,
            } => {
                assembly.push_str(&format!(
                    "    sdiv x{}, x{}, x{}\n",
                    destination.0, lhs.0, rhs.0
                ));
            }
            LirInstruction::Neg { destination, value } => {
                assembly.push_str(&format!("    neg x{}, x{}\n", destination.0, value.0));
            }
            LirInstruction::Rem {
                destination,
                lhs,
                rhs,
            } => {
                assembly.push_str(&format!(
                    "    sdiv x16, x{}, x{}\n    msub x{}, x16, x{}, x{}\n",
                    lhs.0, rhs.0, destination.0, rhs.0, lhs.0
                ));
            }
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
                    "    cmp x{}, x{}\n    cset x{}, {}\n",
                    lhs.0, rhs.0, destination.0, condition
                ));
            }
            LirInstruction::Call { name, .. } => {
                return Err(format!(
                    "external call '{name}' is not yet supported by the ARM64 assembly emitter"
                ));
            }
            LirInstruction::Phi { .. } => {
                return Err("phi nodes require multi-block code generation".into());
            }
        }
    }
    if let Some(value) = return_value(function) {
        if value.0 != 0 {
            assembly.push_str(&format!("    mov x0, x{}\n", value.0));
        }
    }
    assembly.push_str("    ret\n");
    Ok(())
}

fn return_value(function: &LirFunction) -> Option<ValueId> {
    match function.blocks.first()?.terminator {
        MirTerminator::Return(value) => value,
        _ => None,
    }
}
