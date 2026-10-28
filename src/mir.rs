use crate::hir::{HirFunction, HirModule, HirType};
use crate::parser::Expression;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ValueId(pub(crate) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BlockId(pub(crate) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SlotId(pub(crate) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompareKind {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloatBinaryKind {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MirInstruction {
    ConstInt {
        result: ValueId,
        value: i128,
    },
    ConstFloat {
        result: ValueId,
        bits: u64,
        width: u16,
    },
    ConstBool {
        result: ValueId,
        value: bool,
    },
    ConstString {
        result: ValueId,
        bytes: Vec<u8>,
    },
    LoadFunctionAddress {
        result: ValueId,
        symbol: String,
    },
    Add {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    FloatBinary {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
        kind: FloatBinaryKind,
        width: u16,
    },
    Sub {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Mul {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Div {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Rem {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Neg {
        result: ValueId,
        value: ValueId,
    },
    FloatNeg {
        result: ValueId,
        value: ValueId,
        width: u16,
    },
    Not {
        result: ValueId,
        value: ValueId,
    },
    Compare {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
        kind: CompareKind,
    },
    FloatCompare {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
        kind: CompareKind,
        width: u16,
    },
    Phi {
        result: ValueId,
        incoming: Vec<(BlockId, ValueId)>,
    },
    Call {
        result: Option<ValueId>,
        result_slice_length: Option<ValueId>,
        name: String,
        args: Vec<ValueId>,
        float_args: Vec<(usize, u16)>,
        result_float: Option<u16>,
    },
    CallIndirect {
        result: Option<ValueId>,
        result_slice_length: Option<ValueId>,
        callee: ValueId,
        args: Vec<ValueId>,
        float_args: Vec<(usize, u16)>,
        result_float: Option<u16>,
    },
    LoadSlot {
        result: ValueId,
        slot: SlotId,
    },
    StoreSlot {
        slot: SlotId,
        value: ValueId,
    },
    SlotAddress {
        result: ValueId,
        slot: SlotId,
    },
    IndexAddress {
        result: ValueId,
        base: ValueId,
        index: ValueId,
        scale: u8,
    },
    LoadIndirect {
        result: ValueId,
        address: ValueId,
    },
    LoadByteIndirect {
        result: ValueId,
        address: ValueId,
    },
    LoadSizedIndirect {
        result: ValueId,
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
pub(crate) enum MirTerminator {
    Return {
        value: Option<ValueId>,
        slice_length: Option<ValueId>,
    },
    Jump(BlockId),
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    Unreachable,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MirBlock {
    pub(crate) id: BlockId,
    pub(crate) instructions: Vec<MirInstruction>,
    pub(crate) terminator: MirTerminator,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MirFunction {
    pub(crate) name: String,
    pub(crate) params: Vec<(String, HirType, ValueId)>,
    pub(crate) abi_params: Vec<ValueId>,
    pub(crate) abi_float_params: Vec<(ValueId, u16)>,
    pub(crate) result: HirType,
    pub(crate) slot_count: u32,
    pub(crate) blocks: Vec<MirBlock>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MirModule {
    pub(crate) functions: Vec<MirFunction>,
}

struct Builder {
    next_value: u32,
    next_slot: u32,
    blocks: Vec<MirBlock>,
    current: BlockId,
    variables: std::collections::HashMap<String, Variable>,
    array_lengths: std::collections::HashMap<String, u64>,
    slice_lengths: std::collections::HashMap<String, SlotId>,
    slice_scales: std::collections::HashMap<String, u8>,
    pointer_scales: std::collections::HashMap<String, u8>,
    enum_variants: std::collections::HashMap<String, EnumVariant>,
    struct_fields: std::collections::HashMap<String, Vec<(String, Option<Expression>)>>,
    struct_field_types: std::collections::HashMap<String, Vec<(String, HirType)>>,
    function_slice_params: std::collections::HashMap<String, Vec<bool>>,
    method_targets: std::collections::HashMap<(String, String), MethodTarget>,
    function_signatures: std::collections::HashMap<String, (Vec<HirType>, HirType)>,
    variable_function_signatures: std::collections::HashMap<String, (Vec<HirType>, HirType)>,
    variable_types: std::collections::HashMap<String, HirType>,
    value_types: std::collections::HashMap<ValueId, HirType>,
    slice_value_lengths: std::collections::HashMap<ValueId, ValueId>,
    expected_type_hint: Option<HirType>,
    return_type: HirType,
    variable_structs: std::collections::HashMap<String, String>,
    defer_scopes: Vec<Vec<Expression>>,
    loop_targets: Vec<LoopTargets>,
}

#[derive(Clone, Copy)]
enum Variable {
    Value(ValueId),
    Slot(SlotId),
}

#[derive(Clone)]
struct EnumVariant {
    tag: u64,
    payload: Option<HirType>,
    uses_storage: bool,
}

#[derive(Clone)]
struct MethodTarget {
    symbol: String,
    parameter_count: usize,
    has_receiver: bool,
}

#[derive(Clone, Copy)]
struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
    defer_scope_depth: usize,
}

impl Builder {
    fn value(&mut self) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
    }

    fn slot(&mut self) -> SlotId {
        let slot = SlotId(self.next_slot);
        self.next_slot += 1;
        slot
    }

    fn emit(&mut self, instruction: MirInstruction) {
        self.blocks[self.current.0 as usize]
            .instructions
            .push(instruction);
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(MirBlock {
            id,
            instructions: Vec::new(),
            terminator: MirTerminator::Unreachable,
        });
        id
    }

    fn terminate(&mut self, terminator: MirTerminator) {
        self.blocks[self.current.0 as usize].terminator = terminator;
    }

    fn expression(&mut self, expression: &Expression) -> Option<ValueId> {
        match expression {
            Expression::Identifier(name) => match self.variables.get(name).copied() {
                Some(Variable::Value(value)) => Some(value),
                Some(Variable::Slot(slot)) => {
                    let result = self.value();
                    self.emit(MirInstruction::LoadSlot { result, slot });
                    if let Some(data_type) = self.variable_types.get(name).cloned() {
                        self.value_types.insert(result, data_type);
                    }
                    if let Some(length_slot) = self.slice_lengths.get(name).copied() {
                        let length = self.value();
                        self.emit(MirInstruction::LoadSlot {
                            result: length,
                            slot: length_slot,
                        });
                        self.slice_value_lengths.insert(result, length);
                    }
                    Some(result)
                }
                None => {
                    if let Some(variant) = self.enum_variants.get(name).cloned() {
                        self.lower_enum_value(&variant, None)
                    } else {
                        self.function_signatures.get(name)?;
                        let result = self.value();
                        self.emit(MirInstruction::LoadFunctionAddress {
                            result,
                            symbol: format!("_{name}"),
                        });
                        Some(result)
                    }
                }
            },
            Expression::ArrayLiteral(values) => {
                let expected_element = match self.expected_type_hint.as_ref() {
                    Some(HirType::Array { element, .. }) | Some(HirType::Slice(element)) => {
                        Some(element.as_ref().clone())
                    }
                    _ => None,
                };
                let packed_size = expected_element
                    .as_ref()
                    .and_then(scalar_storage_size)
                    .filter(|size| *size < 8);
                let base = self.slot();
                let slot_count = if let Some(size) = packed_size {
                    (values.len() * usize::from(size)).div_ceil(8)
                } else {
                    values.len()
                };
                self.next_slot += slot_count.saturating_sub(1) as u32;
                let packed_base = if packed_size.is_some() {
                    let address = self.value();
                    self.emit(MirInstruction::SlotAddress {
                        result: address,
                        slot: base,
                    });
                    Some(address)
                } else {
                    None
                };
                for (index, expression) in values.iter().enumerate() {
                    let value = match expected_element.as_ref() {
                        Some(data_type) => self.expression_expected(expression, data_type)?,
                        None => self.expression(expression)?,
                    };
                    if let Some(base) = packed_base {
                        let index_value = self.value();
                        self.emit(MirInstruction::ConstInt {
                            result: index_value,
                            value: index as i128,
                        });
                        let address = self.value();
                        self.emit(MirInstruction::IndexAddress {
                            result: address,
                            base,
                            index: index_value,
                            scale: packed_size.expect("packed array has an element size"),
                        });
                        match packed_size.expect("packed array has an element size") {
                            1 => self.emit(MirInstruction::StoreByteIndirect { address, value }),
                            bytes => self.emit(MirInstruction::StoreSizedIndirect {
                                address,
                                value,
                                bytes,
                            }),
                        }
                    } else {
                        self.emit(MirInstruction::StoreSlot {
                            slot: SlotId(base.0 + index as u32),
                            value,
                        });
                    }
                }
                let result = if let Some(base) = packed_base {
                    base
                } else {
                    let result = self.value();
                    self.emit(MirInstruction::SlotAddress { result, slot: base });
                    result
                };
                if matches!(self.expected_type_hint.as_ref(), Some(HirType::Slice(_))) {
                    let length = self.value();
                    self.emit(MirInstruction::ConstInt {
                        result: length,
                        value: values.len() as i128,
                    });
                    self.slice_value_lengths.insert(result, length);
                }
                Some(result)
            }
            Expression::Index { object, index } => {
                let (base, scale) = self.lower_index_base(object)?;
                let index = self.expression(index)?;
                let address = self.value();
                self.emit(MirInstruction::IndexAddress {
                    result: address,
                    base,
                    index,
                    scale,
                });
                let result = self.value();
                let data_type = self.expression_type(expression);
                self.emit_load_indirect(result, address, data_type.as_ref());
                if let Some(data_type) = data_type {
                    self.value_types.insert(result, data_type);
                }
                Some(result)
            }
            Expression::AddressOf(inner) => {
                let address = match inner.as_ref() {
                    Expression::Identifier(name) => {
                        if self.variable_structs.contains_key(name) {
                            return self.expression(inner);
                        }
                        let Variable::Slot(slot) = self.variables.get(name).copied()? else {
                            return None;
                        };
                        let address = self.value();
                        self.emit(MirInstruction::SlotAddress {
                            result: address,
                            slot,
                        });
                        address
                    }
                    Expression::Index { object, index } => {
                        let (base, scale) = self.lower_index_base(object)?;
                        let index = self.expression(index)?;
                        let address = self.value();
                        self.emit(MirInstruction::IndexAddress {
                            result: address,
                            base,
                            index,
                            scale,
                        });
                        address
                    }
                    _ => return None,
                };
                Some(address)
            }
            Expression::Dereference(pointer) => {
                let address = self.expression(pointer)?;
                let result = self.value();
                let data_type = self.expression_type(expression);
                self.emit_load_indirect(result, address, data_type.as_ref());
                if let Some(data_type) = data_type {
                    self.value_types.insert(result, data_type);
                }
                Some(result)
            }
            Expression::StructLiteral { type_name, fields } => {
                self.lower_struct_literal((*type_name)?, fields)
            }
            Expression::MemberAccess { object, member } => {
                let struct_name = self.expression_struct_name(object)?;
                let fields = self.struct_fields.get(&struct_name)?;
                let field_index = fields.iter().position(|(name, _)| name == member)?;
                let base = self.expression(object)?;
                let index = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: index,
                    value: field_index as i128,
                });
                let address = self.value();
                self.emit(MirInstruction::IndexAddress {
                    result: address,
                    base,
                    index,
                    scale: 8,
                });
                let result = self.value();
                self.emit(MirInstruction::LoadIndirect { result, address });
                if let Some(data_type) = self
                    .struct_field_types
                    .get(&struct_name)
                    .and_then(|fields| fields.iter().find(|(name, _)| name == member))
                    .map(|(_, data_type)| data_type.clone())
                {
                    self.value_types.insert(result, data_type);
                }
                Some(result)
            }
            Expression::StaticAccess { type_name, member } => {
                let symbol = self
                    .method_targets
                    .get(&(type_name.clone(), member.clone()))?
                    .symbol
                    .clone();
                let result = self.value();
                self.emit(MirInstruction::LoadFunctionAddress {
                    result,
                    symbol: format!("_{symbol}"),
                });
                if let Some((parameters, result_type)) =
                    self.function_signature_for_expression(expression)
                {
                    self.value_types.insert(
                        result,
                        HirType::Function {
                            params: parameters,
                            result: Box::new(result_type),
                        },
                    );
                }
                Some(result)
            }
            Expression::LiteralNumber(text) => {
                let result = self.value();
                let expected_float = self.expected_type_hint.as_ref().and_then(float_width);
                if let Some(width) = expected_float.or_else(|| text.contains('.').then_some(64)) {
                    let value = text.parse::<f64>().ok()?;
                    let bits = match width {
                        32 => (value as f32).to_bits() as u64,
                        64 => value.to_bits(),
                        _ => value.to_bits(),
                    };
                    self.emit(MirInstruction::ConstFloat {
                        result,
                        bits,
                        width,
                    });
                    self.value_types
                        .insert(result, HirType::Float { bits: width });
                } else {
                    let value = text.parse().ok()?;
                    self.emit(MirInstruction::ConstInt { result, value });
                    self.value_types.insert(
                        result,
                        HirType::Integer {
                            bits: 32,
                            signed: true,
                        },
                    );
                }
                Some(result)
            }
            Expression::LiteralBool(value) => {
                let result = self.value();
                self.emit(MirInstruction::ConstBool {
                    result,
                    value: *value,
                });
                self.value_types.insert(result, HirType::Bool);
                Some(result)
            }
            Expression::LiteralString(value) => {
                let result = self.value();
                self.emit(MirInstruction::ConstString {
                    result,
                    bytes: value.as_bytes().to_vec(),
                });
                self.value_types.insert(
                    result,
                    HirType::Slice(Box::new(HirType::Integer {
                        bits: 8,
                        signed: false,
                    })),
                );
                let length = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: length,
                    value: value.len() as i128,
                });
                self.slice_value_lengths.insert(result, length);
                Some(result)
            }
            Expression::Slice { .. } => {
                let expected = self
                    .expected_type_hint
                    .clone()
                    .or_else(|| self.expression_type(expression))?;
                let (base, length, _) = self.lower_array_view_expected(expression, &expected)?;
                self.value_types.insert(base, expected);
                self.slice_value_lengths.insert(base, length);
                Some(base)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if let Expression::LiteralBool(value) = condition.as_ref() {
                    return self.expression(if *value { then_branch } else { else_branch });
                }
                let condition = self.expression(condition)?;
                let then_block = self.new_block();
                let else_block = self.new_block();
                let merge_block = self.new_block();
                self.terminate(MirTerminator::Branch {
                    condition,
                    then_block,
                    else_block,
                });
                self.current = then_block;
                let then_value = self.expression(then_branch);
                let then_end = self.current;
                let then_live = self.is_open(then_end);
                if then_live {
                    self.terminate(MirTerminator::Jump(merge_block));
                }
                self.current = else_block;
                let else_value = self.expression(else_branch);
                let else_end = self.current;
                let else_live = self.is_open(else_end);
                if else_live {
                    self.terminate(MirTerminator::Jump(merge_block));
                }
                if !then_live && !else_live {
                    self.current = then_end;
                    return None;
                }
                self.current = merge_block;
                if !then_live {
                    return else_value;
                }
                if !else_live {
                    return then_value;
                }
                if then_value.is_none() && else_value.is_none() {
                    return None;
                }
                let (Some(then_value), Some(else_value)) = (then_value, else_value) else {
                    return None;
                };
                let result = self.value();
                self.emit(MirInstruction::Phi {
                    result,
                    incoming: vec![(then_end, then_value), (else_end, else_value)],
                });
                if let (Some(then_length), Some(else_length)) = (
                    self.slice_value_lengths.get(&then_value).copied(),
                    self.slice_value_lengths.get(&else_value).copied(),
                ) {
                    let length = self.value();
                    self.emit(MirInstruction::Phi {
                        result: length,
                        incoming: vec![(then_end, then_length), (else_end, else_length)],
                    });
                    self.slice_value_lengths.insert(result, length);
                }
                Some(result)
            }
            Expression::Match { value, arms } => self.lower_match(value, arms),
            Expression::Call { name, arguments } => {
                if let Some((parameter_types, return_type)) =
                    self.variable_function_signatures.get(name).cloned()
                {
                    let callee = self.expression(&Expression::Identifier(name.clone()))?;
                    let (args, float_args) =
                        self.lower_function_value_arguments(arguments, &parameter_types)?;
                    let result = (return_type != HirType::Void).then(|| self.value());
                    let result_slice_length =
                        matches!(return_type, HirType::Slice(_)).then(|| self.value());
                    if let Some(result) = result {
                        self.value_types.insert(result, return_type.clone());
                    }
                    if let (Some(result), Some(length)) = (result, result_slice_length) {
                        self.slice_value_lengths.insert(result, length);
                    }
                    let result_float = float_width(&return_type);
                    self.emit(MirInstruction::CallIndirect {
                        result,
                        result_slice_length,
                        callee,
                        args,
                        float_args,
                        result_float,
                    });
                    return result;
                }
                if !self.function_slice_params.contains_key(name)
                    && let Some(variant) = self.enum_variants.get(name).cloned()
                {
                    if arguments.len() != usize::from(variant.payload.is_some()) {
                        return None;
                    }
                    return self.lower_enum_value(&variant, arguments.first());
                }
                let (args, float_args) = self.lower_call_arguments(name, arguments)?;
                let return_type = self
                    .function_signatures
                    .get(name)
                    .map(|(_, result)| result.clone());
                let result = return_type
                    .as_ref()
                    .is_none_or(|ty| *ty != HirType::Void)
                    .then(|| self.value());
                let result_slice_length = return_type
                    .as_ref()
                    .is_some_and(|ty| matches!(ty, HirType::Slice(_)))
                    .then(|| self.value());
                if let (Some(result), Some(return_type)) = (result, return_type.as_ref()) {
                    self.value_types.insert(result, return_type.clone());
                }
                if let (Some(result), Some(length)) = (result, result_slice_length) {
                    self.slice_value_lengths.insert(result, length);
                }
                self.emit(MirInstruction::Call {
                    result,
                    result_slice_length,
                    name: name.clone(),
                    args,
                    float_args,
                    result_float: return_type.as_ref().and_then(float_width),
                });
                result
            }
            Expression::CallValue { callee, arguments } => {
                if let Expression::Identifier(name) = callee.as_ref()
                    && let Some((parameter_types, return_type)) = self
                        .variable_function_signatures
                        .get(name)
                        .cloned()
                        .or_else(|| self.function_signatures.get(name).cloned())
                {
                    let callee = self.expression(callee)?;
                    let (args, float_args) =
                        self.lower_function_value_arguments(arguments, &parameter_types)?;
                    let result = (return_type != HirType::Void).then(|| self.value());
                    let result_slice_length =
                        matches!(return_type, HirType::Slice(_)).then(|| self.value());
                    if let Some(result) = result {
                        self.value_types.insert(result, return_type.clone());
                    }
                    if let (Some(result), Some(length)) = (result, result_slice_length) {
                        self.slice_value_lengths.insert(result, length);
                    }
                    let result_float = float_width(&return_type);
                    self.emit(MirInstruction::CallIndirect {
                        result,
                        result_slice_length,
                        callee,
                        args,
                        float_args,
                        result_float,
                    });
                    return result;
                }
                let (target, receiver) = match callee.as_ref() {
                    Expression::StaticAccess { type_name, member } => (
                        self.method_targets
                            .get(&(type_name.clone(), member.clone()))?
                            .clone(),
                        None,
                    ),
                    Expression::MemberAccess { object, member } => {
                        let struct_name = self.expression_struct_name(object)?;
                        let target = self
                            .method_targets
                            .get(&(struct_name, member.clone()))?
                            .clone();
                        let receiver = if target.has_receiver
                            && arguments.len() + 1 == target.parameter_count
                        {
                            Some(object.as_ref())
                        } else {
                            None
                        };
                        (target, receiver)
                    }
                    _ => return None,
                };
                let mut call_arguments = Vec::with_capacity(arguments.len() + 1);
                if let Some(receiver) = receiver {
                    call_arguments.push(receiver.clone());
                }
                call_arguments.extend(arguments.iter().cloned());
                if call_arguments.len() != target.parameter_count {
                    return None;
                }
                let (args, float_args) =
                    self.lower_call_arguments(&target.symbol, &call_arguments)?;
                let return_type = self
                    .function_signatures
                    .get(&target.symbol)
                    .map(|(_, result)| result.clone());
                let result = return_type
                    .as_ref()
                    .is_none_or(|ty| *ty != HirType::Void)
                    .then(|| self.value());
                let result_slice_length = return_type
                    .as_ref()
                    .is_some_and(|ty| matches!(ty, HirType::Slice(_)))
                    .then(|| self.value());
                if let (Some(result), Some(return_type)) = (result, return_type.as_ref()) {
                    self.value_types.insert(result, return_type.clone());
                }
                if let (Some(result), Some(length)) = (result, result_slice_length) {
                    self.slice_value_lengths.insert(result, length);
                }
                self.emit(MirInstruction::Call {
                    result,
                    result_slice_length,
                    name: target.symbol,
                    args,
                    float_args,
                    result_float: return_type.as_ref().and_then(float_width),
                });
                result
            }
            Expression::Add(lhs, rhs) => self.binary(lhs, rhs, 0),
            Expression::Subtract(lhs, rhs) => self.binary(lhs, rhs, 1),
            Expression::Multiply(lhs, rhs) => self.binary(lhs, rhs, 2),
            Expression::Divide(lhs, rhs) => self.binary(lhs, rhs, 3),
            Expression::Remainder(lhs, rhs) => self.binary(lhs, rhs, 4),
            Expression::Negate(value) => {
                let expected = self
                    .expected_type_hint
                    .clone()
                    .filter(|ty| float_width(ty).is_some())
                    .or_else(|| self.expression_type(value));
                let value = if let Some(data_type) = expected.as_ref() {
                    self.expression_expected(value, data_type)
                } else {
                    self.expression(value)
                }?;
                let result = self.value();
                if let Some(width) = self.value_type(value).and_then(|ty| float_width(&ty)) {
                    self.emit(MirInstruction::FloatNeg {
                        result,
                        value,
                        width,
                    });
                    self.value_types
                        .insert(result, HirType::Float { bits: width });
                } else {
                    self.emit(MirInstruction::Neg { result, value });
                    if let Some(data_type) = self.value_type(value) {
                        self.value_types.insert(result, data_type);
                    }
                }
                Some(result)
            }
            Expression::Not(value) => {
                let value = self.expression(value)?;
                let result = self.value();
                self.emit(MirInstruction::Not { result, value });
                Some(result)
            }
            Expression::And(lhs, rhs) => self.logical(lhs, rhs, true),
            Expression::Or(lhs, rhs) => self.logical(lhs, rhs, false),
            Expression::Equal(lhs, rhs)
            | Expression::NotEqual(lhs, rhs)
            | Expression::LessThan(lhs, rhs)
            | Expression::LessThanOrEqual(lhs, rhs)
            | Expression::GreaterThan(lhs, rhs)
            | Expression::GreaterThanOrEqual(lhs, rhs) => {
                let operand_type = self.float_operation_type(lhs, rhs);
                let lhs = if let Some(data_type) = operand_type.as_ref() {
                    self.expression_expected(lhs, data_type)
                } else {
                    self.expression(lhs)
                }?;
                let rhs = if let Some(data_type) = operand_type.as_ref() {
                    self.expression_expected(rhs, data_type)
                } else {
                    self.expression(rhs)
                }?;
                let kind = match expression {
                    Expression::Equal(_, _) => CompareKind::Equal,
                    Expression::NotEqual(_, _) => CompareKind::NotEqual,
                    Expression::LessThan(_, _) => CompareKind::Less,
                    Expression::LessThanOrEqual(_, _) => CompareKind::LessEqual,
                    Expression::GreaterThan(_, _) => CompareKind::Greater,
                    _ => CompareKind::GreaterEqual,
                };
                let result = self.value();
                if let Some(width) = operand_type.as_ref().and_then(float_width) {
                    self.emit(MirInstruction::FloatCompare {
                        result,
                        lhs,
                        rhs,
                        kind,
                        width,
                    });
                } else {
                    self.emit(MirInstruction::Compare {
                        result,
                        lhs,
                        rhs,
                        kind,
                    });
                }
                self.value_types.insert(result, HirType::Bool);
                Some(result)
            }
            Expression::Block(items) => {
                let outer_variables = self.variables.clone();
                let outer_variable_types = self.variable_types.clone();
                let outer_array_lengths = self.array_lengths.clone();
                let outer_slice_lengths = self.slice_lengths.clone();
                let outer_slice_scales = self.slice_scales.clone();
                let outer_pointer_scales = self.pointer_scales.clone();
                let outer_function_signatures = self.variable_function_signatures.clone();
                let outer_variable_structs = self.variable_structs.clone();
                self.defer_scopes.push(Vec::new());
                let result = (|| {
                    let mut result = None;
                    for item in items {
                        match item {
                            crate::parser::ASTNode::VariableDefinition {
                                name,
                                data_type,
                                value,
                            } => {
                                let struct_name = match data_type {
                                    Some(crate::parser::DataType::Struct(name))
                                        if self.struct_fields.contains_key(*name) =>
                                    {
                                        Some((*name).to_string())
                                    }
                                    _ => self.expression_struct_name(value),
                                };
                                let declared_slice =
                                    matches!(data_type, Some(crate::parser::DataType::Slice(_)));
                                let array_length = match data_type {
                                    Some(crate::parser::DataType::Array(length, _)) => {
                                        Some(*length)
                                    }
                                    Some(crate::parser::DataType::Slice(_)) => None,
                                    _ => match value {
                                        Expression::ArrayLiteral(values) => {
                                            Some(values.len() as u64)
                                        }
                                        _ => None,
                                    },
                                };
                                let value_type = data_type
                                    .map(Into::into)
                                    .or_else(|| self.expression_type(value));
                                let is_slice = declared_slice
                                    || self.is_slice_value(value)
                                    || matches!(&value_type, Some(HirType::Slice(_)));
                                let inferred_pointer_scale = match value {
                                    Expression::AddressOf(inner) => {
                                        Some(self.address_target_scale(inner))
                                    }
                                    _ => None,
                                };
                                let function_signature = match data_type {
                                    Some(crate::parser::DataType::Function(params, result)) => {
                                        Some((
                                            params.iter().map(|ty| (*ty).into()).collect(),
                                            (**result).into(),
                                        ))
                                    }
                                    _ => self.function_signature_for_expression(value),
                                };
                                let (value, slice_view) = if is_slice {
                                    let view = match value_type.as_ref() {
                                        Some(data_type) => {
                                            self.lower_array_view_expected(value, data_type)
                                        }
                                        None => self.lower_array_view(value),
                                    }?;
                                    let (base, length, scale) = view;
                                    (base, Some((length, scale)))
                                } else {
                                    let value = match (value, struct_name.as_deref()) {
                                        (
                                            Expression::StructLiteral {
                                                type_name: None,
                                                fields,
                                            },
                                            Some(struct_name),
                                        ) => self.lower_struct_literal(struct_name, fields)?,
                                        _ => match value_type.as_ref() {
                                            Some(data_type) => {
                                                self.expression_expected(value, data_type)?
                                            }
                                            None => self.expression(value)?,
                                        },
                                    };
                                    (value, None)
                                };
                                let slot = self.slot();
                                self.emit(MirInstruction::StoreSlot { slot, value });
                                self.variables.insert(name.clone(), Variable::Slot(slot));
                                if let Some(data_type) = value_type {
                                    self.variable_types.insert(name.clone(), data_type);
                                }
                                if let Some((length, scale)) = slice_view {
                                    let length_slot = self.slot();
                                    self.emit(MirInstruction::StoreSlot {
                                        slot: length_slot,
                                        value: length,
                                    });
                                    self.slice_lengths.insert(name.clone(), length_slot);
                                    self.slice_scales.insert(name.clone(), scale);
                                } else {
                                    self.slice_lengths.remove(name);
                                    self.slice_scales.remove(name);
                                }
                                let pointer_scale = match data_type {
                                    Some(crate::parser::DataType::Pointer(element)) => {
                                        Some(pointer_scale_from_data_type(element))
                                    }
                                    _ => inferred_pointer_scale,
                                };
                                if let Some(scale) = pointer_scale {
                                    self.pointer_scales.insert(name.clone(), scale);
                                } else {
                                    self.pointer_scales.remove(name);
                                }
                                if let Some(signature) = function_signature {
                                    self.variable_function_signatures
                                        .insert(name.clone(), signature);
                                } else {
                                    self.variable_function_signatures.remove(name);
                                }
                                if let Some(length) = array_length {
                                    self.array_lengths.insert(name.clone(), length);
                                } else {
                                    self.array_lengths.remove(name);
                                }
                                if let Some(struct_name) = struct_name {
                                    self.variable_structs.insert(name.clone(), struct_name);
                                } else {
                                    self.variable_structs.remove(name);
                                }
                            }
                            crate::parser::ASTNode::CompileTimeConstant { name, value, .. } => {
                                let struct_name = self.expression_struct_name(value);
                                let function_signature =
                                    self.function_signature_for_expression(value);
                                let value_type = self.expression_type(value);
                                let value = match value_type.as_ref() {
                                    Some(data_type) => {
                                        self.expression_expected(value, data_type)?
                                    }
                                    None => self.expression(value)?,
                                };
                                self.variables.insert(name.clone(), Variable::Value(value));
                                if let Some(data_type) = value_type {
                                    self.variable_types.insert(name.clone(), data_type);
                                }
                                if let Some(signature) = function_signature {
                                    self.variable_function_signatures
                                        .insert(name.clone(), signature);
                                } else {
                                    self.variable_function_signatures.remove(name);
                                }
                                if let Some(struct_name) = struct_name {
                                    self.variable_structs.insert(name.clone(), struct_name);
                                }
                            }
                            crate::parser::ASTNode::Assignment { name, value } => {
                                let struct_name = self.expression_struct_name(value);
                                let function_signature =
                                    self.function_signature_for_expression(value);
                                let expected_type = self.variable_types.get(name).cloned();
                                let slice_length_slot = self.slice_lengths.get(name).copied();
                                let slice_assignment =
                                    slice_length_slot.is_some() || self.is_slice_value(value);
                                let (value, slice_view) = if slice_assignment {
                                    let (base, length, scale) = self.lower_array_view(value)?;
                                    (base, Some((length, scale)))
                                } else {
                                    let value = match expected_type.as_ref() {
                                        Some(data_type) => {
                                            self.expression_expected(value, data_type)?
                                        }
                                        None => self.expression(value)?,
                                    };
                                    (value, None)
                                };
                                match self.variables.get(name).copied() {
                                    Some(Variable::Slot(slot)) => {
                                        self.emit(MirInstruction::StoreSlot { slot, value });
                                    }
                                    _ => {
                                        self.variables.insert(name.clone(), Variable::Value(value));
                                    }
                                }
                                if let Some((length, scale)) = slice_view {
                                    let length_slot = if let Some(slot) = slice_length_slot {
                                        slot
                                    } else {
                                        let slot = self.slot();
                                        self.slice_lengths.insert(name.clone(), slot);
                                        slot
                                    };
                                    self.emit(MirInstruction::StoreSlot {
                                        slot: length_slot,
                                        value: length,
                                    });
                                    self.slice_scales.insert(name.clone(), scale);
                                }
                                if let Some(struct_name) = struct_name {
                                    self.variable_structs.insert(name.clone(), struct_name);
                                }
                                if let Some(signature) = function_signature {
                                    self.variable_function_signatures
                                        .insert(name.clone(), signature);
                                }
                            }
                            crate::parser::ASTNode::IndexAssignment {
                                object,
                                index,
                                value,
                            } => {
                                let expected_type = self.expression_type(&Expression::Index {
                                    object: Box::new(object.clone()),
                                    index: Box::new(index.clone()),
                                });
                                let base = self.expression(object)?;
                                let index = self.expression(index)?;
                                let value = match expected_type.as_ref() {
                                    Some(data_type) => {
                                        self.expression_expected(value, data_type)?
                                    }
                                    None => self.expression(value)?,
                                };
                                let address = self.value();
                                let scale = self.index_scale(object);
                                self.emit(MirInstruction::IndexAddress {
                                    result: address,
                                    base,
                                    index,
                                    scale,
                                });
                                self.emit_store_indirect(address, value, expected_type.as_ref());
                            }
                            crate::parser::ASTNode::PointerAssignment { pointer, value } => {
                                let address = self.expression(pointer)?;
                                let expected_type = match self.expression_type(pointer) {
                                    Some(HirType::Pointer(inner)) => Some(*inner),
                                    _ => None,
                                };
                                let value = match expected_type.as_ref() {
                                    Some(data_type) => {
                                        self.expression_expected(value, data_type)?
                                    }
                                    None => self.expression(value)?,
                                };
                                self.emit_store_indirect(address, value, expected_type.as_ref());
                            }
                            crate::parser::ASTNode::MemberAssignment {
                                object,
                                member,
                                value,
                            } => {
                                let struct_name = self.expression_struct_name(object)?;
                                let fields = self.struct_fields.get(&struct_name)?;
                                let field_index =
                                    fields.iter().position(|(name, _)| name == member)?;
                                let field_type = self
                                    .struct_field_types
                                    .get(&struct_name)?
                                    .iter()
                                    .find(|(name, _)| name == member)?
                                    .1
                                    .clone();
                                let base = self.expression(object)?;
                                let index = self.value();
                                self.emit(MirInstruction::ConstInt {
                                    result: index,
                                    value: field_index as i128,
                                });
                                let address = self.value();
                                self.emit(MirInstruction::IndexAddress {
                                    result: address,
                                    base,
                                    index,
                                    scale: 8,
                                });
                                let value = self.expression_expected(value, &field_type)?;
                                self.emit(MirInstruction::StoreIndirect { address, value });
                            }
                            crate::parser::ASTNode::While { condition, body } => {
                                self.lower_while(condition, body, false)?;
                            }
                            crate::parser::ASTNode::DoWhile { body, condition } => {
                                self.lower_while(condition, body, true)?;
                            }
                            crate::parser::ASTNode::Loop { body } => {
                                self.lower_loop(body);
                            }
                            crate::parser::ASTNode::If {
                                condition,
                                then_branch,
                                else_branch,
                            } => {
                                let expression = Expression::If {
                                    condition: Box::new(condition.clone()),
                                    then_branch: Box::new(then_branch.clone()),
                                    else_branch: Box::new(
                                        else_branch
                                            .clone()
                                            .unwrap_or_else(|| Expression::Block(Vec::new())),
                                    ),
                                };
                                self.expression(&expression);
                            }
                            crate::parser::ASTNode::IfLet {
                                variant,
                                binding,
                                value,
                                then_branch,
                                else_branch,
                            } => {
                                self.lower_if_let(
                                    variant,
                                    binding.as_deref(),
                                    value,
                                    then_branch,
                                    else_branch.as_ref(),
                                )?;
                            }
                            crate::parser::ASTNode::For {
                                binding,
                                iterable,
                                body,
                            } => {
                                self.lower_for(binding, iterable, body)?;
                            }
                            crate::parser::ASTNode::Defer { body } => {
                                self.defer_scopes.last_mut()?.push(body.clone());
                            }
                            crate::parser::ASTNode::Expression(expression) => {
                                result = self.expression(expression);
                            }
                            crate::parser::ASTNode::Return(value) => {
                                let value = if let Some(expression) = value {
                                    let return_type = self.return_type.clone();
                                    self.expression_expected(expression, &return_type)
                                } else {
                                    None
                                };
                                self.run_all_defers();
                                let slice_length = value.and_then(|value| {
                                    matches!(&self.return_type, HirType::Slice(_))
                                        .then(|| self.slice_value_lengths.get(&value).copied())
                                        .flatten()
                                });
                                self.terminate(MirTerminator::Return {
                                    value,
                                    slice_length,
                                });
                                break;
                            }
                            crate::parser::ASTNode::Break => {
                                let targets = *self.loop_targets.last()?;
                                self.run_loop_defers(targets.defer_scope_depth);
                                self.terminate(MirTerminator::Jump(targets.break_block));
                                break;
                            }
                            crate::parser::ASTNode::Continue => {
                                let targets = *self.loop_targets.last()?;
                                self.run_loop_defers(targets.defer_scope_depth);
                                self.terminate(MirTerminator::Jump(targets.continue_block));
                                break;
                            }
                            _ => {}
                        }
                        if !self.is_open(self.current) {
                            break;
                        }
                    }
                    Some(result).flatten()
                })();
                if self.is_open(self.current) {
                    self.run_scope_defers();
                }
                self.defer_scopes.pop();
                self.variables = outer_variables;
                self.variable_types = outer_variable_types;
                self.array_lengths = outer_array_lengths;
                self.slice_lengths = outer_slice_lengths;
                self.slice_scales = outer_slice_scales;
                self.pointer_scales = outer_pointer_scales;
                self.variable_function_signatures = outer_function_signatures;
                self.variable_structs = outer_variable_structs;
                result
            }
            _ => None,
        }
    }

    fn lower_call_arguments(
        &mut self,
        name: &str,
        arguments: &[Expression],
    ) -> Option<(Vec<ValueId>, Vec<(usize, u16)>)> {
        let parameter_types = self
            .function_signatures
            .get(name)
            .map(|(parameters, _)| parameters.clone());
        let mut values = Vec::new();
        let mut float_args = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let parameter_type = parameter_types
                .as_ref()
                .and_then(|parameters| parameters.get(index));
            if matches!(parameter_type, Some(HirType::Slice(_))) {
                let (base, length, _) = self.lower_array_view_expected(
                    argument,
                    parameter_type.expect("checked slice parameter"),
                )?;
                values.extend([base, length]);
                continue;
            }
            let expected_struct = match parameter_type {
                Some(HirType::Named(name)) if self.struct_fields.contains_key(name) => {
                    Some(name.as_str())
                }
                _ => None,
            };
            let value = match (argument, expected_struct) {
                (
                    Expression::StructLiteral {
                        type_name: None,
                        fields,
                    },
                    Some(struct_name),
                ) => self.lower_struct_literal(struct_name, fields)?,
                _ => match parameter_type {
                    Some(data_type) => self.expression_expected(argument, data_type)?,
                    None => self.expression(argument)?,
                },
            };
            if let Some(width) = parameter_type.and_then(float_width) {
                float_args.push((values.len(), width));
            }
            values.push(value);
        }
        Some((values, float_args))
    }

    fn lower_function_value_arguments(
        &mut self,
        arguments: &[Expression],
        parameter_types: &[HirType],
    ) -> Option<(Vec<ValueId>, Vec<(usize, u16)>)> {
        if arguments.len() != parameter_types.len() {
            return None;
        }
        let mut values = Vec::new();
        let mut float_args = Vec::new();
        for (argument, parameter_type) in arguments.iter().zip(parameter_types) {
            match parameter_type {
                HirType::Slice(_) => {
                    let (base, length, _) =
                        self.lower_array_view_expected(argument, parameter_type)?;
                    values.push(base);
                    values.push(length);
                }
                HirType::Named(name) if self.struct_fields.contains_key(name) => {
                    let value = match argument {
                        Expression::StructLiteral {
                            type_name: None,
                            fields,
                        } => self.lower_struct_literal(name, fields)?,
                        _ => self.expression_expected(argument, parameter_type)?,
                    };
                    values.push(value);
                }
                _ => {
                    if let Some(width) = float_width(parameter_type) {
                        float_args.push((values.len(), width));
                    }
                    values.push(self.expression_expected(argument, parameter_type)?);
                }
            }
        }
        Some((values, float_args))
    }

    fn binary(&mut self, lhs: &Expression, rhs: &Expression, kind: u8) -> Option<ValueId> {
        if matches!(kind, 0 | 1) {
            let lhs_type = self.expression_type(lhs);
            let rhs_type = self.expression_type(rhs);
            let pointer_offset = match (kind, lhs_type, rhs_type) {
                (0, Some(HirType::Pointer(element)), _) => Some((lhs, rhs, *element, false)),
                (0, _, Some(HirType::Pointer(element))) => Some((rhs, lhs, *element, false)),
                (1, Some(HirType::Pointer(element)), _) => Some((lhs, rhs, *element, true)),
                _ => None,
            };
            if let Some((pointer, offset, element_type, subtract)) = pointer_offset {
                let pointer = self.expression(pointer)?;
                let mut offset = self.expression(offset)?;
                if subtract {
                    let negated = self.value();
                    self.emit(MirInstruction::Neg {
                        result: negated,
                        value: offset,
                    });
                    offset = negated;
                }
                let result = self.value();
                self.emit(MirInstruction::IndexAddress {
                    result,
                    base: pointer,
                    index: offset,
                    scale: pointer_scale_from_hir_type(&element_type),
                });
                self.value_types
                    .insert(result, HirType::Pointer(Box::new(element_type)));
                return Some(result);
            }
        }
        let operation_type = self.float_operation_type(lhs, rhs);
        let lhs = if let Some(data_type) = operation_type.as_ref() {
            self.expression_expected(lhs, data_type)
        } else {
            self.expression(lhs)
        }?;
        let rhs = if let Some(data_type) = operation_type.as_ref() {
            self.expression_expected(rhs, data_type)
        } else {
            self.expression(rhs)
        }?;
        let result = self.value();
        if let Some(width) = operation_type.as_ref().and_then(float_width) {
            let kind = match kind {
                0 => FloatBinaryKind::Add,
                1 => FloatBinaryKind::Subtract,
                2 => FloatBinaryKind::Multiply,
                3 => FloatBinaryKind::Divide,
                _ => return None,
            };
            self.emit(MirInstruction::FloatBinary {
                result,
                lhs,
                rhs,
                kind,
                width,
            });
            self.value_types
                .insert(result, HirType::Float { bits: width });
            return Some(result);
        }
        let instruction = match kind {
            0 => MirInstruction::Add { result, lhs, rhs },
            1 => MirInstruction::Sub { result, lhs, rhs },
            2 => MirInstruction::Mul { result, lhs, rhs },
            3 => MirInstruction::Div { result, lhs, rhs },
            _ => MirInstruction::Rem { result, lhs, rhs },
        };
        self.emit(instruction);
        if let Some(data_type) = self.value_type(lhs) {
            self.value_types.insert(result, data_type);
        }
        Some(result)
    }

    fn expression_expected(
        &mut self,
        expression: &Expression,
        expected: &HirType,
    ) -> Option<ValueId> {
        let previous = self.expected_type_hint.replace(expected.clone());
        let result = self.expression(expression);
        self.expected_type_hint = previous;
        result
    }

    fn lower_array_view_expected(
        &mut self,
        expression: &Expression,
        expected: &HirType,
    ) -> Option<(ValueId, ValueId, u8)> {
        let previous = self.expected_type_hint.replace(expected.clone());
        let result = self.lower_array_view(expression);
        self.expected_type_hint = previous;
        result
    }

    fn value_type(&self, value: ValueId) -> Option<HirType> {
        self.value_types.get(&value).cloned()
    }

    fn emit_load_indirect(
        &mut self,
        result: ValueId,
        address: ValueId,
        data_type: Option<&HirType>,
    ) {
        let bytes = data_type.and_then(scalar_storage_size);
        let signed = matches!(data_type, Some(HirType::Integer { signed: true, .. }));
        match bytes {
            Some(1) if !signed => self.emit(MirInstruction::LoadByteIndirect { result, address }),
            Some(bytes @ (1 | 2 | 4)) => self.emit(MirInstruction::LoadSizedIndirect {
                result,
                address,
                bytes,
                signed,
            }),
            _ => self.emit(MirInstruction::LoadIndirect { result, address }),
        }
    }

    fn emit_store_indirect(
        &mut self,
        address: ValueId,
        value: ValueId,
        data_type: Option<&HirType>,
    ) {
        match data_type.and_then(scalar_storage_size) {
            Some(1) => self.emit(MirInstruction::StoreByteIndirect { address, value }),
            Some(bytes @ (2 | 4)) => self.emit(MirInstruction::StoreSizedIndirect {
                address,
                value,
                bytes,
            }),
            _ => self.emit(MirInstruction::StoreIndirect { address, value }),
        }
    }

    fn expression_type(&self, expression: &Expression) -> Option<HirType> {
        match expression {
            Expression::Identifier(name) => self.variable_types.get(name).cloned().or_else(|| {
                self.function_signature_for_expression(expression)
                    .map(|(params, result)| HirType::Function {
                        params,
                        result: Box::new(result),
                    })
            }),
            Expression::StaticAccess { .. } => self
                .function_signature_for_expression(expression)
                .map(|(params, result)| HirType::Function {
                    params,
                    result: Box::new(result),
                }),
            Expression::LiteralNumber(text) if text.contains('.') => {
                Some(HirType::Float { bits: 64 })
            }
            Expression::LiteralNumber(_) => Some(HirType::Integer {
                bits: 32,
                signed: true,
            }),
            Expression::LiteralBool(_) => Some(HirType::Bool),
            Expression::LiteralString(_) => Some(HirType::Slice(Box::new(HirType::Integer {
                bits: 8,
                signed: false,
            }))),
            Expression::Slice { object, .. } => match self.expression_type(object) {
                Some(HirType::Array { element, .. }) | Some(HirType::Slice(element)) => {
                    Some(HirType::Slice(element))
                }
                _ => None,
            },
            Expression::AddressOf(inner) => self
                .expression_type(inner)
                .map(|inner| HirType::Pointer(Box::new(inner))),
            Expression::Dereference(pointer) => match self.expression_type(pointer) {
                Some(HirType::Pointer(inner)) => Some(*inner),
                _ => None,
            },
            Expression::MemberAccess { object, member } => {
                let struct_name = self.expression_struct_name(object)?;
                self.struct_field_types
                    .get(&struct_name)?
                    .iter()
                    .find(|(name, _)| name == member)
                    .map(|(_, data_type)| data_type.clone())
            }
            Expression::Index { object, .. } => match self.expression_type(object) {
                Some(HirType::Array { element, .. }) | Some(HirType::Slice(element)) => {
                    Some(*element)
                }
                Some(HirType::Pointer(element)) => Some(*element),
                _ => None,
            },
            Expression::ArrayLiteral(values) => values.first().and_then(|value| {
                self.expression_type(value).map(|element| HirType::Array {
                    length: values.len() as u64,
                    element: Box::new(element),
                })
            }),
            Expression::Call { name, .. } => self
                .variable_function_signatures
                .get(name)
                .map(|(_, result)| result.clone())
                .or_else(|| {
                    self.function_signatures
                        .get(name)
                        .map(|(_, result)| result.clone())
                }),
            Expression::CallValue { callee, .. } => self
                .function_signature_for_expression(callee)
                .map(|(_, result)| result),
            Expression::Negate(inner) => self.expression_type(inner),
            Expression::Add(lhs, rhs)
            | Expression::Subtract(lhs, rhs)
            | Expression::Multiply(lhs, rhs)
            | Expression::Divide(lhs, rhs)
            | Expression::Remainder(lhs, rhs) => self
                .float_operation_type(lhs, rhs)
                .or_else(|| self.expression_type(lhs))
                .or_else(|| self.expression_type(rhs)),
            Expression::If {
                then_branch,
                else_branch,
                ..
            } => self
                .expression_type(then_branch)
                .or_else(|| self.expression_type(else_branch)),
            Expression::Block(items) => items.iter().rev().find_map(|item| match item {
                crate::parser::ASTNode::Expression(expression) => self.expression_type(expression),
                _ => None,
            }),
            _ => None,
        }
    }

    fn float_operation_type(&self, lhs: &Expression, rhs: &Expression) -> Option<HirType> {
        let is_literal = |expression: &Expression| {
            matches!(expression, Expression::LiteralNumber(_))
                || matches!(expression, Expression::Negate(inner) if matches!(inner.as_ref(), Expression::LiteralNumber(_)))
        };
        let lhs_type = (!is_literal(lhs))
            .then(|| self.expression_type(lhs))
            .flatten();
        let rhs_type = (!is_literal(rhs))
            .then(|| self.expression_type(rhs))
            .flatten();
        lhs_type
            .filter(|ty| float_width(ty).is_some())
            .or_else(|| rhs_type.filter(|ty| float_width(ty).is_some()))
            .or_else(|| {
                self.expected_type_hint
                    .clone()
                    .filter(|ty| float_width(ty).is_some())
            })
            .or_else(|| {
                self.expression_type(lhs)
                    .filter(|ty| float_width(ty).is_some())
            })
            .or_else(|| {
                self.expression_type(rhs)
                    .filter(|ty| float_width(ty).is_some())
            })
    }

    fn is_open(&self, block: BlockId) -> bool {
        matches!(
            self.blocks[block.0 as usize].terminator,
            MirTerminator::Unreachable
        )
    }

    fn run_scope_defers(&mut self) {
        loop {
            let deferred = self.defer_scopes.last_mut().and_then(Vec::pop);
            let Some(deferred) = deferred else {
                break;
            };
            self.expression(&deferred);
        }
    }

    fn run_all_defers(&mut self) {
        for index in (0..self.defer_scopes.len()).rev() {
            while let Some(deferred) = self.defer_scopes[index].pop() {
                self.expression(&deferred);
            }
        }
    }

    fn run_loop_defers(&mut self, scope_depth: usize) {
        let deferred = self.defer_scopes[scope_depth..]
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev().cloned())
            .collect::<Vec<_>>();
        for expression in deferred {
            self.expression(&expression);
            if !self.is_open(self.current) {
                break;
            }
        }
    }

    fn lower_while(
        &mut self,
        condition: &Expression,
        body: &Expression,
        do_first: bool,
    ) -> Option<()> {
        let condition_block = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();
        if do_first {
            self.terminate(MirTerminator::Jump(body_block));
        } else {
            self.terminate(MirTerminator::Jump(condition_block));
        }

        self.loop_targets.push(LoopTargets {
            break_block: exit_block,
            continue_block: condition_block,
            defer_scope_depth: self.defer_scopes.len(),
        });

        if !do_first {
            self.current = condition_block;
            let condition = self.expression(condition)?;
            self.terminate(MirTerminator::Branch {
                condition,
                then_block: body_block,
                else_block: exit_block,
            });
        }

        self.current = body_block;
        self.expression(body);
        if self.is_open(self.current) {
            self.terminate(MirTerminator::Jump(condition_block));
        }

        if do_first {
            self.current = condition_block;
            let condition = self.expression(condition)?;
            self.terminate(MirTerminator::Branch {
                condition,
                then_block: body_block,
                else_block: exit_block,
            });
        }
        self.loop_targets.pop();
        self.current = exit_block;
        Some(())
    }

    fn lower_loop(&mut self, body: &Expression) {
        let loop_block = self.new_block();
        let exit_block = self.new_block();
        self.terminate(MirTerminator::Jump(loop_block));
        self.loop_targets.push(LoopTargets {
            break_block: exit_block,
            continue_block: loop_block,
            defer_scope_depth: self.defer_scopes.len(),
        });
        self.current = loop_block;
        self.expression(body);
        if self.is_open(self.current) {
            self.terminate(MirTerminator::Jump(loop_block));
        }
        self.loop_targets.pop();
        self.current = exit_block;
    }

    fn lower_for(&mut self, binding: &str, iterable: &Expression, body: &Expression) -> Option<()> {
        let element_type = match self.expression_type(iterable)? {
            HirType::Array { element, .. } | HirType::Slice(element) => Some(*element),
            _ => None,
        }?;
        let (array, length, scale) = self.lower_array_view(iterable)?;
        let array_slot = self.slot();
        self.emit(MirInstruction::StoreSlot {
            slot: array_slot,
            value: array,
        });

        let index_slot = self.slot();
        let zero = self.value();
        self.emit(MirInstruction::ConstInt {
            result: zero,
            value: 0,
        });
        self.emit(MirInstruction::StoreSlot {
            slot: index_slot,
            value: zero,
        });
        let length_slot = self.slot();
        self.emit(MirInstruction::StoreSlot {
            slot: length_slot,
            value: length,
        });
        let binding_slot = self.slot();
        let previous_binding = self
            .variables
            .insert(binding.to_string(), Variable::Slot(binding_slot));
        let previous_binding_type = self
            .variable_types
            .insert(binding.to_string(), element_type.clone());
        let previous_length = self.array_lengths.remove(binding);
        let previous_slice_length = self.slice_lengths.remove(binding);
        let previous_slice_scale = self.slice_scales.remove(binding);

        let condition_block = self.new_block();
        let body_block = self.new_block();
        let step_block = self.new_block();
        let exit_block = self.new_block();
        self.terminate(MirTerminator::Jump(condition_block));

        self.current = condition_block;
        let index = self.value();
        self.emit(MirInstruction::LoadSlot {
            result: index,
            slot: index_slot,
        });
        let length = self.value();
        self.emit(MirInstruction::LoadSlot {
            result: length,
            slot: length_slot,
        });
        let condition = self.value();
        self.emit(MirInstruction::Compare {
            result: condition,
            lhs: index,
            rhs: length,
            kind: CompareKind::Less,
        });
        self.terminate(MirTerminator::Branch {
            condition,
            then_block: body_block,
            else_block: exit_block,
        });

        self.loop_targets.push(LoopTargets {
            break_block: exit_block,
            continue_block: step_block,
            defer_scope_depth: self.defer_scopes.len(),
        });
        self.current = body_block;
        let index = self.value();
        self.emit(MirInstruction::LoadSlot {
            result: index,
            slot: index_slot,
        });
        let array = self.value();
        self.emit(MirInstruction::LoadSlot {
            result: array,
            slot: array_slot,
        });
        let address = self.value();
        self.emit(MirInstruction::IndexAddress {
            result: address,
            base: array,
            index,
            scale,
        });
        let item = self.value();
        self.emit_load_indirect(item, address, Some(&element_type));
        self.value_types.insert(item, element_type.clone());
        self.emit(MirInstruction::StoreSlot {
            slot: binding_slot,
            value: item,
        });
        self.expression(body);
        if self.is_open(self.current) {
            self.terminate(MirTerminator::Jump(step_block));
        }

        self.current = step_block;
        if self.is_open(self.current) {
            let index = self.value();
            self.emit(MirInstruction::LoadSlot {
                result: index,
                slot: index_slot,
            });
            let one = self.value();
            self.emit(MirInstruction::ConstInt {
                result: one,
                value: 1,
            });
            let next_index = self.value();
            self.emit(MirInstruction::Add {
                result: next_index,
                lhs: index,
                rhs: one,
            });
            self.emit(MirInstruction::StoreSlot {
                slot: index_slot,
                value: next_index,
            });
            self.terminate(MirTerminator::Jump(condition_block));
        }

        self.loop_targets.pop();
        self.current = exit_block;
        if let Some(previous) = previous_binding {
            self.variables.insert(binding.to_string(), previous);
        } else {
            self.variables.remove(binding);
        }
        if let Some(previous) = previous_binding_type {
            self.variable_types.insert(binding.to_string(), previous);
        } else {
            self.variable_types.remove(binding);
        }
        if let Some(length) = previous_length {
            self.array_lengths.insert(binding.to_string(), length);
        }
        if let Some(length) = previous_slice_length {
            self.slice_lengths.insert(binding.to_string(), length);
        }
        if let Some(scale) = previous_slice_scale {
            self.slice_scales.insert(binding.to_string(), scale);
        }
        Some(())
    }

    fn is_slice_value(&self, expression: &Expression) -> bool {
        match expression {
            Expression::LiteralString(_) | Expression::Slice { .. } => true,
            Expression::Identifier(name) => self.slice_lengths.contains_key(name),
            _ => matches!(self.expression_type(expression), Some(HirType::Slice(_))),
        }
    }

    fn function_signature_for_expression(
        &self,
        expression: &Expression,
    ) -> Option<(Vec<HirType>, HirType)> {
        match expression {
            Expression::Identifier(name) => self
                .variable_function_signatures
                .get(name)
                .cloned()
                .or_else(|| self.function_signatures.get(name).cloned()),
            Expression::StaticAccess { type_name, member } => {
                let target = self
                    .method_targets
                    .get(&(type_name.clone(), member.clone()))?;
                self.function_signatures.get(&target.symbol).cloned()
            }
            _ => None,
        }
    }

    fn index_scale(&self, object: &Expression) -> u8 {
        match object {
            Expression::Identifier(name) => self
                .slice_scales
                .get(name)
                .copied()
                .or_else(|| self.pointer_scales.get(name).copied())
                .or_else(|| match self.variable_types.get(name) {
                    Some(HirType::Array { element, .. }) => {
                        Some(pointer_scale_from_hir_type(element))
                    }
                    _ => None,
                })
                .unwrap_or(8),
            Expression::LiteralString(_) => 1,
            _ => 8,
        }
    }

    fn lower_index_base(&mut self, object: &Expression) -> Option<(ValueId, u8)> {
        if let Expression::Identifier(name) = object
            && self.pointer_scales.contains_key(name)
        {
            return Some((self.expression(object)?, self.index_scale(object)));
        }
        let (base, _, scale) = self.lower_array_view(object)?;
        Some((base, scale))
    }

    fn address_target_scale(&self, target: &Expression) -> u8 {
        match target {
            Expression::Index { object, .. } => self.index_scale(object),
            Expression::Dereference(pointer) => match pointer.as_ref() {
                Expression::Identifier(name) => self.pointer_scales.get(name).copied().unwrap_or(8),
                _ => 8,
            },
            _ => 8,
        }
    }

    fn lower_array_view(&mut self, expression: &Expression) -> Option<(ValueId, ValueId, u8)> {
        match expression {
            Expression::ArrayLiteral(values) => {
                let base = self.expression(expression)?;
                let length = if let Some(length) = self.slice_value_lengths.get(&base).copied() {
                    length
                } else {
                    let length = self.value();
                    self.emit(MirInstruction::ConstInt {
                        result: length,
                        value: values.len() as i128,
                    });
                    length
                };
                let scale = match self.expected_type_hint.as_ref() {
                    Some(HirType::Array { element, .. }) | Some(HirType::Slice(element)) => {
                        pointer_scale_from_hir_type(element)
                    }
                    _ => 8,
                };
                Some((base, length, scale))
            }
            Expression::LiteralString(_) => {
                let base = self.expression(expression)?;
                let length = self.slice_value_lengths.get(&base).copied()?;
                Some((base, length, 1))
            }
            Expression::Identifier(name) => {
                let base = self.expression(expression)?;
                if let Some(length_slot) = self.slice_lengths.get(name).copied() {
                    let length = self.slice_value_lengths.get(&base).copied().or_else(|| {
                        let length = self.value();
                        self.emit(MirInstruction::LoadSlot {
                            result: length,
                            slot: length_slot,
                        });
                        self.slice_value_lengths.insert(base, length);
                        Some(length)
                    })?;
                    let scale = *self.slice_scales.get(name)?;
                    Some((base, length, scale))
                } else {
                    let length = *self.array_lengths.get(name)?;
                    let length_value = self.value();
                    self.emit(MirInstruction::ConstInt {
                        result: length_value,
                        value: length as i128,
                    });
                    let scale = match self.variable_types.get(name) {
                        Some(HirType::Array { element, .. }) => {
                            pointer_scale_from_hir_type(element)
                        }
                        _ => 8,
                    };
                    Some((base, length_value, scale))
                }
            }
            Expression::Slice { object, start, end } => {
                let (base, object_length, scale) = self.lower_array_view(object)?;
                let start = if let Some(start) = start {
                    self.expression(start)?
                } else {
                    let start = self.value();
                    self.emit(MirInstruction::ConstInt {
                        result: start,
                        value: 0,
                    });
                    start
                };
                let end = if let Some(end) = end {
                    self.expression(end)?
                } else {
                    object_length
                };
                let sliced_base = self.value();
                self.emit(MirInstruction::IndexAddress {
                    result: sliced_base,
                    base,
                    index: start,
                    scale,
                });
                let length = self.value();
                self.emit(MirInstruction::Sub {
                    result: length,
                    lhs: end,
                    rhs: start,
                });
                self.slice_value_lengths.insert(sliced_base, length);
                Some((sliced_base, length, scale))
            }
            _ => {
                let HirType::Slice(element) = self.expression_type(expression)? else {
                    return None;
                };
                let base = self.expression(expression)?;
                let length = self.slice_value_lengths.get(&base).copied()?;
                Some((base, length, pointer_scale_from_hir_type(&element)))
            }
        }
    }

    fn logical(&mut self, lhs: &Expression, rhs: &Expression, and: bool) -> Option<ValueId> {
        let lhs = self.expression(lhs)?;
        let rhs_block = self.new_block();
        let short_block = self.new_block();
        let merge_block = self.new_block();
        let (then_block, else_block) = if and {
            (rhs_block, short_block)
        } else {
            (short_block, rhs_block)
        };
        self.terminate(MirTerminator::Branch {
            condition: lhs,
            then_block,
            else_block,
        });

        self.current = short_block;
        let short_value = self.value();
        self.emit(MirInstruction::ConstBool {
            result: short_value,
            value: !and,
        });
        let short_end = self.current;
        self.terminate(MirTerminator::Jump(merge_block));

        self.current = rhs_block;
        let rhs = self.expression(rhs)?;
        let rhs_end = self.current;
        self.terminate(MirTerminator::Jump(merge_block));

        self.current = merge_block;
        let result = self.value();
        self.emit(MirInstruction::Phi {
            result,
            incoming: vec![(short_end, short_value), (rhs_end, rhs)],
        });
        Some(result)
    }

    fn lower_match(
        &mut self,
        value: &Expression,
        arms: &[(crate::parser::Pattern, Expression)],
    ) -> Option<ValueId> {
        if arms.is_empty() {
            return None;
        }
        let value = self.expression(value)?;
        let merge_block = self.new_block();
        let mut incoming = Vec::new();
        let mut test_block = self.current;

        for (index, (pattern, body)) in arms.iter().enumerate() {
            let crate::parser::Pattern::Variant { name, binding } = pattern;
            let variant = self.enum_variants.get(name)?.clone();
            let arm_block = self.new_block();
            self.current = test_block;
            if index + 1 == arms.len() {
                self.terminate(MirTerminator::Jump(arm_block));
            } else {
                let discriminant = self.enum_discriminant(value, &variant)?;
                let expected = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: expected,
                    value: variant.tag as i128,
                });
                let condition = self.value();
                self.emit(MirInstruction::Compare {
                    result: condition,
                    lhs: discriminant,
                    rhs: expected,
                    kind: CompareKind::Equal,
                });
                let next_test = self.new_block();
                self.terminate(MirTerminator::Branch {
                    condition,
                    then_block: arm_block,
                    else_block: next_test,
                });
                test_block = next_test;
            }

            self.current = arm_block;
            let previous_binding = if let Some(binding) = binding {
                let payload = self.enum_payload(value)?;
                let slot = self.slot();
                self.emit(MirInstruction::StoreSlot {
                    slot,
                    value: payload,
                });
                self.variables.insert(binding.clone(), Variable::Slot(slot))
            } else {
                None
            };
            let result = self.expression(body);
            if let Some(binding) = binding {
                if let Some(previous) = previous_binding {
                    self.variables.insert(binding.clone(), previous);
                } else {
                    self.variables.remove(binding);
                }
            }
            let end_block = self.current;
            if self.is_open(end_block) {
                self.terminate(MirTerminator::Jump(merge_block));
                incoming.push((end_block, result));
            }
        }

        self.current = merge_block;
        if incoming.is_empty() || incoming.iter().any(|(_, value)| value.is_none()) {
            return None;
        }
        let values = incoming
            .into_iter()
            .map(|(block, value)| (block, value.unwrap()))
            .collect::<Vec<_>>();
        if values.len() == 1 {
            return Some(values[0].1);
        }
        let result = self.value();
        self.emit(MirInstruction::Phi {
            result,
            incoming: values,
        });
        Some(result)
    }

    fn lower_if_let(
        &mut self,
        variant_name: &str,
        binding: Option<&str>,
        value_expression: &Expression,
        then_branch: &Expression,
        else_branch: Option<&Expression>,
    ) -> Option<()> {
        let variant = self.enum_variants.get(variant_name)?.clone();
        let value = self.expression(value_expression)?;
        let discriminant = self.enum_discriminant(value, &variant)?;
        let expected = self.value();
        self.emit(MirInstruction::ConstInt {
            result: expected,
            value: variant.tag as i128,
        });
        let condition = self.value();
        self.emit(MirInstruction::Compare {
            result: condition,
            lhs: discriminant,
            rhs: expected,
            kind: CompareKind::Equal,
        });
        let then_block = self.new_block();
        let else_block = self.new_block();
        let merge_block = self.new_block();
        self.terminate(MirTerminator::Branch {
            condition,
            then_block,
            else_block,
        });

        self.current = then_block;
        let previous_binding = if let Some(binding) = binding {
            let payload = self.enum_payload(value)?;
            let slot = self.slot();
            self.emit(MirInstruction::StoreSlot {
                slot,
                value: payload,
            });
            self.variables
                .insert(binding.to_string(), Variable::Slot(slot))
        } else {
            None
        };
        self.expression(then_branch);
        if let Some(binding) = binding {
            if let Some(previous) = previous_binding {
                self.variables.insert(binding.to_string(), previous);
            } else {
                self.variables.remove(binding);
            }
        }
        let then_end = self.current;
        let then_live = self.is_open(then_end);
        if then_live {
            self.terminate(MirTerminator::Jump(merge_block));
        }

        self.current = else_block;
        if let Some(else_branch) = else_branch {
            self.expression(else_branch);
        }
        let else_end = self.current;
        let else_live = self.is_open(else_end);
        if else_live {
            self.terminate(MirTerminator::Jump(merge_block));
        }
        self.current = if then_live || else_live {
            merge_block
        } else {
            then_end
        };
        Some(())
    }

    fn lower_enum_value(
        &mut self,
        variant: &EnumVariant,
        payload: Option<&Expression>,
    ) -> Option<ValueId> {
        if !variant.uses_storage {
            let result = self.value();
            self.emit(MirInstruction::ConstInt {
                result,
                value: variant.tag as i128,
            });
            return Some(result);
        }

        let base = self.slot();
        self.next_slot += 1;
        let tag = self.value();
        self.emit(MirInstruction::ConstInt {
            result: tag,
            value: variant.tag as i128,
        });
        self.emit(MirInstruction::StoreSlot {
            slot: base,
            value: tag,
        });
        let payload_value = if let Some(payload) = payload {
            self.expression(payload)?
        } else {
            let zero = self.value();
            self.emit(MirInstruction::ConstInt {
                result: zero,
                value: 0,
            });
            zero
        };
        self.emit(MirInstruction::StoreSlot {
            slot: SlotId(base.0 + 1),
            value: payload_value,
        });
        let result = self.value();
        self.emit(MirInstruction::SlotAddress { result, slot: base });
        Some(result)
    }

    fn enum_discriminant(&mut self, value: ValueId, variant: &EnumVariant) -> Option<ValueId> {
        if !variant.uses_storage {
            return Some(value);
        }
        let zero = self.value();
        self.emit(MirInstruction::ConstInt {
            result: zero,
            value: 0,
        });
        let address = self.value();
        self.emit(MirInstruction::IndexAddress {
            result: address,
            base: value,
            index: zero,
            scale: 8,
        });
        let discriminant = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: discriminant,
            address,
        });
        Some(discriminant)
    }

    fn enum_payload(&mut self, value: ValueId) -> Option<ValueId> {
        let one = self.value();
        self.emit(MirInstruction::ConstInt {
            result: one,
            value: 1,
        });
        let address = self.value();
        self.emit(MirInstruction::IndexAddress {
            result: address,
            base: value,
            index: one,
            scale: 8,
        });
        let payload = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: payload,
            address,
        });
        Some(payload)
    }

    fn expression_struct_name(&self, expression: &Expression) -> Option<String> {
        match expression {
            Expression::Identifier(name) => self.variable_structs.get(name).cloned(),
            Expression::StructLiteral {
                type_name: Some(name),
                ..
            } => Some((*name).to_string()),
            Expression::AddressOf(inner) => self.expression_struct_name(inner),
            _ => match self.expression_type(expression)? {
                HirType::Named(name) if self.struct_fields.contains_key(&name) => Some(name),
                HirType::Pointer(inner) => match *inner {
                    HirType::Named(name) if self.struct_fields.contains_key(&name) => Some(name),
                    _ => None,
                },
                _ => None,
            },
        }
    }

    fn lower_struct_literal(
        &mut self,
        struct_name: &str,
        provided: &[(String, Expression)],
    ) -> Option<ValueId> {
        let fields = self.struct_fields.get(struct_name)?.clone();
        let base = self.slot();
        self.next_slot += fields.len().saturating_sub(1) as u32;
        for (index, (field_name, default)) in fields.iter().enumerate() {
            let expression = provided
                .iter()
                .find(|(name, _)| name == field_name)
                .map(|(_, expression)| expression)
                .or(default.as_ref())?;
            let field_type = self
                .struct_field_types
                .get(struct_name)?
                .iter()
                .find(|(name, _)| name == field_name)?
                .1
                .clone();
            let value = self.expression_expected(expression, &field_type)?;
            self.value_types.insert(value, field_type);
            self.emit(MirInstruction::StoreSlot {
                slot: SlotId(base.0 + index as u32),
                value,
            });
        }
        let result = self.value();
        self.emit(MirInstruction::SlotAddress { result, slot: base });
        Some(result)
    }
}

fn lower_function(
    function: &HirFunction,
    enum_variants: &std::collections::HashMap<String, EnumVariant>,
    struct_fields: &std::collections::HashMap<String, Vec<(String, Option<Expression>)>>,
    struct_field_types: &std::collections::HashMap<String, Vec<(String, HirType)>>,
    function_slice_params: &std::collections::HashMap<String, Vec<bool>>,
    method_targets: &std::collections::HashMap<(String, String), MethodTarget>,
    function_signatures: &std::collections::HashMap<String, (Vec<HirType>, HirType)>,
) -> MirFunction {
    let mut builder = Builder {
        next_value: 0,
        next_slot: 0,
        blocks: vec![MirBlock {
            id: BlockId(0),
            instructions: Vec::new(),
            terminator: MirTerminator::Unreachable,
        }],
        current: BlockId(0),
        variables: std::collections::HashMap::new(),
        array_lengths: std::collections::HashMap::new(),
        slice_lengths: std::collections::HashMap::new(),
        slice_scales: std::collections::HashMap::new(),
        pointer_scales: std::collections::HashMap::new(),
        enum_variants: enum_variants.clone(),
        struct_fields: struct_fields.clone(),
        struct_field_types: struct_field_types.clone(),
        function_slice_params: function_slice_params.clone(),
        method_targets: method_targets.clone(),
        function_signatures: function_signatures.clone(),
        variable_function_signatures: std::collections::HashMap::new(),
        variable_types: std::collections::HashMap::new(),
        value_types: std::collections::HashMap::new(),
        slice_value_lengths: std::collections::HashMap::new(),
        expected_type_hint: None,
        return_type: function.result.clone(),
        variable_structs: std::collections::HashMap::new(),
        defer_scopes: Vec::new(),
        loop_targets: Vec::new(),
    };
    let mut params = Vec::new();
    let mut abi_params = Vec::new();
    let mut abi_float_params = Vec::new();
    for (name, ty) in &function.params {
        let incoming = builder.value();
        if let Some(width) = float_width(ty) {
            abi_float_params.push((incoming, width));
        } else {
            abi_params.push(incoming);
        }
        builder.variable_types.insert(name.clone(), ty.clone());
        builder.value_types.insert(incoming, ty.clone());
        let slot = builder.slot();
        builder.variables.insert(name.clone(), Variable::Slot(slot));
        if let HirType::Array { length, .. } = ty {
            builder.array_lengths.insert(name.clone(), *length);
        }
        if let HirType::Slice(element) = ty {
            let length = builder.value();
            abi_params.push(length);
            let length_slot = builder.slot();
            builder.slice_lengths.insert(name.clone(), length_slot);
            builder
                .slice_scales
                .insert(name.clone(), pointer_scale_from_hir_type(element));
            builder.emit(MirInstruction::StoreSlot {
                slot: length_slot,
                value: length,
            });
        }
        if let HirType::Pointer(element) = ty {
            builder
                .pointer_scales
                .insert(name.clone(), pointer_scale_from_hir_type(element));
        }
        if let HirType::Function { params, result } = ty {
            builder
                .variable_function_signatures
                .insert(name.clone(), (params.clone(), (**result).clone()));
        }
        if let Some(struct_name) = struct_name_from_hir_type(ty, &builder.struct_fields) {
            builder.variable_structs.insert(name.clone(), struct_name);
        }
        builder.emit(MirInstruction::StoreSlot {
            slot,
            value: incoming,
        });
        params.push((name.clone(), ty.clone(), incoming));
    }
    let result = builder.expression_expected(&function.body, &function.result);
    let result = if function.result == HirType::Void {
        None
    } else {
        result
    };
    if builder.is_open(builder.current) {
        let slice_length = result.and_then(|value| {
            matches!(&function.result, HirType::Slice(_))
                .then(|| builder.slice_value_lengths.get(&value).copied())
                .flatten()
        });
        builder.terminate(MirTerminator::Return {
            value: result,
            slice_length,
        });
    }
    let mut blocks = builder.blocks;
    prune_unreachable_blocks(&mut blocks);
    MirFunction {
        name: function.name.clone(),
        params,
        abi_params,
        abi_float_params,
        result: function.result.clone(),
        slot_count: builder.next_slot,
        blocks,
    }
}

fn prune_unreachable_blocks(blocks: &mut Vec<MirBlock>) {
    if blocks.is_empty() {
        return;
    }
    let mut reachable = vec![false; blocks.len()];
    let mut pending = vec![BlockId(0)];
    while let Some(block_id) = pending.pop() {
        let index = block_id.0 as usize;
        if index >= blocks.len() || reachable[index] {
            continue;
        }
        reachable[index] = true;
        match blocks[index].terminator {
            MirTerminator::Jump(target) => pending.push(target),
            MirTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(then_block);
                pending.push(else_block);
            }
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
    }
    let remap = reachable
        .iter()
        .enumerate()
        .filter_map(|(index, is_reachable)| {
            is_reachable.then_some((BlockId(index as u32), BlockId(0)))
        })
        .enumerate()
        .map(|(new_index, (old_id, _))| (old_id, BlockId(new_index as u32)))
        .collect::<std::collections::HashMap<_, _>>();
    let mut retained = Vec::with_capacity(remap.len());
    for mut block in blocks.drain(..) {
        let Some(new_id) = remap.get(&block.id).copied() else {
            continue;
        };
        block.id = new_id;
        match &mut block.terminator {
            MirTerminator::Jump(target) => *target = remap[target],
            MirTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                *then_block = remap[then_block];
                *else_block = remap[else_block];
            }
            MirTerminator::Return { .. } | MirTerminator::Unreachable => {}
        }
        for instruction in &mut block.instructions {
            if let MirInstruction::Phi { incoming, .. } = instruction {
                incoming.retain_mut(|(predecessor, _)| {
                    if let Some(new_id) = remap.get(predecessor) {
                        *predecessor = *new_id;
                        true
                    } else {
                        false
                    }
                });
            }
        }
        retained.push(block);
    }
    *blocks = retained;
}

pub(crate) fn lower_hir(module: &HirModule) -> MirModule {
    let enum_variants = module
        .enums
        .iter()
        .flat_map(|enumeration| {
            let uses_storage = enumeration
                .variants
                .iter()
                .any(|(_, payload)| payload.is_some());
            enumeration
                .variants
                .iter()
                .enumerate()
                .map(move |(tag, (variant, payload))| {
                    (
                        variant.clone(),
                        EnumVariant {
                            tag: tag as u64,
                            payload: payload.clone(),
                            uses_storage,
                        },
                    )
                })
        })
        .collect::<std::collections::HashMap<_, _>>();
    let struct_fields = module
        .structs
        .iter()
        .map(|structure| {
            (
                structure.name.clone(),
                structure
                    .fields
                    .iter()
                    .map(|(name, _, default)| (name.clone(), default.clone()))
                    .collect(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let struct_field_types = module
        .structs
        .iter()
        .map(|structure| {
            (
                structure.name.clone(),
                structure
                    .fields
                    .iter()
                    .map(|(name, data_type, _)| (name.clone(), data_type.clone()))
                    .collect(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let function_slice_params = module
        .functions
        .iter()
        .map(|function| {
            (
                function.name.clone(),
                function
                    .params
                    .iter()
                    .map(|(_, ty)| matches!(ty, HirType::Slice(_)))
                    .collect(),
            )
        })
        .chain(module.extern_functions.iter().map(|function| {
            (
                function.name.clone(),
                function
                    .params
                    .iter()
                    .map(|ty| matches!(ty, HirType::Slice(_)))
                    .collect(),
            )
        }))
        .collect::<std::collections::HashMap<_, _>>();
    let method_targets = module
        .structs
        .iter()
        .flat_map(|structure| {
            structure
                .methods
                .iter()
                .map(|(method, symbol, has_receiver)| {
                    let parameter_count = module
                        .functions
                        .iter()
                        .find(|function| function.name == *symbol)
                        .map_or(0, |function| function.params.len());
                    (
                        (structure.name.clone(), method.clone()),
                        MethodTarget {
                            symbol: symbol.clone(),
                            parameter_count,
                            has_receiver: *has_receiver,
                        },
                    )
                })
        })
        .collect::<std::collections::HashMap<_, _>>();
    let function_signatures = module
        .functions
        .iter()
        .map(|function| {
            (
                function.name.clone(),
                (
                    function
                        .params
                        .iter()
                        .map(|(_, data_type)| data_type.clone())
                        .collect(),
                    function.result.clone(),
                ),
            )
        })
        .chain(module.extern_functions.iter().map(|function| {
            (
                function.name.clone(),
                (function.params.clone(), function.result.clone()),
            )
        }))
        .collect::<std::collections::HashMap<_, _>>();
    MirModule {
        functions: module
            .functions
            .iter()
            .map(|function| {
                lower_function(
                    function,
                    &enum_variants,
                    &struct_fields,
                    &struct_field_types,
                    &function_slice_params,
                    &method_targets,
                    &function_signatures,
                )
            })
            .collect(),
    }
}

fn struct_name_from_hir_type(
    data_type: &HirType,
    structs: &std::collections::HashMap<String, Vec<(String, Option<Expression>)>>,
) -> Option<String> {
    match data_type {
        HirType::Named(name) if structs.contains_key(name) => Some(name.clone()),
        HirType::Pointer(inner) => struct_name_from_hir_type(inner, structs),
        _ => None,
    }
}

fn pointer_scale_from_hir_type(data_type: &HirType) -> u8 {
    scalar_storage_size(data_type).unwrap_or(8)
}

fn scalar_storage_size(data_type: &HirType) -> Option<u8> {
    match data_type {
        HirType::Integer { bits, .. } | HirType::Float { bits } => match bits {
            8 => Some(1),
            16 => Some(2),
            32 => Some(4),
            64 => Some(8),
            _ => None,
        },
        HirType::Bool => Some(1),
        _ => None,
    }
}

fn float_width(data_type: &HirType) -> Option<u16> {
    match data_type {
        HirType::Float { bits } => Some(*bits),
        _ => None,
    }
}

fn pointer_scale_from_data_type(data_type: &crate::parser::DataType) -> u8 {
    match data_type {
        crate::parser::DataType::U8 | crate::parser::DataType::I8 | crate::parser::DataType::F8 => {
            1
        }
        crate::parser::DataType::U16
        | crate::parser::DataType::I16
        | crate::parser::DataType::F16 => 2,
        crate::parser::DataType::U32
        | crate::parser::DataType::I32
        | crate::parser::DataType::F32 => 4,
        crate::parser::DataType::Bool => 1,
        _ => 8,
    }
}
