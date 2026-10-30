use std::collections::{HashMap, HashSet};

use crate::lir::{LirBlock, LirFunction, LirInstruction};
use crate::mir::{BlockId, MirTerminator, SlotId, ValueId};

type Values = HashSet<ValueId>;

pub(crate) fn allocate_registers(function: &LirFunction) -> Result<LirFunction, String> {
    let mut function = spill_values_across_calls(function);
    let (live_in, live_out) = block_liveness(&function);
    let mut interference = HashMap::<ValueId, HashSet<ValueId>>::new();

    for parameter in &function.params {
        interference.entry(*parameter).or_default();
    }
    for block in &function.blocks {
        let mut live = live_out[block.id.0 as usize].clone();
        let terminator_values = terminator_uses(&block.terminator);
        for value in &terminator_values {
            interference.entry(*value).or_default();
        }
        live.extend(terminator_values);
        for instruction in block.instructions.iter().rev() {
            let (uses, definitions) = instruction_uses_defs(instruction);
            for value in uses.iter().chain(&definitions) {
                interference.entry(*value).or_default();
            }
            for (index, lhs) in uses.iter().enumerate() {
                for rhs in uses.iter().skip(index + 1) {
                    if lhs != rhs {
                        interference.entry(*lhs).or_default().insert(*rhs);
                        interference.entry(*rhs).or_default().insert(*lhs);
                    }
                }
            }
            for definition in &definitions {
                for value in &live {
                    if value != definition {
                        interference.entry(*definition).or_default().insert(*value);
                        interference.entry(*value).or_default().insert(*definition);
                    }
                }
            }
            for definition in definitions {
                live.remove(&definition);
            }
            live.extend(uses);
        }
    }

    let mut colors = HashMap::<ValueId, u32>::new();
    if function.float_params.len() > 8 {
        return Err("ARM64 backend currently supports at most 8 floating-point parameters".into());
    }
    for (index, parameter) in function.params.iter().enumerate() {
        if index >= 16 {
            return Err("ARM64 register allocator supports at most 16 integer parameters".into());
        }
        colors.insert(*parameter, index as u32);
    }
    for (index, (parameter, _)) in function.float_params.iter().enumerate() {
        colors.insert(*parameter, 19 + index as u32);
    }
    let mut order = interference.keys().copied().collect::<Vec<_>>();
    order.sort_by_key(|value| std::cmp::Reverse(interference.get(value).map_or(0, HashSet::len)));
    for value in order {
        if colors.contains_key(&value) {
            continue;
        }
        let used = interference
            .get(&value)
            .into_iter()
            .flat_map(|neighbors| neighbors.iter())
            .filter_map(|neighbor| colors.get(neighbor).copied())
            .collect::<HashSet<_>>();
        let color = (0..16)
            .chain(19..=28)
            .find(|color| !used.contains(color))
            .ok_or_else(|| {
                "ARM64 register pressure exceeds 26 allocatable registers".to_string()
            })?;
        colors.insert(value, color);
    }

    rewrite_function(&mut function, &colors)?;
    let _ = live_in;
    Ok(function)
}

fn spill_values_across_calls(function: &LirFunction) -> LirFunction {
    let (_, live_after) = instruction_liveness(function);
    let mut next_slot = function.slot_count;
    let blocks = function
        .blocks
        .iter()
        .map(|block| {
            let mut instructions = Vec::new();
            for (index, instruction) in block.instructions.iter().enumerate() {
                let values = if let LirInstruction::Call {
                    result,
                    result_slice_length,
                    ..
                }
                | LirInstruction::CallIndirect {
                    result,
                    result_slice_length,
                    ..
                } = instruction
                {
                    live_after[block.id.0 as usize][index]
                        .iter()
                        .copied()
                        .filter(|value| {
                            Some(*value) != *result && Some(*value) != *result_slice_length
                        })
                        .collect::<HashSet<_>>()
                } else {
                    HashSet::new()
                };
                let mut spills = values.into_iter().collect::<Vec<_>>();
                spills.sort_by_key(|value| value.0);
                let slots = spills
                    .iter()
                    .map(|value| {
                        let slot = SlotId(next_slot);
                        next_slot += 1;
                        (*value, slot)
                    })
                    .collect::<Vec<_>>();
                for (value, slot) in &slots {
                    instructions.push(LirInstruction::StoreSlot {
                        slot: *slot,
                        value: *value,
                    });
                }
                instructions.push(instruction.clone());
                for (value, slot) in slots {
                    instructions.push(LirInstruction::LoadSlot {
                        destination: value,
                        slot,
                    });
                }
            }
            LirBlock {
                instructions,
                ..block.clone()
            }
        })
        .collect();
    LirFunction {
        slot_count: next_slot,
        blocks,
        ..function.clone()
    }
}

fn instruction_liveness(function: &LirFunction) -> (Vec<Values>, Vec<Vec<Values>>) {
    let (live_in, live_out) = block_liveness(function);
    let mut live_after = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        let mut live = live_out[block.id.0 as usize].clone();
        live.extend(terminator_uses(&block.terminator));
        let mut states = vec![Values::new(); block.instructions.len()];
        for (index, instruction) in block.instructions.iter().enumerate().rev() {
            states[index] = live.clone();
            let (uses, definitions) = instruction_uses_defs(instruction);
            for definition in definitions {
                live.remove(&definition);
            }
            live.extend(uses);
        }
        live_after[block.id.0 as usize] = states;
    }
    (live_in, live_after)
}

fn block_liveness(function: &LirFunction) -> (Vec<Values>, Vec<Values>) {
    let count = function.blocks.len();
    let mut uses = vec![Values::new(); count];
    let mut definitions = vec![Values::new(); count];
    let mut edge_uses = HashMap::<(BlockId, BlockId), Values>::new();

    for block in &function.blocks {
        let index = block.id.0 as usize;
        for instruction in &block.instructions {
            if let LirInstruction::Phi {
                destination,
                incoming,
            } = instruction
            {
                definitions[index].insert(*destination);
                for (predecessor, value) in incoming {
                    edge_uses
                        .entry((*predecessor, block.id))
                        .or_default()
                        .insert(*value);
                }
                continue;
            }
            let (instruction_uses, instruction_definitions) = instruction_uses_defs(instruction);
            for value in instruction_uses {
                if !definitions[index].contains(&value) {
                    uses[index].insert(value);
                }
            }
            definitions[index].extend(instruction_definitions);
        }
        for value in terminator_uses(&block.terminator) {
            if !definitions[index].contains(&value) {
                uses[index].insert(value);
            }
        }
    }

    let mut live_in = vec![Values::new(); count];
    let mut live_out = vec![Values::new(); count];
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let index = block.id.0 as usize;
            let mut new_out = Values::new();
            for successor in successors(&block.terminator) {
                new_out.extend(live_in[successor.0 as usize].iter().copied());
                if let Some(values) = edge_uses.get(&(block.id, successor)) {
                    new_out.extend(values.iter().copied());
                }
            }
            let mut new_in = uses[index].clone();
            new_in.extend(new_out.difference(&definitions[index]).copied());
            if new_out != live_out[index] || new_in != live_in[index] {
                live_out[index] = new_out;
                live_in[index] = new_in;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (live_in, live_out)
}

fn successors(terminator: &MirTerminator) -> Vec<BlockId> {
    match terminator {
        MirTerminator::Jump(block) => vec![*block],
        MirTerminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        MirTerminator::Return { .. } | MirTerminator::Unreachable => Vec::new(),
    }
}

fn terminator_uses(terminator: &MirTerminator) -> Vec<ValueId> {
    match terminator {
        MirTerminator::Return {
            value,
            slice_length,
        } => value.iter().chain(slice_length.iter()).copied().collect(),
        MirTerminator::Branch { condition, .. } => vec![*condition],
        MirTerminator::Jump(_) | MirTerminator::Unreachable => Vec::new(),
    }
}

fn instruction_uses_defs(instruction: &LirInstruction) -> (Vec<ValueId>, Vec<ValueId>) {
    match instruction {
        LirInstruction::LoadImmediate { register, .. }
        | LirInstruction::LoadAddress { register, .. } => (Vec::new(), vec![*register]),
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
        }
        | LirInstruction::IndexAddress {
            destination,
            base: lhs,
            index: rhs,
            ..
        } => (vec![*lhs, *rhs], vec![*destination]),
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
        } => (vec![*lhs, *rhs], vec![*destination]),
        LirInstruction::Neg { destination, value } | LirInstruction::Not { destination, value } => {
            (vec![*value], vec![*destination])
        }
        LirInstruction::FloatNeg {
            destination, value, ..
        } => (vec![*value], vec![*destination]),
        LirInstruction::FloatConvert {
            destination, value, ..
        } => (vec![*value], vec![*destination]),
        LirInstruction::Call {
            result,
            result_slice_length,
            args,
            ..
        } => (
            args.clone(),
            result
                .iter()
                .chain(result_slice_length.iter())
                .copied()
                .collect(),
        ),
        LirInstruction::CallIndirect {
            result,
            result_slice_length,
            callee,
            args,
            ..
        } => (
            std::iter::once(*callee)
                .chain(args.iter().copied())
                .collect(),
            result
                .iter()
                .chain(result_slice_length.iter())
                .copied()
                .collect(),
        ),
        LirInstruction::Phi { destination, .. } => (Vec::new(), vec![*destination]),
        LirInstruction::LoadSlot { destination, .. }
        | LirInstruction::SlotAddress { destination, .. } => (Vec::new(), vec![*destination]),
        LirInstruction::StoreSlot { value, .. } => (vec![*value], Vec::new()),
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
        } => (vec![*address], vec![*destination]),
        LirInstruction::StoreIndirect { address, value }
        | LirInstruction::StoreByteIndirect { address, value }
        | LirInstruction::StoreSizedIndirect { address, value, .. } => {
            (vec![*address, *value], Vec::new())
        }
    }
}

fn rewrite_function(
    function: &mut LirFunction,
    colors: &HashMap<ValueId, u32>,
) -> Result<(), String> {
    let map_value = |value: &mut ValueId| -> Result<(), String> {
        value.0 = *colors.get(value).ok_or_else(|| {
            format!(
                "no physical register assigned to virtual register {}",
                value.0
            )
        })?;
        Ok(())
    };
    for parameter in &mut function.params {
        map_value(parameter)?;
    }
    for (parameter, _) in &mut function.float_params {
        map_value(parameter)?;
    }
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            match instruction {
                LirInstruction::LoadImmediate { register, .. }
                | LirInstruction::LoadAddress { register, .. } => map_value(register)?,
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
                | LirInstruction::IndexAddress {
                    destination,
                    base: lhs,
                    index: rhs,
                    ..
                } => {
                    map_value(destination)?;
                    map_value(lhs)?;
                    map_value(rhs)?;
                }
                LirInstruction::FloatBinary {
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
                    map_value(destination)?;
                    map_value(lhs)?;
                    map_value(rhs)?;
                }
                LirInstruction::Neg { destination, value }
                | LirInstruction::Not { destination, value } => {
                    map_value(destination)?;
                    map_value(value)?;
                }
                LirInstruction::FloatNeg {
                    destination, value, ..
                } => {
                    map_value(destination)?;
                    map_value(value)?;
                }
                LirInstruction::FloatConvert {
                    destination, value, ..
                } => {
                    map_value(destination)?;
                    map_value(value)?;
                }
                LirInstruction::Call {
                    result,
                    result_slice_length,
                    args,
                    ..
                } => {
                    if let Some(result) = result {
                        map_value(result)?;
                    }
                    if let Some(length) = result_slice_length {
                        map_value(length)?;
                    }
                    for argument in args {
                        map_value(argument)?;
                    }
                }
                LirInstruction::CallIndirect {
                    result,
                    result_slice_length,
                    callee,
                    args,
                    ..
                } => {
                    if let Some(result) = result {
                        map_value(result)?;
                    }
                    if let Some(length) = result_slice_length {
                        map_value(length)?;
                    }
                    map_value(callee)?;
                    for argument in args {
                        map_value(argument)?;
                    }
                }
                LirInstruction::Phi {
                    destination,
                    incoming,
                } => {
                    map_value(destination)?;
                    for (_, value) in incoming {
                        map_value(value)?;
                    }
                }
                LirInstruction::LoadSlot { destination, .. }
                | LirInstruction::SlotAddress { destination, .. } => map_value(destination)?,
                LirInstruction::StoreSlot { value, .. } => map_value(value)?,
                LirInstruction::LoadIndirect {
                    destination,
                    address,
                }
                | LirInstruction::LoadByteIndirect {
                    destination,
                    address,
                } => {
                    map_value(destination)?;
                    map_value(address)?;
                }
                LirInstruction::LoadSizedIndirect {
                    destination,
                    address,
                    ..
                } => {
                    map_value(destination)?;
                    map_value(address)?;
                }
                LirInstruction::StoreIndirect { address, value }
                | LirInstruction::StoreByteIndirect { address, value }
                | LirInstruction::StoreSizedIndirect { address, value, .. } => {
                    map_value(address)?;
                    map_value(value)?;
                }
            }
        }
        match &mut block.terminator {
            MirTerminator::Return {
                value,
                slice_length,
            } => {
                if let Some(value) = value {
                    map_value(value)?;
                }
                if let Some(length) = slice_length {
                    map_value(length)?;
                }
            }
            MirTerminator::Branch { condition, .. } => map_value(condition)?,
            MirTerminator::Jump(_) | MirTerminator::Unreachable => {}
        }
    }
    Ok(())
}
