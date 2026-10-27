use std::path::Path;
use std::process::Command;

use crate::lir::{LirFunction, LirInstruction, LirModule, TargetArch};
use crate::mir::{MirTerminator, ValueId};

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
    let (text, symbols) = encode_module(module)?;
    let object = write_macho_object(&text, &symbols);
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

fn encode_module(module: &LirModule) -> Result<(Vec<u8>, Vec<(String, u64)>), String> {
    let mut text = Vec::new();
    let mut symbols = Vec::new();
    for function in &module.functions {
        symbols.push((format!("_{}", function.name), text.len() as u64));
        encode_function(function, &mut text)?;
    }
    Ok((text, symbols))
}

fn encode_function(function: &LirFunction, bytes: &mut Vec<u8>) -> Result<(), String> {
    if function.params.len() > 8 {
        return Err("ARM64 backend currently supports at most 8 integer parameters".into());
    }
    if function
        .instructions
        .iter()
        .flat_map(instruction_registers)
        .any(|register| register.0 > 15)
    {
        return Err("ARM64 prototype backend supports virtual registers x0 through x15".into());
    }
    for instruction in &function.instructions {
        let word = match instruction {
            LirInstruction::LoadImmediate { register, value } => {
                encode_immediate(register.0, *value as u64, bytes)?;
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
            LirInstruction::Call { name, .. } => {
                return Err(format!(
                    "external call '{name}' is not yet supported by the ARM64 machine-code emitter"
                ));
            }
        };
        push_word(bytes, word);
    }
    let result = match function.terminator {
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
        push_word(bytes, 0xd65f_03c0); // ret
    }
    Ok(())
}

fn instruction_registers(instruction: &LirInstruction) -> Vec<ValueId> {
    match instruction {
        LirInstruction::LoadImmediate { register, .. } => vec![*register],
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
        LirInstruction::Call { result, args, .. } => {
            result.iter().copied().chain(args.iter().copied()).collect()
        }
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

fn write_macho_object(text: &[u8], symbols: &[(String, u64)]) -> Vec<u8> {
    const HEADER_SIZE: usize = 32;
    const SEGMENT_COMMAND_SIZE: usize = 152;
    const SYMTAB_COMMAND_SIZE: usize = 24;
    let commands_size = SEGMENT_COMMAND_SIZE + SYMTAB_COMMAND_SIZE;
    let text_offset = HEADER_SIZE + commands_size;
    let symbol_offset = (text_offset + text.len() + 7) & !7;
    let string_offset = symbol_offset + symbols.len() * 16;
    let mut strings = vec![0u8];
    let mut string_indexes = Vec::new();
    for (name, _) in symbols {
        string_indexes.push(strings.len() as u32);
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
    }
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
    for value in [0u64, 0, text_offset as u64, text.len() as u64] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    for value in [7u32, 5, 1, 0] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    write_name(&mut output, "__text");
    write_name(&mut output, "__TEXT");
    output.extend_from_slice(&0u64.to_le_bytes());
    output.extend_from_slice(&(text.len() as u64).to_le_bytes());
    for value in [text_offset as u32, 2, 0, 0, 0x8000_0400u32, 0, 0, 0] {
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
    output.resize(symbol_offset, 0);
    for ((_, value), string_index) in symbols.iter().zip(string_indexes) {
        output.extend_from_slice(&string_index.to_le_bytes());
        output.push(0x0f); // N_SECT | N_EXT
        output.push(1);
        output.extend_from_slice(&0u16.to_le_bytes());
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
    for instruction in &function.instructions {
        match instruction {
            LirInstruction::LoadImmediate { register, value } => {
                if !(-4095..=4095).contains(value) {
                    return Err(format!(
                        "integer immediate {value} is too large for ARM64 prototype backend"
                    ));
                }
                assembly.push_str(&format!("    mov x{}, #{}\n", register.0, value));
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
            LirInstruction::Call { name, .. } => {
                return Err(format!(
                    "external call '{name}' is not yet supported by the ARM64 assembly emitter"
                ));
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
    match function.terminator {
        MirTerminator::Return(value) => value,
        _ => None,
    }
}
