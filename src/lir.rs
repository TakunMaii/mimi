use crate::mir::{
    CompareKind, MirFunction, MirInstruction, MirModule, MirTerminator, SlotId, ValueId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetArch {
    Aarch64,
    X86_64,
}

pub(crate) trait Target {
    fn arch(&self) -> TargetArch;
    fn pointer_bits(&self) -> u16;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HostTarget;

impl Target for HostTarget {
    fn arch(&self) -> TargetArch {
        if cfg!(target_arch = "aarch64") {
            TargetArch::Aarch64
        } else {
            TargetArch::X86_64
        }
    }

    fn pointer_bits(&self) -> u16 {
        usize::BITS as u16
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LirInstruction {
    LoadImmediate {
        register: ValueId,
        value: i128,
    },
    LoadAddress {
        register: ValueId,
        symbol: String,
    },
    Add {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Sub {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Mul {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Div {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Rem {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Neg {
        destination: ValueId,
        value: ValueId,
    },
    Not {
        destination: ValueId,
        value: ValueId,
    },
    Compare {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
        kind: CompareKind,
    },
    Call {
        result: Option<ValueId>,
        name: String,
        args: Vec<ValueId>,
    },
    Phi {
        destination: ValueId,
        incoming: Vec<(crate::mir::BlockId, ValueId)>,
    },
    LoadSlot {
        destination: ValueId,
        slot: SlotId,
    },
    StoreSlot {
        slot: SlotId,
        value: ValueId,
    },
    SlotAddress {
        destination: ValueId,
        slot: SlotId,
    },
    IndexAddress {
        destination: ValueId,
        base: ValueId,
        index: ValueId,
        scale: u8,
    },
    LoadIndirect {
        destination: ValueId,
        address: ValueId,
    },
    LoadByteIndirect {
        destination: ValueId,
        address: ValueId,
    },
    StoreIndirect {
        address: ValueId,
        value: ValueId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LirFunction {
    pub(crate) name: String,
    pub(crate) params: Vec<ValueId>,
    pub(crate) slot_count: u32,
    pub(crate) blocks: Vec<LirBlock>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LirBlock {
    pub(crate) id: crate::mir::BlockId,
    pub(crate) instructions: Vec<LirInstruction>,
    pub(crate) terminator: MirTerminator,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LirModule {
    pub(crate) target: TargetArch,
    pub(crate) functions: Vec<LirFunction>,
    pub(crate) strings: Vec<(String, Vec<u8>)>,
}

pub(crate) fn lower_mir<T: Target>(module: &MirModule, target: &T) -> LirModule {
    let mut strings = Vec::new();
    LirModule {
        target: target.arch(),
        functions: module
            .functions
            .iter()
            .map(|function| lower_function(function, &mut strings))
            .collect(),
        strings,
    }
}

fn lower_function(function: &MirFunction, strings: &mut Vec<(String, Vec<u8>)>) -> LirFunction {
    let blocks = function
        .blocks
        .iter()
        .map(|block| {
            let instructions = block
                .instructions
                .iter()
                .filter_map(|instruction| {
                    Some(match instruction {
                        MirInstruction::ConstInt { result, value } => {
                            LirInstruction::LoadImmediate {
                                register: *result,
                                value: *value,
                            }
                        }
                        MirInstruction::Add { result, lhs, rhs } => LirInstruction::Add {
                            destination: *result,
                            lhs: *lhs,
                            rhs: *rhs,
                        },
                        MirInstruction::Sub { result, lhs, rhs } => LirInstruction::Sub {
                            destination: *result,
                            lhs: *lhs,
                            rhs: *rhs,
                        },
                        MirInstruction::Mul { result, lhs, rhs } => LirInstruction::Mul {
                            destination: *result,
                            lhs: *lhs,
                            rhs: *rhs,
                        },
                        MirInstruction::Div { result, lhs, rhs } => LirInstruction::Div {
                            destination: *result,
                            lhs: *lhs,
                            rhs: *rhs,
                        },
                        MirInstruction::Rem { result, lhs, rhs } => LirInstruction::Rem {
                            destination: *result,
                            lhs: *lhs,
                            rhs: *rhs,
                        },
                        MirInstruction::Neg { result, value } => LirInstruction::Neg {
                            destination: *result,
                            value: *value,
                        },
                        MirInstruction::Not { result, value } => LirInstruction::Not {
                            destination: *result,
                            value: *value,
                        },
                        MirInstruction::Compare {
                            result,
                            lhs,
                            rhs,
                            kind,
                        } => LirInstruction::Compare {
                            destination: *result,
                            lhs: *lhs,
                            rhs: *rhs,
                            kind: *kind,
                        },
                        MirInstruction::Phi { result, incoming } => LirInstruction::Phi {
                            destination: *result,
                            incoming: incoming.clone(),
                        },
                        MirInstruction::ConstBool { result, value } => {
                            LirInstruction::LoadImmediate {
                                register: *result,
                                value: i128::from(*value),
                            }
                        }
                        MirInstruction::ConstString { result, bytes } => {
                            let symbol = format!("_mimi_string_{}", strings.len());
                            strings.push((symbol.clone(), bytes.clone()));
                            LirInstruction::LoadAddress {
                                register: *result,
                                symbol,
                            }
                        }
                        MirInstruction::Call { result, name, args } => LirInstruction::Call {
                            result: *result,
                            name: name.clone(),
                            args: args.clone(),
                        },
                        MirInstruction::LoadSlot { result, slot } => LirInstruction::LoadSlot {
                            destination: *result,
                            slot: *slot,
                        },
                        MirInstruction::StoreSlot { slot, value } => LirInstruction::StoreSlot {
                            slot: *slot,
                            value: *value,
                        },
                        MirInstruction::SlotAddress { result, slot } => {
                            LirInstruction::SlotAddress {
                                destination: *result,
                                slot: *slot,
                            }
                        }
                        MirInstruction::IndexAddress {
                            result,
                            base,
                            index,
                            scale,
                        } => LirInstruction::IndexAddress {
                            destination: *result,
                            base: *base,
                            index: *index,
                            scale: *scale,
                        },
                        MirInstruction::LoadIndirect { result, address } => {
                            LirInstruction::LoadIndirect {
                                destination: *result,
                                address: *address,
                            }
                        }
                        MirInstruction::LoadByteIndirect { result, address } => {
                            LirInstruction::LoadByteIndirect {
                                destination: *result,
                                address: *address,
                            }
                        }
                        MirInstruction::StoreIndirect { address, value } => {
                            LirInstruction::StoreIndirect {
                                address: *address,
                                value: *value,
                            }
                        }
                    })
                })
                .collect();
            LirBlock {
                id: block.id,
                instructions,
                terminator: block.terminator.clone(),
            }
        })
        .collect();
    LirFunction {
        name: function.name.clone(),
        params: function.abi_params.clone(),
        slot_count: function.slot_count,
        blocks,
    }
}
