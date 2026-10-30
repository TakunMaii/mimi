use crate::hir::HirType;
use crate::mir::{
    CompareKind, FloatBinaryKind, MirFunction, MirInstruction, MirModule, MirTerminator, SlotId,
    ValueId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetArch {
    Aarch64,
    X86_64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TargetDataLayout {
    pointer_bytes: u64,
    stack_slot_bytes: u64,
}

impl TargetDataLayout {
    pub(crate) const HOST_64: Self = Self {
        pointer_bytes: 8,
        stack_slot_bytes: 8,
    };

    pub(crate) const fn new(pointer_bytes: u64, stack_slot_bytes: u64) -> Self {
        Self {
            pointer_bytes,
            stack_slot_bytes,
        }
    }

    pub(crate) const fn pointer_bytes(self) -> u64 {
        self.pointer_bytes
    }

    pub(crate) const fn stack_slot_bytes(self) -> u64 {
        self.stack_slot_bytes
    }
}

impl Default for TargetDataLayout {
    fn default() -> Self {
        Self::HOST_64
    }
}

pub(crate) trait Target {
    fn arch(&self) -> TargetArch;
    fn data_layout(&self) -> TargetDataLayout;
}

impl Target for TargetArch {
    fn arch(&self) -> TargetArch {
        *self
    }

    fn data_layout(&self) -> TargetDataLayout {
        TargetDataLayout::HOST_64
    }
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

    fn data_layout(&self) -> TargetDataLayout {
        TargetDataLayout::new((usize::BITS / 8) as u64, 8)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct X86_64Target;

impl Target for X86_64Target {
    fn arch(&self) -> TargetArch {
        TargetArch::X86_64
    }

    fn data_layout(&self) -> TargetDataLayout {
        TargetDataLayout::HOST_64
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
        external: bool,
    },
    CallIndirect {
        result: Option<ValueId>,
        result_slice_length: Option<ValueId>,
        callee: ValueId,
        args: Vec<ValueId>,
        float_args: Vec<(usize, u16)>,
        result_float: Option<u16>,
    },
    Add {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    FloatBinary {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
        kind: FloatBinaryKind,
        width: u16,
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
    FloatNeg {
        destination: ValueId,
        value: ValueId,
        width: u16,
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
    FloatCompare {
        destination: ValueId,
        lhs: ValueId,
        rhs: ValueId,
        kind: CompareKind,
        width: u16,
    },
    Call {
        result: Option<ValueId>,
        result_slice_length: Option<ValueId>,
        name: String,
        args: Vec<ValueId>,
        float_args: Vec<(usize, u16)>,
        result_float: Option<u16>,
        variadic: bool,
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
        scale: u64,
    },
    LoadIndirect {
        destination: ValueId,
        address: ValueId,
    },
    LoadByteIndirect {
        destination: ValueId,
        address: ValueId,
    },
    LoadSizedIndirect {
        destination: ValueId,
        address: ValueId,
        bytes: u8,
        signed: bool,
    },
    StoreIndirect {
        address: ValueId,
        value: ValueId,
    },
    StoreByteIndirect {
        address: ValueId,
        value: ValueId,
    },
    StoreSizedIndirect {
        address: ValueId,
        value: ValueId,
        bytes: u8,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LirFunction {
    pub(crate) name: String,
    pub(crate) params: Vec<ValueId>,
    pub(crate) float_params: Vec<(ValueId, u16)>,
    pub(crate) result: HirType,
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
    pub(crate) data_layout: TargetDataLayout,
    pub(crate) functions: Vec<LirFunction>,
    pub(crate) strings: Vec<(String, Vec<u8>)>,
}

pub(crate) fn lower_mir<T: Target>(module: &MirModule, target: &T) -> LirModule {
    debug_assert_eq!(module.data_layout, target.data_layout());
    let mut strings = Vec::new();
    LirModule {
        target: target.arch(),
        data_layout: module.data_layout,
        functions: module
            .functions
            .iter()
            .map(|function| lower_function(function, target.arch(), &mut strings))
            .collect(),
        strings,
    }
}

fn lower_function(
    function: &MirFunction,
    target: TargetArch,
    strings: &mut Vec<(String, Vec<u8>)>,
) -> LirFunction {
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
                        MirInstruction::ConstFloat { result, bits, .. } => {
                            LirInstruction::LoadImmediate {
                                register: *result,
                                value: *bits as i128,
                            }
                        }
                        MirInstruction::Add { result, lhs, rhs } => LirInstruction::Add {
                            destination: *result,
                            lhs: *lhs,
                            rhs: *rhs,
                        },
                        MirInstruction::FloatBinary {
                            result,
                            lhs,
                            rhs,
                            kind,
                            width,
                        } => {
                            if *width == 8 {
                                let operation = match kind {
                                    FloatBinaryKind::Add => "add",
                                    FloatBinaryKind::Subtract => "sub",
                                    FloatBinaryKind::Multiply => "mul",
                                    FloatBinaryKind::Divide => "div",
                                };
                                LirInstruction::Call {
                                    result: Some(*result),
                                    result_slice_length: None,
                                    name: format!("mimi_f8_{operation}"),
                                    args: vec![*lhs, *rhs],
                                    float_args: Vec::new(),
                                    result_float: None,
                                    variadic: false,
                                }
                            } else {
                                LirInstruction::FloatBinary {
                                    destination: *result,
                                    lhs: *lhs,
                                    rhs: *rhs,
                                    kind: *kind,
                                    width: *width,
                                }
                            }
                        }
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
                        MirInstruction::FloatNeg {
                            result,
                            value,
                            width,
                        } => {
                            if *width == 8 {
                                LirInstruction::Call {
                                    result: Some(*result),
                                    result_slice_length: None,
                                    name: "mimi_f8_neg".to_string(),
                                    args: vec![*value],
                                    float_args: Vec::new(),
                                    result_float: None,
                                    variadic: false,
                                }
                            } else {
                                LirInstruction::FloatNeg {
                                    destination: *result,
                                    value: *value,
                                    width: *width,
                                }
                            }
                        }
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
                        MirInstruction::FloatCompare {
                            result,
                            lhs,
                            rhs,
                            kind,
                            width,
                        } => {
                            if *width == 8 {
                                let operation = match kind {
                                    CompareKind::Equal => "eq",
                                    CompareKind::NotEqual => "ne",
                                    CompareKind::Less => "lt",
                                    CompareKind::LessEqual => "le",
                                    CompareKind::Greater => "gt",
                                    CompareKind::GreaterEqual => "ge",
                                };
                                LirInstruction::Call {
                                    result: Some(*result),
                                    result_slice_length: None,
                                    name: format!("mimi_f8_{operation}"),
                                    args: vec![*lhs, *rhs],
                                    float_args: Vec::new(),
                                    result_float: None,
                                    variadic: false,
                                }
                            } else {
                                LirInstruction::FloatCompare {
                                    destination: *result,
                                    lhs: *lhs,
                                    rhs: *rhs,
                                    kind: *kind,
                                    width: *width,
                                }
                            }
                        }
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
                            let mut c_bytes = bytes.clone();
                            c_bytes.push(0);
                            strings.push((symbol.clone(), c_bytes));
                            LirInstruction::LoadAddress {
                                register: *result,
                                symbol,
                                external: false,
                            }
                        }
                        MirInstruction::LoadFunctionAddress {
                            result,
                            symbol,
                            external,
                        } => LirInstruction::LoadAddress {
                            register: *result,
                            symbol: symbol.clone(),
                            external: *external,
                        },
                        MirInstruction::Call {
                            result,
                            result_slice_length,
                            name,
                            args,
                            float_args,
                            result_float,
                            variadic,
                        } => LirInstruction::Call {
                            result: *result,
                            result_slice_length: *result_slice_length,
                            name: name.clone(),
                            args: args.clone(),
                            float_args: if *variadic && target == TargetArch::Aarch64 {
                                Vec::new()
                            } else {
                                float_args.clone()
                            },
                            result_float: *result_float,
                            variadic: *variadic,
                        },
                        MirInstruction::CallIndirect {
                            result,
                            result_slice_length,
                            callee,
                            args,
                            float_args,
                            result_float,
                        } => LirInstruction::CallIndirect {
                            result: *result,
                            result_slice_length: *result_slice_length,
                            callee: *callee,
                            args: args.clone(),
                            float_args: float_args.clone(),
                            result_float: *result_float,
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
                        MirInstruction::LoadSizedIndirect {
                            result,
                            address,
                            bytes,
                            signed,
                        } => LirInstruction::LoadSizedIndirect {
                            destination: *result,
                            address: *address,
                            bytes: *bytes,
                            signed: *signed,
                        },
                        MirInstruction::StoreIndirect { address, value } => {
                            LirInstruction::StoreIndirect {
                                address: *address,
                                value: *value,
                            }
                        }
                        MirInstruction::StoreByteIndirect { address, value } => {
                            LirInstruction::StoreByteIndirect {
                                address: *address,
                                value: *value,
                            }
                        }
                        MirInstruction::StoreSizedIndirect {
                            address,
                            value,
                            bytes,
                        } => LirInstruction::StoreSizedIndirect {
                            address: *address,
                            value: *value,
                            bytes: *bytes,
                        },
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
        float_params: function.abi_float_params.clone(),
        result: function.result.clone(),
        slot_count: function.slot_count,
        blocks,
    }
}
