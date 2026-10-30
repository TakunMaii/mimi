use crate::hir::{HirFunction, HirModule, HirType};
use crate::layout::{AbiClass, TypeLayouts, scalar_storage_size};
use crate::lir::TargetDataLayout;
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
        external: bool,
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
        scale: u64,
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
    pub(crate) data_layout: TargetDataLayout,
    pub(crate) functions: Vec<MirFunction>,
}

struct Builder {
    layout: TargetDataLayout,
    type_layouts: TypeLayouts,
    next_value: u32,
    next_slot: u32,
    blocks: Vec<MirBlock>,
    current: BlockId,
    variables: std::collections::HashMap<String, Variable>,
    array_lengths: std::collections::HashMap<String, u64>,
    slice_lengths: std::collections::HashMap<String, SlotId>,
    slice_scales: std::collections::HashMap<String, u64>,
    pointer_scales: std::collections::HashMap<String, u64>,
    enum_variants: std::collections::HashMap<String, EnumVariant>,
    struct_fields: std::collections::HashMap<String, Vec<(String, Option<Expression>)>>,
    struct_field_types: std::collections::HashMap<String, Vec<(String, HirType)>>,
    function_slice_params: std::collections::HashMap<String, Vec<bool>>,
    method_targets: std::collections::HashMap<(String, String), MethodTarget>,
    function_signatures: std::collections::HashMap<String, (Vec<HirType>, HirType)>,
    variadic_functions: std::collections::HashSet<String>,
    external_symbols: std::collections::HashMap<String, String>,
    external_globals: std::collections::HashMap<String, crate::hir::HirExternalGlobal>,
    global_constants: std::collections::HashMap<String, crate::hir::HirConstant>,
    function_namespace: Option<String>,
    variable_function_signatures: std::collections::HashMap<String, (Vec<HirType>, HirType)>,
    variable_types: std::collections::HashMap<String, HirType>,
    value_types: std::collections::HashMap<ValueId, HirType>,
    slice_value_lengths: std::collections::HashMap<ValueId, ValueId>,
    function_value_environments: std::collections::HashMap<ValueId, ValueId>,
    function_environment_slots: std::collections::HashMap<String, SlotId>,
    lambda_captures: std::collections::HashMap<String, Vec<String>>,
    closure_capture_types: Vec<(String, Vec<(String, HirType)>)>,
    expected_type_hint: Option<HirType>,
    return_type: HirType,
    return_pointer: Option<ValueId>,
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
    owner: String,
    tag: u64,
    payload: Option<HirType>,
    uses_storage: bool,
}

struct SavedBinding {
    variable: Option<Variable>,
    data_type: Option<HirType>,
    struct_name: Option<String>,
    array_length: Option<u64>,
    slice_length: Option<SlotId>,
    slice_scale: Option<u64>,
    pointer_scale: Option<u64>,
    function_signature: Option<(Vec<HirType>, HirType)>,
}

#[derive(Clone)]
struct MethodTarget {
    symbol: String,
    bound_symbol: Option<String>,
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
    fn bind_slot_value(&mut self, name: &str, data_type: &HirType, value: ValueId) -> SavedBinding {
        let previous = SavedBinding {
            variable: self.variables.remove(name),
            data_type: self.variable_types.remove(name),
            struct_name: self.variable_structs.remove(name),
            array_length: self.array_lengths.remove(name),
            slice_length: self.slice_lengths.remove(name),
            slice_scale: self.slice_scales.remove(name),
            pointer_scale: self.pointer_scales.remove(name),
            function_signature: self.variable_function_signatures.remove(name),
        };
        let slot = self.slot();
        self.variables
            .insert(name.to_string(), Variable::Slot(slot));
        self.variable_types
            .insert(name.to_string(), data_type.clone());
        if let HirType::Array { length, .. } = data_type {
            self.array_lengths.insert(name.to_string(), *length);
        }
        if let HirType::Slice(element) = data_type
            && let Some(length) = self.slice_value_lengths.get(&value).copied()
        {
            let length_slot = self.slot();
            self.slice_lengths.insert(name.to_string(), length_slot);
            self.slice_scales
                .insert(name.to_string(), self.element_stride(element));
            self.emit(MirInstruction::StoreSlot {
                slot: length_slot,
                value: length,
            });
        }
        if let HirType::Named(struct_name) = data_type
            && self.struct_fields.contains_key(struct_name)
        {
            self.variable_structs
                .insert(name.to_string(), struct_name.clone());
        }
        if let HirType::Pointer(element) = data_type {
            self.pointer_scales.insert(
                name.to_string(),
                pointer_scale_from_hir_type(element, self.layout.pointer_bytes()),
            );
        }
        if let HirType::Function { params, result } = data_type {
            self.variable_function_signatures
                .insert(name.to_string(), (params.clone(), result.as_ref().clone()));
        }
        self.emit(MirInstruction::StoreSlot { slot, value });
        previous
    }

    fn restore_slot_binding(&mut self, name: &str, saved: SavedBinding) {
        restore_binding(&mut self.variables, name, saved.variable);
        restore_binding(&mut self.variable_types, name, saved.data_type);
        restore_binding(&mut self.variable_structs, name, saved.struct_name);
        restore_binding(&mut self.array_lengths, name, saved.array_length);
        restore_binding(&mut self.slice_lengths, name, saved.slice_length);
        restore_binding(&mut self.slice_scales, name, saved.slice_scale);
        restore_binding(&mut self.pointer_scales, name, saved.pointer_scale);
        restore_binding(
            &mut self.variable_function_signatures,
            name,
            saved.function_signature,
        );
    }

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
            Expression::Identifier(name) if name == "null" => {
                let result = self.value();
                self.emit(MirInstruction::ConstInt { result, value: 0 });
                if let Some(data_type) = self.expected_type_hint.clone() {
                    self.value_types.insert(result, data_type);
                }
                Some(result)
            }
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
                    if let Some(environment_slot) =
                        self.function_environment_slots.get(name).copied()
                    {
                        let environment = self.value();
                        self.emit(MirInstruction::LoadSlot {
                            result: environment,
                            slot: environment_slot,
                        });
                        self.function_value_environments.insert(result, environment);
                        self.slice_value_lengths.insert(result, environment);
                    }
                    Some(result)
                }
                None => {
                    if let Some(capture_names) = self.lambda_captures.get(name).cloned() {
                        if !capture_names.is_empty() {
                            return self.lower_closure_value(name, &capture_names);
                        }
                    }
                    if let Some(constant) = self.resolve_global_constant_name(name) {
                        return self.lower_global_constant(&constant);
                    }
                    if let Some(variant) =
                        self.enum_variant_for_type(name, self.expected_type_hint.as_ref())
                    {
                        self.lower_enum_value(&variant, None)
                    } else {
                        let symbol = self.resolve_function_name(name);
                        self.function_signatures.get(&symbol)?;
                        let result = self.value();
                        let external = self.external_symbols.contains_key(&symbol);
                        let symbol = self.link_function_name(&symbol);
                        self.emit(MirInstruction::LoadFunctionAddress {
                            result,
                            symbol: format!("_{symbol}"),
                            external,
                        });
                        let environment = self.value();
                        self.emit(MirInstruction::ConstInt {
                            result: environment,
                            value: 0,
                        });
                        self.function_value_environments.insert(result, environment);
                        self.slice_value_lengths.insert(result, environment);
                        Some(result)
                    }
                }
            },
            Expression::ArrayLiteral(values) => {
                let expected_element = match self.expected_type_hint.as_ref() {
                    Some(HirType::Array { element, .. }) | Some(HirType::Slice(element)) => {
                        Some(element.as_ref().clone())
                    }
                    _ => values.first().and_then(|value| self.expression_type(value)),
                };
                let packed_size = expected_element
                    .as_ref()
                    .and_then(scalar_storage_size)
                    .filter(|size| u64::from(*size) < self.layout.stack_slot_bytes());
                let element_stride = expected_element
                    .as_ref()
                    .map(|data_type| self.element_stride(data_type))
                    .unwrap_or(self.layout.pointer_bytes());
                let total_bytes = (values.len() as u64).checked_mul(element_stride)?;
                let slot_count =
                    u32::try_from(total_bytes.div_ceil(self.layout.stack_slot_bytes())).ok()?;
                let inline_elements = expected_element
                    .as_ref()
                    .is_some_and(|data_type| self.is_inline_storage_aggregate(data_type));
                let base = self.slot();
                self.next_slot += slot_count.saturating_sub(1) as u32;
                let storage_base = if packed_size.is_some() || inline_elements {
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
                    if inline_elements {
                        let offset = (index as u64).checked_mul(element_stride)?;
                        let destination = self.byte_offset_address(storage_base?, offset);
                        self.lower_inline_value_into(
                            expected_element.as_ref()?,
                            expression,
                            destination,
                        )?;
                        continue;
                    }
                    let value = match expected_element.as_ref() {
                        Some(data_type) => self.expression_expected(expression, data_type)?,
                        None => self.expression(expression)?,
                    };
                    if let Some(base) = storage_base {
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
                            scale: u64::from(
                                packed_size.expect("packed array has an element size"),
                            ),
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
                let result = if let Some(base) = storage_base {
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
                let index = self.expression_unhinted(index)?;
                let address = self.value();
                self.emit(MirInstruction::IndexAddress {
                    result: address,
                    base,
                    index,
                    scale,
                });
                let data_type = self.expression_type(expression);
                if data_type
                    .as_ref()
                    .is_some_and(|data_type| self.is_inline_aggregate(data_type))
                {
                    return Some(address);
                }
                if let Some(data_type @ HirType::Slice(_)) = data_type.as_ref() {
                    return self.load_slice_descriptor(address, data_type);
                }
                if let Some(data_type @ HirType::Function { .. }) = data_type.as_ref() {
                    return self.load_function_descriptor(address, data_type);
                }
                let result = self.value();
                self.emit_load_indirect(result, address, data_type.as_ref());
                if let Some(data_type) = data_type {
                    self.value_types.insert(result, data_type);
                }
                Some(result)
            }
            Expression::AddressOf(inner) => {
                let address = match inner.as_ref() {
                    Expression::Identifier(name) => {
                        if self.variable_structs.contains_key(name)
                            || matches!(self.variable_types.get(name), Some(HirType::Array { .. }))
                        {
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
                    Expression::MemberAccess { object, member }
                        if let Expression::Identifier(package) = object.as_ref() =>
                    {
                        let name = self.resolve_package_member_name(package, member);
                        let global = self.external_globals.get(&name)?.clone();
                        self.external_global_address(&global)
                    }
                    Expression::Index { object, index } => {
                        let (base, scale) = self.lower_index_base(object)?;
                        let index = self.expression_unhinted(index)?;
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
                let data_type = self.expression_type(expression);
                if let Some(data_type @ HirType::Slice(_)) = data_type.as_ref() {
                    return self.load_slice_descriptor(address, data_type);
                }
                if let Some(data_type @ HirType::Function { .. }) = data_type.as_ref() {
                    return self.load_function_descriptor(address, data_type);
                }
                let result = self.value();
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
                if member == "len"
                    && matches!(
                        self.expression_type(object),
                        Some(HirType::Array { .. } | HirType::Slice(_))
                    )
                {
                    let (_, length, _) = self.lower_array_view(object)?;
                    self.value_types.insert(
                        length,
                        HirType::Integer {
                            bits: 64,
                            signed: false,
                        },
                    );
                    return Some(length);
                }
                if let Expression::Identifier(package) = object.as_ref() {
                    let name = self.resolve_package_member_name(package, member);
                    if self.global_constants.contains_key(&name) {
                        return self.lower_global_constant(&name);
                    }
                    if let Some(global) = self.external_globals.get(&name).cloned() {
                        let address = self.external_global_address(&global);
                        if matches!(global.data_type, HirType::Named(_)) {
                            self.value_types.insert(address, global.data_type.clone());
                            return Some(address);
                        }
                        let result = self.value();
                        self.emit_load_indirect(result, address, Some(&global.data_type));
                        self.value_types.insert(result, global.data_type);
                        return Some(result);
                    }
                    if let Some((parameters, result_type)) =
                        self.function_signatures.get(&name).cloned()
                    {
                        let result = self.value();
                        let external = self.external_symbols.contains_key(&name);
                        let symbol = self.link_function_name(&name);
                        self.emit(MirInstruction::LoadFunctionAddress {
                            result,
                            symbol: format!("_{symbol}"),
                            external,
                        });
                        self.value_types.insert(
                            result,
                            HirType::Function {
                                params: parameters,
                                result: Box::new(result_type),
                            },
                        );
                        let environment = self.value();
                        self.emit(MirInstruction::ConstInt {
                            result: environment,
                            value: 0,
                        });
                        self.function_value_environments.insert(result, environment);
                        self.slice_value_lengths.insert(result, environment);
                        return Some(result);
                    }
                }
                let struct_name = self.expression_struct_name(object)?;
                if self.struct_field_offset(&struct_name, member).is_none()
                    && let Some(target) = self
                        .method_targets
                        .get(&(struct_name.clone(), member.clone()))
                        .cloned()
                    && target.has_receiver
                {
                    return self.lower_bound_method_value(object, &target);
                }
                let field_offset = self.struct_field_offset(&struct_name, member)?;
                let field_type = self
                    .struct_field_types
                    .get(&struct_name)?
                    .iter()
                    .find(|(name, _)| name == member)?
                    .1
                    .clone();
                let base = self.expression(object)?;
                let address = self.byte_offset_address(base, field_offset);
                if self.is_inline_aggregate(&field_type) {
                    return Some(address);
                }
                if let HirType::Slice(_) = field_type {
                    return self.load_slice_descriptor(address, &field_type);
                }
                if matches!(field_type, HirType::Function { .. }) {
                    return self.load_function_descriptor(address, &field_type);
                }
                if let Some((bit_offset, bit_width)) = self.struct_bit_field(&struct_name, member) {
                    let value =
                        self.load_unsigned_bit_field(address, bit_offset, bit_width, &field_type)?;
                    if matches!(field_type, HirType::Integer { signed: true, .. }) {
                        return Some(self.sign_extend_bit_field(value, bit_width, &field_type));
                    }
                    return Some(value);
                }
                let result = self.value();
                self.emit_load_indirect(result, address, Some(&field_type));
                self.value_types.insert(result, field_type);
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
                    external: false,
                });
                let environment = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: environment,
                    value: 0,
                });
                self.function_value_environments.insert(result, environment);
                self.slice_value_lengths.insert(result, environment);
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
                let expected_float = self
                    .expected_type_hint
                    .as_ref()
                    .and_then(|ty| self.float_math_width(ty));
                if let Some(width) = expected_float.or_else(|| text.contains('.').then_some(64)) {
                    let value = text.parse::<f64>().ok()?;
                    let bits = match width {
                        8 => u64::from(crate::float8::to_e4m3fn(value)),
                        16 => f64_to_f16_bits(value) as u64,
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
                if let (Some(then_environment), Some(else_environment)) = (
                    self.function_value_environments.get(&then_value).copied(),
                    self.function_value_environments.get(&else_value).copied(),
                ) {
                    let environment = self.value();
                    self.emit(MirInstruction::Phi {
                        result: environment,
                        incoming: vec![(then_end, then_environment), (else_end, else_environment)],
                    });
                    self.function_value_environments.insert(result, environment);
                    self.slice_value_lengths.insert(result, environment);
                }
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
                    let (mut args, mut float_args) =
                        self.lower_function_value_arguments(arguments, &parameter_types)?;
                    self.append_function_environment(callee, &mut args);
                    let aggregate_result = self.is_inline_aggregate(&return_type);
                    let result = if aggregate_result {
                        Some(self.allocate_inline_temporary(&return_type)?)
                    } else {
                        (return_type != HirType::Void).then(|| self.value())
                    };
                    if aggregate_result {
                        self.prepend_indirect_result(&mut args, &mut float_args, result?);
                    }
                    let result_slice_length = (!aggregate_result
                        && self.is_slice_abi_type(&return_type))
                    .then(|| self.value());
                    if let Some(result) = result {
                        self.value_types.insert(result, return_type.clone());
                    }
                    if let (Some(result), Some(length)) = (result, result_slice_length) {
                        self.slice_value_lengths.insert(result, length);
                        if matches!(return_type, HirType::Function { .. }) {
                            self.function_value_environments.insert(result, length);
                        }
                    }
                    let result_float = self.float_abi_width(&return_type);
                    self.emit(MirInstruction::CallIndirect {
                        result: if aggregate_result { None } else { result },
                        result_slice_length,
                        callee,
                        args,
                        float_args,
                        result_float,
                    });
                    return result;
                }
                if !self.function_slice_params.contains_key(name)
                    && let Some(variant) =
                        self.enum_variant_for_type(name, self.expected_type_hint.as_ref())
                {
                    if arguments.len() != usize::from(variant.payload.is_some()) {
                        return None;
                    }
                    return self.lower_enum_value(&variant, arguments.first());
                }
                let function_name = self.resolve_function_name(name);
                let (mut args, mut float_args) =
                    self.lower_call_arguments(&function_name, arguments)?;
                let return_type = self
                    .function_signatures
                    .get(&function_name)
                    .map(|(_, result)| result.clone());
                let aggregate_result = return_type
                    .as_ref()
                    .is_some_and(|ty| self.is_inline_aggregate(ty));
                let result = if aggregate_result {
                    Some(self.allocate_inline_temporary(return_type.as_ref()?)?)
                } else {
                    return_type
                        .as_ref()
                        .is_none_or(|ty| *ty != HirType::Void)
                        .then(|| self.value())
                };
                if aggregate_result {
                    self.prepend_indirect_result(&mut args, &mut float_args, result?);
                }
                let result_slice_length = (!aggregate_result
                    && return_type
                        .as_ref()
                        .is_some_and(|ty| self.is_slice_abi_type(ty)))
                .then(|| self.value());
                if let (Some(result), Some(return_type)) = (result, return_type.as_ref()) {
                    self.value_types.insert(result, return_type.clone());
                }
                if let (Some(result), Some(length)) = (result, result_slice_length) {
                    self.slice_value_lengths.insert(result, length);
                    if return_type
                        .as_ref()
                        .is_some_and(|ty| matches!(ty, HirType::Function { .. }))
                    {
                        self.function_value_environments.insert(result, length);
                    }
                }
                let symbol = self.link_function_name(&function_name);
                self.emit(MirInstruction::Call {
                    result: if aggregate_result { None } else { result },
                    result_slice_length,
                    name: symbol,
                    args,
                    float_args,
                    result_float: return_type.as_ref().and_then(|ty| self.float_abi_width(ty)),
                });
                result
            }
            Expression::CallValue { callee, arguments } => {
                if let Expression::MemberAccess { object, member } = callee.as_ref()
                    && let Expression::Identifier(package) = object.as_ref()
                {
                    let name = self.resolve_package_member_name(package, member);
                    if let Some((_, return_type)) = self.function_signatures.get(&name).cloned() {
                        let (mut args, mut float_args) =
                            self.lower_call_arguments(&name, arguments)?;
                        let aggregate_result = self.is_inline_aggregate(&return_type);
                        let result = if aggregate_result {
                            Some(self.allocate_inline_temporary(&return_type)?)
                        } else {
                            (return_type != HirType::Void).then(|| self.value())
                        };
                        if aggregate_result {
                            self.prepend_indirect_result(&mut args, &mut float_args, result?);
                        }
                        let result_slice_length = (!aggregate_result
                            && self.is_slice_abi_type(&return_type))
                        .then(|| self.value());
                        if let Some(result) = result {
                            self.value_types.insert(result, return_type.clone());
                        }
                        if let (Some(result), Some(length)) = (result, result_slice_length) {
                            self.slice_value_lengths.insert(result, length);
                            if matches!(return_type, HirType::Function { .. }) {
                                self.function_value_environments.insert(result, length);
                            }
                        }
                        let symbol = self.link_function_name(&name);
                        self.emit(MirInstruction::Call {
                            result: if aggregate_result { None } else { result },
                            result_slice_length,
                            name: symbol,
                            args,
                            float_args,
                            result_float: self.float_abi_width(&return_type),
                        });
                        return result;
                    }
                }
                if let Expression::Identifier(name) = callee.as_ref()
                    && let Some((parameter_types, return_type)) = self
                        .variable_function_signatures
                        .get(name)
                        .cloned()
                        .or_else(|| {
                            self.function_signatures
                                .get(&self.resolve_function_name(name))
                                .cloned()
                        })
                {
                    let callee = self.expression(callee)?;
                    let (mut args, mut float_args) =
                        self.lower_function_value_arguments(arguments, &parameter_types)?;
                    self.append_function_environment(callee, &mut args);
                    let aggregate_result = self.is_inline_aggregate(&return_type);
                    let result = if aggregate_result {
                        Some(self.allocate_inline_temporary(&return_type)?)
                    } else {
                        (return_type != HirType::Void).then(|| self.value())
                    };
                    if aggregate_result {
                        self.prepend_indirect_result(&mut args, &mut float_args, result?);
                    }
                    let result_slice_length = (!aggregate_result
                        && self.is_slice_abi_type(&return_type))
                    .then(|| self.value());
                    if let Some(result) = result {
                        self.value_types.insert(result, return_type.clone());
                    }
                    if let (Some(result), Some(length)) = (result, result_slice_length) {
                        self.slice_value_lengths.insert(result, length);
                        if matches!(return_type, HirType::Function { .. }) {
                            self.function_value_environments.insert(result, length);
                        }
                    }
                    let result_float = self.float_abi_width(&return_type);
                    self.emit(MirInstruction::CallIndirect {
                        result: if aggregate_result { None } else { result },
                        result_slice_length,
                        callee,
                        args,
                        float_args,
                        result_float,
                    });
                    return result;
                }
                if let Expression::MemberAccess { object, member } = callee.as_ref()
                    && let Some(struct_name) = self.expression_struct_name(object)
                    && let Some(HirType::Function { params, result }) = self
                        .struct_field_types
                        .get(&struct_name)
                        .and_then(|fields| fields.iter().find(|(name, _)| name == member))
                        .map(|(_, ty)| ty.clone())
                {
                    return self.lower_indirect_function_call(callee, arguments, &params, &result);
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
                let (mut args, mut float_args) =
                    self.lower_call_arguments(&target.symbol, &call_arguments)?;
                let return_type = self
                    .function_signatures
                    .get(&target.symbol)
                    .map(|(_, result)| result.clone());
                let aggregate_result = return_type
                    .as_ref()
                    .is_some_and(|ty| self.is_inline_aggregate(ty));
                let result = if aggregate_result {
                    Some(self.allocate_inline_temporary(return_type.as_ref()?)?)
                } else {
                    return_type
                        .as_ref()
                        .is_none_or(|ty| *ty != HirType::Void)
                        .then(|| self.value())
                };
                if aggregate_result {
                    self.prepend_indirect_result(&mut args, &mut float_args, result?);
                }
                let result_slice_length = (!aggregate_result
                    && return_type
                        .as_ref()
                        .is_some_and(|ty| self.is_slice_abi_type(ty)))
                .then(|| self.value());
                if let (Some(result), Some(return_type)) = (result, return_type.as_ref()) {
                    self.value_types.insert(result, return_type.clone());
                }
                if let (Some(result), Some(length)) = (result, result_slice_length) {
                    self.slice_value_lengths.insert(result, length);
                    if return_type
                        .as_ref()
                        .is_some_and(|ty| matches!(ty, HirType::Function { .. }))
                    {
                        self.function_value_environments.insert(result, length);
                    }
                }
                self.emit(MirInstruction::Call {
                    result: if aggregate_result { None } else { result },
                    result_slice_length,
                    name: target.symbol,
                    args,
                    float_args,
                    result_float: return_type.as_ref().and_then(|ty| self.float_abi_width(ty)),
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
                    .filter(|ty| self.float_math_width(ty).is_some())
                    .or_else(|| self.expression_type(value));
                let value = if let Some(data_type) = expected.as_ref() {
                    self.expression_expected(value, data_type)
                } else {
                    self.expression(value)
                }?;
                let result = self.value();
                if let Some(width) = self
                    .value_type(value)
                    .and_then(|ty| self.float_math_width(&ty))
                {
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
                if let Some(width) = operand_type
                    .as_ref()
                    .and_then(|ty| self.float_math_width(ty))
                {
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
                let outer_function_environment_slots = self.function_environment_slots.clone();
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
                                }
                                .or_else(|| match value_type.as_ref() {
                                    Some(HirType::Function { params, result }) => {
                                        Some((params.clone(), result.as_ref().clone()))
                                    }
                                    _ => None,
                                });
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
                                if let Some(data_type) = value_type.as_ref() {
                                    self.variable_types.insert(name.clone(), data_type.clone());
                                }
                                if let Some((length, scale)) = slice_view {
                                    let length_slot = self.slot();
                                    self.emit(MirInstruction::StoreSlot {
                                        slot: length_slot,
                                        value: length,
                                    });
                                    self.slice_lengths.insert(name.clone(), length_slot);
                                    self.slice_scales.insert(name.clone(), scale);
                                } else if value_type
                                    .as_ref()
                                    .is_some_and(|ty| matches!(ty, HirType::Function { .. }))
                                {
                                    let environment = self
                                        .function_value_environments
                                        .get(&value)
                                        .copied()
                                        .unwrap_or_else(|| {
                                            let environment = self.value();
                                            self.emit(MirInstruction::ConstInt {
                                                result: environment,
                                                value: 0,
                                            });
                                            environment
                                        });
                                    let environment_slot = self.slot();
                                    self.emit(MirInstruction::StoreSlot {
                                        slot: environment_slot,
                                        value: environment,
                                    });
                                    self.function_environment_slots
                                        .insert(name.clone(), environment_slot);
                                } else {
                                    self.slice_lengths.remove(name);
                                    self.slice_scales.remove(name);
                                }
                                let pointer_scale = match data_type {
                                    Some(crate::parser::DataType::Pointer(element)) => {
                                        Some(pointer_scale_from_data_type(
                                            element,
                                            self.layout.pointer_bytes(),
                                        ))
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
                                let function_environment_slot =
                                    self.function_environment_slots.get(name).copied();
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
                                if let Some(environment_slot) = function_environment_slot {
                                    let environment = self
                                        .function_value_environments
                                        .get(&value)
                                        .copied()
                                        .unwrap_or_else(|| {
                                            let environment = self.value();
                                            self.emit(MirInstruction::ConstInt {
                                                result: environment,
                                                value: 0,
                                            });
                                            environment
                                        });
                                    self.emit(MirInstruction::StoreSlot {
                                        slot: environment_slot,
                                        value: environment,
                                    });
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
                                let index = self.expression_unhinted(index)?;
                                let address = self.value();
                                let scale = self.index_scale(object);
                                self.emit(MirInstruction::IndexAddress {
                                    result: address,
                                    base,
                                    index,
                                    scale,
                                });
                                if let Some(data_type) = expected_type.as_ref()
                                    && self.is_inline_storage_aggregate(data_type)
                                {
                                    self.lower_inline_value_into(data_type, value, address)?;
                                } else {
                                    let value = match expected_type.as_ref() {
                                        Some(data_type) => {
                                            self.expression_expected(value, data_type)?
                                        }
                                        None => self.expression(value)?,
                                    };
                                    self.emit_store_indirect(
                                        address,
                                        value,
                                        expected_type.as_ref(),
                                    );
                                }
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
                                if let Expression::Identifier(package) = object
                                    && let Some(global) = self
                                        .external_globals
                                        .get(&self.resolve_package_member_name(package, member))
                                        .cloned()
                                {
                                    if !global.mutable {
                                        return None;
                                    }
                                    let address = self.external_global_address(&global);
                                    let value =
                                        self.expression_expected(value, &global.data_type)?;
                                    self.emit_store_indirect(
                                        address,
                                        value,
                                        Some(&global.data_type),
                                    );
                                    continue;
                                }
                                let struct_name = self.expression_struct_name(object)?;
                                let field_offset =
                                    self.struct_field_offset(&struct_name, member)?;
                                let field_type = self
                                    .struct_field_types
                                    .get(&struct_name)?
                                    .iter()
                                    .find(|(name, _)| name == member)?
                                    .1
                                    .clone();
                                let base = self.expression(object)?;
                                let address = self.byte_offset_address(base, field_offset);
                                if let Some((bit_offset, bit_width)) =
                                    self.struct_bit_field(&struct_name, member)
                                {
                                    let value = self.expression_expected(value, &field_type)?;
                                    self.store_unsigned_bit_field(
                                        address,
                                        value,
                                        bit_offset,
                                        bit_width,
                                        &field_type,
                                    )?;
                                    continue;
                                }
                                if self.is_inline_storage_aggregate(&field_type) {
                                    self.lower_inline_value_into(&field_type, value, address)?;
                                } else {
                                    let value = self.expression_expected(value, &field_type)?;
                                    self.emit_store_indirect(address, value, Some(&field_type));
                                }
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
                                let value = self.copy_return_value(value)?;
                                self.run_all_defers();
                                let slice_length = value.and_then(|value| {
                                    self.is_slice_abi_type(&self.return_type)
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
                self.function_environment_slots = outer_function_environment_slots;
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
        let variadic = self.variadic_functions.contains(name);
        let mut values = Vec::new();
        let mut float_args = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let parameter_type = parameter_types
                .as_ref()
                .and_then(|parameters| parameters.get(index));
            if parameter_type.is_some_and(|ty| matches!(ty, HirType::Function { .. })) {
                let data_type = parameter_type.expect("checked function parameter");
                let value = self.expression_expected(argument, data_type)?;
                values.push(value);
                let environment = self
                    .function_value_environments
                    .get(&value)
                    .copied()
                    .unwrap_or_else(|| {
                        let environment = self.value();
                        self.emit(MirInstruction::ConstInt {
                            result: environment,
                            value: 0,
                        });
                        environment
                    });
                values.push(environment);
                continue;
            }
            if parameter_type.is_some_and(|ty| self.is_slice_abi_type(ty)) {
                let (base, length, _) = self.lower_array_view_expected(
                    argument,
                    parameter_type.expect("checked slice parameter"),
                )?;
                values.extend([base, length]);
                continue;
            }
            let value = match parameter_type {
                Some(data_type) if self.is_inline_aggregate(data_type) => {
                    self.materialize_inline_argument(argument, data_type)?
                }
                Some(data_type) => self.expression_expected(argument, data_type)?,
                None => self.expression(argument)?,
            };
            if let Some(width) = parameter_type.and_then(|ty| self.float_abi_width(ty)) {
                float_args.push((values.len(), width));
            } else if variadic && parameter_type.is_none() {
                if let Some(data_type) = self.expression_type(argument) {
                    if matches!(data_type, HirType::Float { bits: 64 }) {
                        float_args.push((values.len(), 64));
                    } else if matches!(data_type, HirType::Float { bits: 32 }) {
                        return None;
                    }
                }
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
                HirType::Function { .. } => {
                    let value = self.expression_expected(argument, parameter_type)?;
                    values.push(value);
                    self.append_function_environment(value, &mut values);
                }
                _ => {
                    if self.is_inline_aggregate(parameter_type) {
                        values.push(self.materialize_inline_argument(argument, parameter_type)?);
                        continue;
                    }
                    if let Some(width) = self.float_abi_width(parameter_type) {
                        float_args.push((values.len(), width));
                    }
                    values.push(self.expression_expected(argument, parameter_type)?);
                }
            }
        }
        Some((values, float_args))
    }

    fn lower_indirect_function_call(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        parameter_types: &[HirType],
        return_type: &HirType,
    ) -> Option<ValueId> {
        let callee = self.expression(callee)?;
        let (mut args, mut float_args) =
            self.lower_function_value_arguments(arguments, parameter_types)?;
        self.append_function_environment(callee, &mut args);
        let aggregate_result = self.is_inline_aggregate(return_type);
        let result = if aggregate_result {
            Some(self.allocate_inline_temporary(return_type)?)
        } else {
            (*return_type != HirType::Void).then(|| self.value())
        };
        if aggregate_result {
            self.prepend_indirect_result(&mut args, &mut float_args, result?);
        }
        let result_slice_length =
            (!aggregate_result && self.is_slice_abi_type(return_type)).then(|| self.value());
        if let Some(result) = result {
            self.value_types.insert(result, return_type.clone());
        }
        if let (Some(result), Some(environment)) = (result, result_slice_length) {
            self.slice_value_lengths.insert(result, environment);
            if matches!(return_type, HirType::Function { .. }) {
                self.function_value_environments.insert(result, environment);
            }
        }
        self.emit(MirInstruction::CallIndirect {
            result: if aggregate_result { None } else { result },
            result_slice_length,
            callee,
            args,
            float_args,
            result_float: self.float_abi_width(return_type),
        });
        result
    }

    fn binary(&mut self, lhs: &Expression, rhs: &Expression, kind: u8) -> Option<ValueId> {
        if matches!(kind, 0 | 1) {
            let lhs_type = self.expression_type(lhs);
            let rhs_type = self.expression_type(rhs);
            if kind == 1
                && let (Some(HirType::Pointer(lhs_element)), Some(HirType::Pointer(rhs_element))) =
                    (&lhs_type, &rhs_type)
                && lhs_element == rhs_element
            {
                let scale = self.element_stride(lhs_element);
                let lhs = self.expression(lhs)?;
                let rhs = self.expression(rhs)?;
                let byte_difference = self.value();
                self.emit(MirInstruction::Sub {
                    result: byte_difference,
                    lhs,
                    rhs,
                });
                let integer_type = HirType::Integer {
                    bits: 64,
                    signed: true,
                };
                self.value_types
                    .insert(byte_difference, integer_type.clone());
                if scale == 1 {
                    return Some(byte_difference);
                }
                let scale_value = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: scale_value,
                    value: i128::from(scale),
                });
                self.value_types.insert(scale_value, integer_type.clone());
                let element_difference = self.value();
                self.emit(MirInstruction::Div {
                    result: element_difference,
                    lhs: byte_difference,
                    rhs: scale_value,
                });
                self.value_types.insert(element_difference, integer_type);
                return Some(element_difference);
            }
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
                    scale: self.element_stride(&element_type),
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
        if let Some(width) = operation_type
            .as_ref()
            .and_then(|ty| self.float_math_width(ty))
        {
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

    fn expression_unhinted(&mut self, expression: &Expression) -> Option<ValueId> {
        let previous = self.expected_type_hint.take();
        let result = self.expression(expression);
        self.expected_type_hint = previous;
        result
    }

    fn lower_array_view_expected(
        &mut self,
        expression: &Expression,
        expected: &HirType,
    ) -> Option<(ValueId, ValueId, u64)> {
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

    fn load_unsigned_bit_field(
        &mut self,
        address: ValueId,
        bit_offset: u8,
        bit_width: u8,
        result_type: &HirType,
    ) -> Option<ValueId> {
        let storage_type = unsigned_bit_field_storage_type(result_type)?;
        let raw = self.value();
        self.emit_load_indirect(raw, address, Some(&storage_type));
        self.value_types.insert(raw, storage_type.clone());
        let shifted = if bit_offset == 0 {
            raw
        } else {
            let divisor = self.bit_field_constant(1i128 << bit_offset, &storage_type);
            let value = self.value();
            self.emit(MirInstruction::Div {
                result: value,
                lhs: raw,
                rhs: divisor,
            });
            self.value_types.insert(value, storage_type.clone());
            value
        };
        let storage_bits = match &storage_type {
            HirType::Integer { bits, .. } => *bits,
            _ => unreachable!(),
        };
        let extracted = if bit_width as u16 == storage_bits {
            shifted
        } else {
            let modulus = self.bit_field_constant(1i128 << bit_width, &storage_type);
            let value = self.value();
            self.emit(MirInstruction::Rem {
                result: value,
                lhs: shifted,
                rhs: modulus,
            });
            self.value_types.insert(value, storage_type.clone());
            value
        };
        self.value_types.insert(extracted, result_type.clone());
        Some(extracted)
    }

    fn store_unsigned_bit_field(
        &mut self,
        address: ValueId,
        new_value: ValueId,
        bit_offset: u8,
        bit_width: u8,
        field_type: &HirType,
    ) -> Option<()> {
        let storage_type = unsigned_bit_field_storage_type(field_type)?;
        let raw = self.value();
        self.emit_load_indirect(raw, address, Some(&storage_type));
        self.value_types.insert(raw, storage_type.clone());
        let old_field =
            self.load_unsigned_bit_field(address, bit_offset, bit_width, &storage_type)?;
        let modulus = self.bit_field_constant(1i128 << bit_width, &storage_type);
        let remainder = self.value();
        self.emit(MirInstruction::Rem {
            result: remainder,
            lhs: new_value,
            rhs: modulus,
        });
        self.value_types.insert(remainder, storage_type.clone());
        let normalized = self.value();
        self.emit(MirInstruction::Add {
            result: normalized,
            lhs: remainder,
            rhs: modulus,
        });
        self.value_types.insert(normalized, storage_type.clone());
        let bounded = self.value();
        self.emit(MirInstruction::Rem {
            result: bounded,
            lhs: normalized,
            rhs: modulus,
        });
        self.value_types.insert(bounded, storage_type.clone());
        let scale = self.bit_field_constant(1i128 << bit_offset, &storage_type);
        let old_scaled = self.value();
        self.emit(MirInstruction::Mul {
            result: old_scaled,
            lhs: old_field,
            rhs: scale,
        });
        self.value_types.insert(old_scaled, storage_type.clone());
        let new_scaled = self.value();
        self.emit(MirInstruction::Mul {
            result: new_scaled,
            lhs: bounded,
            rhs: scale,
        });
        self.value_types.insert(new_scaled, storage_type.clone());
        let cleared = self.value();
        self.emit(MirInstruction::Sub {
            result: cleared,
            lhs: raw,
            rhs: old_scaled,
        });
        self.value_types.insert(cleared, storage_type.clone());
        let combined = self.value();
        self.emit(MirInstruction::Add {
            result: combined,
            lhs: cleared,
            rhs: new_scaled,
        });
        self.value_types.insert(combined, storage_type.clone());
        self.emit_store_indirect(address, combined, Some(&storage_type));
        Some(())
    }

    fn bit_field_constant(&mut self, value: i128, data_type: &HirType) -> ValueId {
        let result = self.value();
        self.emit(MirInstruction::ConstInt { result, value });
        self.value_types.insert(result, data_type.clone());
        result
    }

    fn external_global_address(&mut self, global: &crate::hir::HirExternalGlobal) -> ValueId {
        let result = self.value();
        self.emit(MirInstruction::LoadFunctionAddress {
            result,
            symbol: format!("_{}", global.link_name),
            external: true,
        });
        result
    }

    fn sign_extend_bit_field(
        &mut self,
        value: ValueId,
        bit_width: u8,
        result_type: &HirType,
    ) -> ValueId {
        let byte_type = HirType::Integer {
            bits: 8,
            signed: false,
        };
        let sign = self.bit_field_constant(1i128 << (bit_width - 1), &byte_type);
        let condition = self.value();
        self.emit(MirInstruction::Compare {
            result: condition,
            lhs: value,
            rhs: sign,
            kind: CompareKind::GreaterEqual,
        });
        self.value_types.insert(condition, HirType::Bool);
        let negative_block = self.new_block();
        let positive_block = self.new_block();
        let merge_block = self.new_block();
        self.terminate(MirTerminator::Branch {
            condition,
            then_block: negative_block,
            else_block: positive_block,
        });
        self.current = negative_block;
        let modulus = self.bit_field_constant(1i128 << bit_width, result_type);
        let negative = self.value();
        self.emit(MirInstruction::Sub {
            result: negative,
            lhs: value,
            rhs: modulus,
        });
        self.value_types.insert(negative, result_type.clone());
        self.terminate(MirTerminator::Jump(merge_block));
        let negative_end = self.current;
        self.current = positive_block;
        self.terminate(MirTerminator::Jump(merge_block));
        let positive_end = self.current;
        self.current = merge_block;
        let result = self.value();
        self.emit(MirInstruction::Phi {
            result,
            incoming: vec![(negative_end, negative), (positive_end, value)],
        });
        self.value_types.insert(result, result_type.clone());
        result
    }

    fn clear_bit_field_storage(&mut self, address: ValueId, field_type: &HirType) -> Option<()> {
        let storage_type = unsigned_bit_field_storage_type(field_type)?;
        let zero = self.bit_field_constant(0, &storage_type);
        self.emit_store_indirect(address, zero, Some(&storage_type));
        Some(())
    }

    fn load_slice_descriptor(&mut self, address: ValueId, data_type: &HirType) -> Option<ValueId> {
        let HirType::Slice(_) = data_type else {
            return None;
        };
        let pointer = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: pointer,
            address,
        });
        let length_address = self.byte_offset_address(address, self.layout.pointer_bytes());
        let length = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: length,
            address: length_address,
        });
        self.slice_value_lengths.insert(pointer, length);
        self.value_types.insert(pointer, data_type.clone());
        Some(pointer)
    }

    fn store_slice_descriptor(&mut self, address: ValueId, pointer: ValueId) -> Option<()> {
        let length = self.slice_value_lengths.get(&pointer).copied()?;
        self.emit(MirInstruction::StoreIndirect {
            address,
            value: pointer,
        });
        let length_address = self.byte_offset_address(address, self.layout.pointer_bytes());
        self.emit(MirInstruction::StoreIndirect {
            address: length_address,
            value: length,
        });
        Some(())
    }

    fn copy_slice_descriptor(&mut self, source: ValueId, destination: ValueId) -> Option<()> {
        let pointer = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: pointer,
            address: source,
        });
        let source_length_address = self.byte_offset_address(source, self.layout.pointer_bytes());
        let length = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: length,
            address: source_length_address,
        });
        self.emit(MirInstruction::StoreIndirect {
            address: destination,
            value: pointer,
        });
        let destination_length_address =
            self.byte_offset_address(destination, self.layout.pointer_bytes());
        self.emit(MirInstruction::StoreIndirect {
            address: destination_length_address,
            value: length,
        });
        Some(())
    }

    fn load_function_descriptor(
        &mut self,
        address: ValueId,
        data_type: &HirType,
    ) -> Option<ValueId> {
        let result = self.value();
        self.emit(MirInstruction::LoadIndirect { result, address });
        let environment_address = self.byte_offset_address(address, self.layout.pointer_bytes());
        let environment = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: environment,
            address: environment_address,
        });
        self.function_value_environments.insert(result, environment);
        self.slice_value_lengths.insert(result, environment);
        self.value_types.insert(result, data_type.clone());
        Some(result)
    }

    fn store_function_descriptor(&mut self, address: ValueId, function: ValueId) -> Option<()> {
        let environment = self.function_value_environments.get(&function).copied()?;
        self.emit(MirInstruction::StoreIndirect {
            address,
            value: function,
        });
        let environment_address = self.byte_offset_address(address, self.layout.pointer_bytes());
        self.emit(MirInstruction::StoreIndirect {
            address: environment_address,
            value: environment,
        });
        Some(())
    }

    fn expression_type(&self, expression: &Expression) -> Option<HirType> {
        match expression {
            Expression::Identifier(name) if name == "null" => self.expected_type_hint.clone(),
            Expression::Identifier(name) => self
                .variable_types
                .get(name)
                .cloned()
                .or_else(|| {
                    self.resolve_global_constant_name(name)
                        .and_then(|name| self.global_constants.get(&name))
                        .map(|constant| constant.data_type.clone())
                })
                .or_else(|| {
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
            Expression::StructLiteral {
                type_name: Some(name),
                ..
            } => Some(HirType::Named((*name).to_string())),
            Expression::MemberAccess { object, member } => {
                if member == "len"
                    && matches!(
                        self.expression_type(object),
                        Some(HirType::Array { .. } | HirType::Slice(_))
                    )
                {
                    return Some(HirType::Integer {
                        bits: 64,
                        signed: false,
                    });
                }
                if let Expression::Identifier(package) = object.as_ref() {
                    let name = self.resolve_package_member_name(package, member);
                    if let Some(constant) = self.global_constants.get(&name) {
                        return Some(constant.data_type.clone());
                    }
                    if let Some(global) = self.external_globals.get(&name) {
                        return Some(global.data_type.clone());
                    }
                    if let Some((params, result)) = self.function_signatures.get(&name) {
                        return Some(HirType::Function {
                            params: params.clone(),
                            result: Box::new(result.clone()),
                        });
                    }
                }
                let struct_name = self.expression_struct_name(object)?;
                if self.struct_field_offset(&struct_name, member).is_none()
                    && let Some(bound_symbol) = self
                        .method_targets
                        .get(&(struct_name.clone(), member.clone()))
                        .and_then(|target| target.bound_symbol.as_ref())
                    && let Some((params, result)) = self.function_signatures.get(bound_symbol)
                {
                    return Some(HirType::Function {
                        params: params[..params.len() - 1].to_vec(),
                        result: Box::new(result.clone()),
                    });
                }
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
                        .get(&self.resolve_function_name(name))
                        .map(|(_, result)| result.clone())
                }),
            Expression::CallValue { callee, .. } => self
                .function_signature_for_expression(callee)
                .map(|(_, result)| result),
            Expression::Negate(inner) => self.expression_type(inner),
            Expression::Add(lhs, rhs)
            | Expression::Multiply(lhs, rhs)
            | Expression::Divide(lhs, rhs)
            | Expression::Remainder(lhs, rhs) => self
                .float_operation_type(lhs, rhs)
                .or_else(|| self.expression_type(lhs))
                .or_else(|| self.expression_type(rhs)),
            Expression::Subtract(lhs, rhs) => {
                let lhs_type = self.expression_type(lhs);
                let rhs_type = self.expression_type(rhs);
                if matches!(
                    (&lhs_type, &rhs_type),
                    (Some(HirType::Pointer(lhs)), Some(HirType::Pointer(rhs))) if lhs == rhs
                ) {
                    Some(HirType::Integer {
                        bits: 64,
                        signed: true,
                    })
                } else {
                    self.float_operation_type(lhs, rhs)
                        .or(lhs_type)
                        .or(rhs_type)
                }
            }
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
            .filter(|ty| self.float_math_width(ty).is_some())
            .or_else(|| rhs_type.filter(|ty| self.float_math_width(ty).is_some()))
            .or_else(|| {
                self.expected_type_hint
                    .clone()
                    .filter(|ty| self.float_math_width(ty).is_some())
            })
            .or_else(|| {
                self.expression_type(lhs)
                    .filter(|ty| self.float_math_width(ty).is_some())
            })
            .or_else(|| {
                self.expression_type(rhs)
                    .filter(|ty| self.float_math_width(ty).is_some())
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
        let binding_slice_length_slot = if let HirType::Slice(element) = &element_type {
            let slot = self.slot();
            self.slice_lengths.insert(binding.to_string(), slot);
            self.slice_scales
                .insert(binding.to_string(), self.element_stride(element));
            Some(slot)
        } else {
            None
        };

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
        let item = if matches!(element_type, HirType::Slice(_)) {
            self.load_slice_descriptor(address, &element_type)?
        } else {
            let item = self.value();
            self.emit_load_indirect(item, address, Some(&element_type));
            item
        };
        self.value_types.insert(item, element_type.clone());
        if let Some(length_slot) = binding_slice_length_slot {
            let length = self.slice_value_lengths.get(&item).copied()?;
            self.emit(MirInstruction::StoreSlot {
                slot: length_slot,
                value: length,
            });
        }
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
                .or_else(|| {
                    self.function_signatures
                        .get(&self.resolve_function_name(name))
                        .cloned()
                }),
            Expression::MemberAccess { object, member } => {
                if let Expression::Identifier(package) = object.as_ref()
                    && let Some(signature) = self
                        .function_signatures
                        .get(&self.resolve_package_member_name(package, member))
                {
                    return Some(signature.clone());
                }
                let struct_name = self.expression_struct_name(object)?;
                let bound_symbol = self
                    .method_targets
                    .get(&(struct_name, member.clone()))?
                    .bound_symbol
                    .as_ref()?;
                let (params, result) = self.function_signatures.get(bound_symbol)?;
                Some((params[..params.len() - 1].to_vec(), result.clone()))
            }
            Expression::StaticAccess { type_name, member } => {
                let target = self
                    .method_targets
                    .get(&(type_name.clone(), member.clone()))?;
                self.function_signatures.get(&target.symbol).cloned()
            }
            _ => None,
        }
    }

    fn resolve_function_name(&self, name: &str) -> String {
        if let Some(namespace) = &self.function_namespace {
            let qualified = format!("{namespace}.{name}");
            if self.function_signatures.contains_key(&qualified) {
                return qualified;
            }
        }
        name.to_string()
    }

    fn resolve_package_member_name(&self, package: &str, member: &str) -> String {
        let direct = format!("{package}.{member}");
        if let Some(namespace) = &self.function_namespace {
            let nested = format!("{namespace}.{direct}");
            if self.function_signatures.contains_key(&nested)
                || self.global_constants.contains_key(&nested)
            {
                return nested;
            }
        }
        direct
    }

    fn link_function_name(&self, name: &str) -> String {
        self.external_symbols
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    fn resolve_global_constant_name(&self, name: &str) -> Option<String> {
        if self.global_constants.contains_key(name) {
            return Some(name.to_string());
        }
        let namespace = self.function_namespace.as_ref()?;
        let qualified = format!("{namespace}.{name}");
        self.global_constants
            .contains_key(&qualified)
            .then_some(qualified)
    }

    fn lower_global_constant(&mut self, name: &str) -> Option<ValueId> {
        let constant = self.global_constants.get(name)?.clone();
        let previous_namespace = self.function_namespace.clone();
        self.function_namespace = name
            .rsplit_once('.')
            .map(|(namespace, _)| namespace.to_string());
        let value = self.expression_expected(&constant.value, &constant.data_type);
        self.function_namespace = previous_namespace;
        let value = value?;
        self.value_types.insert(value, constant.data_type.clone());
        Some(value)
    }

    fn index_scale(&self, object: &Expression) -> u64 {
        match self.expression_type(object) {
            Some(HirType::Pointer(element)) => return self.element_stride(&element),
            Some(HirType::Array { element, .. }) | Some(HirType::Slice(element)) => {
                return self.element_stride(&element);
            }
            _ => {}
        }
        match object {
            Expression::Identifier(name) => self
                .slice_scales
                .get(name)
                .copied()
                .or_else(|| self.pointer_scales.get(name).copied())
                .or_else(|| match self.variable_types.get(name) {
                    Some(HirType::Array { element, .. }) => Some(self.element_stride(element)),
                    _ => None,
                })
                .unwrap_or(self.layout.pointer_bytes()),
            Expression::LiteralString(_) => 1,
            _ => self.layout.pointer_bytes(),
        }
    }

    fn lower_index_base(&mut self, object: &Expression) -> Option<(ValueId, u64)> {
        if matches!(
            self.expression_type(object),
            Some(HirType::Pointer(_)) | Some(HirType::Array { .. })
        ) {
            return Some((self.expression(object)?, self.index_scale(object)));
        }
        let (base, _, scale) = self.lower_array_view(object)?;
        Some((base, scale))
    }

    fn address_target_scale(&self, target: &Expression) -> u64 {
        match target {
            Expression::Index { object, .. } => self.index_scale(object),
            Expression::Identifier(name) => match self.variable_types.get(name) {
                Some(data_type @ HirType::Array { .. }) => {
                    pointer_scale_from_hir_type(data_type, self.layout.pointer_bytes())
                }
                _ => self.layout.pointer_bytes(),
            },
            Expression::Dereference(pointer) => match pointer.as_ref() {
                Expression::Identifier(name) => self
                    .pointer_scales
                    .get(name)
                    .copied()
                    .unwrap_or(self.layout.pointer_bytes()),
                _ => self.layout.pointer_bytes(),
            },
            _ => self.layout.pointer_bytes(),
        }
    }

    fn lower_array_view(&mut self, expression: &Expression) -> Option<(ValueId, ValueId, u64)> {
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
                        self.element_stride(element)
                    }
                    _ => values
                        .first()
                        .and_then(|value| self.expression_type(value))
                        .map(|element| self.element_stride(&element))
                        .unwrap_or(self.layout.pointer_bytes()),
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
                        Some(HirType::Array { element, .. }) => self.element_stride(element),
                        _ => self.layout.pointer_bytes(),
                    };
                    Some((base, length_value, scale))
                }
            }
            Expression::Slice { object, start, end } => {
                let (base, object_length, scale) = self.lower_array_view(object)?;
                let start = if let Some(start) = start {
                    self.expression_unhinted(start)?
                } else {
                    let start = self.value();
                    self.emit(MirInstruction::ConstInt {
                        result: start,
                        value: 0,
                    });
                    start
                };
                let end = if let Some(end) = end {
                    self.expression_unhinted(end)?
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
                Some((
                    base,
                    length,
                    pointer_scale_from_hir_type(&element, self.layout.pointer_bytes()),
                ))
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
        let value_type = self.expression_type(value);
        let value = self.expression(value)?;
        let merge_block = self.new_block();
        let mut incoming = Vec::new();
        let mut test_block = self.current;

        for (index, (pattern, body)) in arms.iter().enumerate() {
            let crate::parser::Pattern::Variant { name, binding } = pattern;
            let variant = self.enum_variant_for_type(name, value_type.as_ref())?;
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
                let payload_type = variant.payload.as_ref()?;
                let payload = self.enum_payload(value, &variant)?;
                let payload = if self.is_inline_aggregate(payload_type) {
                    let copy = self.allocate_inline_temporary(payload_type)?;
                    self.copy_inline_value_to_address(payload_type, payload, copy)?;
                    copy
                } else {
                    payload
                };
                Some(self.bind_slot_value(binding, payload_type, payload))
            } else {
                None
            };
            let result = self.expression(body);
            if let Some(binding) = binding {
                if let Some(previous) = previous_binding {
                    self.restore_slot_binding(binding, previous);
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
        let variant = self.enum_variant_for_type(
            variant_name,
            self.expression_type(value_expression).as_ref(),
        )?;
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
            let payload_type = variant.payload.as_ref()?;
            let payload = self.enum_payload(value, &variant)?;
            let payload = if self.is_inline_aggregate(payload_type) {
                let copy = self.allocate_inline_temporary(payload_type)?;
                self.copy_inline_value_to_address(payload_type, payload, copy)?;
                copy
            } else {
                payload
            };
            Some(self.bind_slot_value(binding, payload_type, payload))
        } else {
            None
        };
        self.expression(then_branch);
        if let Some(binding) = binding {
            if let Some(previous) = previous_binding {
                self.restore_slot_binding(binding, previous);
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

    fn enum_variant_for_type(
        &self,
        name: &str,
        data_type: Option<&HirType>,
    ) -> Option<EnumVariant> {
        if let Some(HirType::Named(owner)) = data_type {
            if let Some(variant) = self.enum_variants.get(&format!("{owner}::{name}")) {
                return Some(variant.clone());
            }
        }
        self.enum_variants.get(name).cloned()
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

        let slot_count = self.enum_storage_slot_count(&variant.owner)?;
        let base = self.slot();
        self.next_slot = self.next_slot.checked_add(slot_count.saturating_sub(1))?;
        let tag = self.value();
        self.emit(MirInstruction::ConstInt {
            result: tag,
            value: variant.tag as i128,
        });
        self.emit(MirInstruction::StoreSlot {
            slot: base,
            value: tag,
        });
        let address = self.slot_address_value(base);
        let slot_bytes = self.layout.stack_slot_bytes();
        let payload_address = self.byte_offset_address(address, slot_bytes);
        if let (Some(payload_expression), Some(payload_type)) = (payload, variant.payload.as_ref())
        {
            if self.is_inline_storage_aggregate(payload_type) {
                self.lower_inline_value_into(payload_type, payload_expression, payload_address)?;
            } else {
                let payload_value = self.expression_expected(payload_expression, payload_type)?;
                self.emit_store_indirect(payload_address, payload_value, Some(payload_type));
            }
        } else {
            for offset in 0..slot_count.saturating_sub(1) {
                let zero = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: zero,
                    value: 0,
                });
                let address =
                    self.byte_offset_address(payload_address, u64::from(offset) * slot_bytes);
                self.emit(MirInstruction::StoreIndirect {
                    address,
                    value: zero,
                });
            }
        }
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
            scale: self.layout.stack_slot_bytes(),
        });
        let discriminant = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: discriminant,
            address,
        });
        Some(discriminant)
    }

    fn enum_payload(&mut self, value: ValueId, variant: &EnumVariant) -> Option<ValueId> {
        let payload_type = variant.payload.as_ref()?;
        let address = self.byte_offset_address(value, self.layout.stack_slot_bytes());
        if matches!(payload_type, HirType::Slice(_)) {
            self.load_slice_descriptor(address, payload_type)
        } else if self.is_inline_aggregate(payload_type) {
            Some(address)
        } else {
            let payload = self.value();
            self.emit_load_indirect(payload, address, Some(payload_type));
            self.value_types.insert(payload, payload_type.clone());
            Some(payload)
        }
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
        let slot_count = self.struct_slot_count(struct_name)?;
        let base = self.slot();
        self.next_slot += slot_count.saturating_sub(1);
        self.lower_struct_fields_into(struct_name, provided, base)?;
        let result = self.value();
        self.emit(MirInstruction::SlotAddress { result, slot: base });
        Some(result)
    }

    fn struct_slot_count(&self, struct_name: &str) -> Option<u32> {
        self.type_layouts
            .aggregate_slot_count(&HirType::Named(struct_name.to_string()))
    }

    fn struct_field_offset(&self, struct_name: &str, member: &str) -> Option<u64> {
        self.type_layouts
            .struct_field_offset_bytes(struct_name, member)
    }

    fn struct_bit_field(&self, struct_name: &str, member: &str) -> Option<(u8, u8)> {
        self.type_layouts.struct_bit_field(struct_name, member)
    }

    fn is_inline_aggregate(&self, data_type: &HirType) -> bool {
        self.type_layouts.is_indirect_aggregate(data_type)
    }

    fn is_slice_abi_type(&self, data_type: &HirType) -> bool {
        self.type_layouts
            .abi_class(data_type)
            .is_some_and(|class| matches!(class, AbiClass::SlicePair))
    }

    fn float_abi_width(&self, data_type: &HirType) -> Option<u16> {
        match self.type_layouts.abi_class(data_type)? {
            AbiClass::Float(width) if width != 8 => Some(width),
            _ => None,
        }
    }

    fn float_math_width(&self, data_type: &HirType) -> Option<u16> {
        match data_type {
            HirType::Float { bits } => Some(*bits),
            _ => None,
        }
    }

    fn is_inline_storage_aggregate(&self, data_type: &HirType) -> bool {
        self.is_inline_aggregate(data_type) || self.is_slice_abi_type(data_type)
    }

    fn inline_aggregate_slot_count(&self, data_type: &HirType) -> Option<u32> {
        self.type_layouts.aggregate_slot_count(data_type)
    }

    fn allocate_inline_temporary(&mut self, data_type: &HirType) -> Option<ValueId> {
        let slot_count = self.inline_aggregate_slot_count(data_type)?;
        let base = self.slot();
        self.next_slot = self.next_slot.checked_add(slot_count - 1)?;
        Some(self.slot_address_value(base))
    }

    fn materialize_inline_argument(
        &mut self,
        expression: &Expression,
        data_type: &HirType,
    ) -> Option<ValueId> {
        let source = match (expression, data_type) {
            (
                Expression::StructLiteral {
                    type_name: None,
                    fields,
                },
                HirType::Named(name),
            ) if self.struct_fields.contains_key(name) => {
                self.lower_struct_literal(name, fields)?
            }
            _ => self.expression_expected(expression, data_type)?,
        };
        let destination = self.allocate_inline_temporary(data_type)?;
        self.copy_inline_value_to_address(data_type, source, destination)?;
        Some(destination)
    }

    fn prepend_indirect_result(
        &mut self,
        args: &mut Vec<ValueId>,
        float_args: &mut [(usize, u16)],
        result: ValueId,
    ) {
        args.insert(0, result);
        for (position, _) in float_args {
            *position += 1;
        }
    }

    fn append_function_environment(&mut self, callee: ValueId, args: &mut Vec<ValueId>) {
        let environment = self
            .function_value_environments
            .get(&callee)
            .copied()
            .unwrap_or_else(|| {
                let environment = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: environment,
                    value: 0,
                });
                environment
            });
        args.push(environment);
    }

    fn lower_bound_method_value(
        &mut self,
        object: &Expression,
        target: &MethodTarget,
    ) -> Option<ValueId> {
        let symbol = target.bound_symbol.as_ref()?;
        let receiver_type = self
            .function_signatures
            .get(&target.symbol)?
            .0
            .first()?
            .clone();
        let object_type = self.expression_type(object)?;
        let environment = match &receiver_type {
            HirType::Named(_) => {
                let source = self.expression(object)?;
                let layout = self.type_layouts.layout(&receiver_type)?;
                let size = self.value();
                self.emit(MirInstruction::ConstInt {
                    result: size,
                    value: layout.size_bytes as i128,
                });
                let environment = self.value();
                self.emit(MirInstruction::Call {
                    result: Some(environment),
                    result_slice_length: None,
                    name: "malloc".to_string(),
                    args: vec![size],
                    float_args: vec![],
                    result_float: None,
                });
                self.copy_inline_value_to_address(&receiver_type, source, environment)?;
                environment
            }
            HirType::Pointer(_) => {
                if matches!(object_type, HirType::Pointer(_)) {
                    self.expression(object)?
                } else {
                    self.expression(&Expression::AddressOf(Box::new(object.clone())))?
                }
            }
            _ => return None,
        };
        let result = self.value();
        self.emit(MirInstruction::LoadFunctionAddress {
            result,
            symbol: format!("_{symbol}"),
            external: false,
        });
        let (params, return_type) = self.function_signatures.get(symbol)?;
        self.value_types.insert(
            result,
            HirType::Function {
                params: params[..params.len() - 1].to_vec(),
                result: Box::new(return_type.clone()),
            },
        );
        self.function_value_environments.insert(result, environment);
        self.slice_value_lengths.insert(result, environment);
        Some(result)
    }

    fn lower_closure_value(&mut self, name: &str, capture_names: &[String]) -> Option<ValueId> {
        let captures = capture_names
            .iter()
            .filter_map(|capture| {
                self.variable_types
                    .get(capture)
                    .cloned()
                    .map(|data_type| (capture.clone(), data_type))
            })
            .collect::<Vec<_>>();
        if captures.is_empty() {
            return None;
        }
        let offsets = captures
            .iter()
            .scan(0u64, |offset, (_, data_type)| {
                let size = self.type_layouts.layout(data_type)?.size_bytes.max(1);
                let aligned = (*offset + 7) & !7;
                *offset = aligned.checked_add((size + 7) & !7)?;
                Some((aligned, data_type.clone()))
            })
            .collect::<Vec<_>>();
        if offsets.len() != captures.len() {
            return None;
        }
        let size = offsets
            .iter()
            .zip(&captures)
            .filter_map(|((offset, _), (_, data_type))| {
                self.type_layouts
                    .layout(data_type)
                    .map(|layout| offset + ((layout.size_bytes + 7) & !7))
            })
            .max()?;
        let size_value = self.value();
        self.emit(MirInstruction::ConstInt {
            result: size_value,
            value: size as i128,
        });
        let environment = self.value();
        self.emit(MirInstruction::Call {
            result: Some(environment),
            result_slice_length: None,
            name: "malloc".to_string(),
            args: vec![size_value],
            float_args: Vec::new(),
            result_float: None,
        });
        for (((capture, data_type), (offset, _)), _) in captures.iter().zip(&offsets).zip(0..) {
            let source = self.expression(&Expression::Identifier(capture.clone()))?;
            let destination = self.byte_offset_address(environment, *offset);
            if self.is_inline_aggregate(data_type) {
                self.copy_inline_value_to_address(data_type, source, destination)?;
            } else if matches!(data_type, HirType::Slice(_)) {
                self.store_slice_descriptor(destination, source)?;
            } else if matches!(data_type, HirType::Function { .. }) {
                self.store_function_descriptor(destination, source)?;
            } else {
                self.emit_store_indirect(destination, source, Some(data_type));
            }
        }
        self.closure_capture_types
            .push((name.to_string(), captures));
        let result = self.value();
        self.emit(MirInstruction::LoadFunctionAddress {
            result,
            symbol: format!("_{}", self.link_function_name(name)),
            external: false,
        });
        self.value_types.insert(
            result,
            self.function_signatures.get(name).map_or_else(
                || HirType::Function {
                    params: Vec::new(),
                    result: Box::new(HirType::Void),
                },
                |(params, result)| HirType::Function {
                    params: params.clone(),
                    result: Box::new(result.clone()),
                },
            ),
        );
        self.function_value_environments.insert(result, environment);
        self.slice_value_lengths.insert(result, environment);
        Some(result)
    }

    fn copy_return_value(&mut self, value: Option<ValueId>) -> Option<Option<ValueId>> {
        if self.is_inline_aggregate(&self.return_type) {
            let source = value?;
            let destination = self.return_pointer?;
            let return_type = self.return_type.clone();
            self.copy_inline_value_to_address(&return_type, source, destination)?;
            Some(Some(destination))
        } else {
            Some(value)
        }
    }

    fn enum_uses_storage(&self, enum_name: &str) -> bool {
        self.type_layouts.enum_uses_storage(enum_name)
    }

    fn enum_storage_slot_count(&self, enum_name: &str) -> Option<u32> {
        self.type_layouts.enum_storage_slot_count(enum_name)
    }

    fn element_stride(&self, data_type: &HirType) -> u64 {
        self.type_layouts
            .storage_stride(data_type)
            .unwrap_or_else(|| pointer_scale_from_hir_type(data_type, self.layout.pointer_bytes()))
    }

    fn inline_field_address(&mut self, base: ValueId, slot_offset: u32) -> ValueId {
        self.byte_offset_address(
            base,
            u64::from(slot_offset) * self.layout.stack_slot_bytes(),
        )
    }

    fn byte_offset_address(&mut self, base: ValueId, offset: u64) -> ValueId {
        let index = self.value();
        self.emit(MirInstruction::ConstInt {
            result: index,
            value: offset as i128,
        });
        let address = self.value();
        self.emit(MirInstruction::IndexAddress {
            result: address,
            base,
            index,
            scale: 1,
        });
        address
    }

    fn slot_address_value(&mut self, slot: SlotId) -> ValueId {
        let address = self.value();
        self.emit(MirInstruction::SlotAddress {
            result: address,
            slot,
        });
        address
    }

    fn copy_struct_to_address(
        &mut self,
        struct_name: &str,
        source: ValueId,
        destination: ValueId,
    ) -> Option<()> {
        let slot_count = self.struct_slot_count(struct_name)?;
        self.copy_words_to_address(source, destination, slot_count)
    }

    fn copy_words_to_address(
        &mut self,
        source: ValueId,
        destination: ValueId,
        slot_count: u32,
    ) -> Option<()> {
        for offset in 0..slot_count {
            let source_address = self.inline_field_address(source, offset);
            let value = self.value();
            self.emit(MirInstruction::LoadIndirect {
                result: value,
                address: source_address,
            });
            let destination_address = self.inline_field_address(destination, offset);
            self.emit(MirInstruction::StoreIndirect {
                address: destination_address,
                value,
            });
        }
        Some(())
    }

    fn lower_inline_value_into(
        &mut self,
        data_type: &HirType,
        expression: &Expression,
        destination: ValueId,
    ) -> Option<()> {
        match data_type {
            HirType::Slice(_) => {
                let value = self.expression_expected(expression, data_type)?;
                self.store_slice_descriptor(destination, value)
            }
            HirType::Function { .. } => {
                let value = self.expression_expected(expression, data_type)?;
                self.store_function_descriptor(destination, value)
            }
            HirType::Named(name) if self.struct_fields.contains_key(name) => {
                if let Expression::StructLiteral { fields, .. } = expression {
                    self.lower_struct_fields_into_address(name, fields, destination)
                } else {
                    let value = self.expression_expected(expression, data_type)?;
                    self.copy_struct_to_address(name, value, destination)
                }
            }
            HirType::Named(name) if self.enum_uses_storage(name) => {
                let value = self.expression_expected(expression, data_type)?;
                let slots = self.enum_storage_slot_count(name)?;
                self.copy_words_to_address(value, destination, slots)
            }
            HirType::Array { length, element } => {
                if let Expression::ArrayLiteral(values) = expression {
                    if values.len() as u64 != *length {
                        return None;
                    }
                    for (index, value) in values.iter().enumerate() {
                        let offset = (index as u64).checked_mul(self.element_stride(element))?;
                        let element_address = self.byte_offset_address(destination, offset);
                        self.lower_inline_value_into(element, value, element_address)?;
                    }
                    Some(())
                } else {
                    let value = self.expression_expected(expression, data_type)?;
                    self.copy_inline_value_to_address(data_type, value, destination)
                }
            }
            _ => {
                let value = self.expression_expected(expression, data_type)?;
                self.emit_store_indirect(destination, value, Some(data_type));
                Some(())
            }
        }
    }

    fn copy_inline_value_to_address(
        &mut self,
        data_type: &HirType,
        source: ValueId,
        destination: ValueId,
    ) -> Option<()> {
        match data_type {
            HirType::Named(name) if self.struct_fields.contains_key(name) => {
                self.copy_struct_to_address(name, source, destination)
            }
            HirType::Named(name) if self.enum_uses_storage(name) => {
                let slots = self.enum_storage_slot_count(name)?;
                self.copy_words_to_address(source, destination, slots)
            }
            HirType::Array { length, element } => {
                let stride = self.element_stride(element);
                for index in 0..*length {
                    let offset = index.checked_mul(stride)?;
                    let source_address = self.byte_offset_address(source, offset);
                    let destination_address = self.byte_offset_address(destination, offset);
                    if matches!(element.as_ref(), HirType::Slice(_)) {
                        self.copy_slice_descriptor(source_address, destination_address)?;
                    } else if self.is_inline_storage_aggregate(element) {
                        self.copy_inline_value_to_address(
                            element,
                            source_address,
                            destination_address,
                        )?;
                    } else {
                        let value = self.value();
                        self.emit_load_indirect(value, source_address, Some(element));
                        self.emit_store_indirect(destination_address, value, Some(element));
                    }
                }
                Some(())
            }
            HirType::Slice(_) => self.store_slice_descriptor(destination, source),
            HirType::Function { .. } => self.copy_words_to_address(source, destination, 2),
            _ => {
                let value = self.value();
                self.emit_load_indirect(value, source, Some(data_type));
                self.emit_store_indirect(destination, value, Some(data_type));
                Some(())
            }
        }
    }

    fn lower_struct_fields_into(
        &mut self,
        struct_name: &str,
        provided: &[(String, Expression)],
        base: SlotId,
    ) -> Option<()> {
        let base_address = self.slot_address_value(base);
        self.lower_struct_fields_into_address(struct_name, provided, base_address)
    }

    fn lower_struct_fields_into_address(
        &mut self,
        struct_name: &str,
        provided: &[(String, Expression)],
        base_address: ValueId,
    ) -> Option<()> {
        let fields = self.struct_fields.get(struct_name)?.clone();
        if self.type_layouts.is_c_union(struct_name) {
            let (field_name, expression) = provided.first()?;
            let field_type = self
                .struct_field_types
                .get(struct_name)?
                .iter()
                .find(|(name, _)| name == field_name)?
                .1
                .clone();
            let offset = self.struct_field_offset(struct_name, field_name)?;
            let target_address = self.byte_offset_address(base_address, offset);
            if let Some((bit_offset, bit_width)) = self.struct_bit_field(struct_name, field_name) {
                self.clear_bit_field_storage(target_address, &field_type)?;
                let value = self.expression_expected(expression, &field_type)?;
                self.store_unsigned_bit_field(
                    target_address,
                    value,
                    bit_offset,
                    bit_width,
                    &field_type,
                )?;
                return Some(());
            }
            if self.is_inline_storage_aggregate(&field_type) {
                self.lower_inline_value_into(&field_type, expression, target_address)?;
            } else {
                let value = self.expression_expected(expression, &field_type)?;
                self.value_types.insert(value, field_type.clone());
                self.emit_store_indirect(target_address, value, Some(&field_type));
            }
            return Some(());
        }
        for (field_name, _) in &fields {
            if self.struct_bit_field(struct_name, field_name).is_some() {
                let offset = self.struct_field_offset(struct_name, field_name)?;
                let address = self.byte_offset_address(base_address, offset);
                let field_type = self
                    .struct_field_types
                    .get(struct_name)?
                    .iter()
                    .find(|(name, _)| name == field_name)?
                    .1
                    .clone();
                self.clear_bit_field_storage(address, &field_type)?;
            }
        }
        for (field_name, default) in fields {
            let expression = provided
                .iter()
                .find(|(name, _)| name == &field_name)
                .map(|(_, expression)| expression)
                .or(default.as_ref())?;
            let field_type = self
                .struct_field_types
                .get(struct_name)?
                .iter()
                .find(|(name, _)| name == &field_name)?
                .1
                .clone();
            let offset = self.struct_field_offset(struct_name, &field_name)?;
            let target_address = self.byte_offset_address(base_address, offset);
            if let Some((bit_offset, bit_width)) = self.struct_bit_field(struct_name, &field_name) {
                let value = self.expression_expected(expression, &field_type)?;
                self.store_unsigned_bit_field(
                    target_address,
                    value,
                    bit_offset,
                    bit_width,
                    &field_type,
                )?;
                continue;
            }
            if self.is_inline_storage_aggregate(&field_type) {
                self.lower_inline_value_into(&field_type, expression, target_address)?;
            } else {
                let value = self.expression_expected(expression, &field_type)?;
                self.value_types.insert(value, field_type.clone());
                self.emit_store_indirect(target_address, value, Some(&field_type));
            }
        }
        Some(())
    }
}

fn unsigned_bit_field_storage_type(data_type: &HirType) -> Option<HirType> {
    let bits = scalar_storage_size(data_type)?.checked_mul(8)? as u16;
    Some(HirType::Integer {
        bits,
        signed: false,
    })
}

fn restore_binding<T>(
    bindings: &mut std::collections::HashMap<String, T>,
    name: &str,
    previous: Option<T>,
) {
    if let Some(previous) = previous {
        bindings.insert(name.to_string(), previous);
    } else {
        bindings.remove(name);
    }
}

fn lower_function(
    function: &HirFunction,
    captures: &[(String, HirType)],
    lambda_captures: &std::collections::HashMap<String, Vec<String>>,
    enum_variants: &std::collections::HashMap<String, EnumVariant>,
    struct_fields: &std::collections::HashMap<String, Vec<(String, Option<Expression>)>>,
    struct_field_types: &std::collections::HashMap<String, Vec<(String, HirType)>>,
    function_slice_params: &std::collections::HashMap<String, Vec<bool>>,
    method_targets: &std::collections::HashMap<(String, String), MethodTarget>,
    function_signatures: &std::collections::HashMap<String, (Vec<HirType>, HirType)>,
    variadic_functions: &std::collections::HashSet<String>,
    external_symbols: &std::collections::HashMap<String, String>,
    external_globals: &std::collections::HashMap<String, crate::hir::HirExternalGlobal>,
    global_constants: &std::collections::HashMap<String, crate::hir::HirConstant>,
    type_layouts: &TypeLayouts,
    layout: TargetDataLayout,
) -> Option<(MirFunction, Vec<(String, Vec<(String, HirType)>)>)> {
    let mut builder = Builder {
        layout,
        type_layouts: type_layouts.clone(),
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
        variadic_functions: variadic_functions.clone(),
        external_symbols: external_symbols.clone(),
        external_globals: external_globals.clone(),
        global_constants: global_constants.clone(),
        function_namespace: function
            .name
            .rsplit_once('.')
            .map(|(namespace, _)| namespace.to_string()),
        variable_function_signatures: std::collections::HashMap::new(),
        variable_types: std::collections::HashMap::new(),
        value_types: std::collections::HashMap::new(),
        slice_value_lengths: std::collections::HashMap::new(),
        function_value_environments: std::collections::HashMap::new(),
        function_environment_slots: std::collections::HashMap::new(),
        lambda_captures: lambda_captures.clone(),
        closure_capture_types: Vec::new(),
        expected_type_hint: None,
        return_type: function.result.clone(),
        return_pointer: None,
        variable_structs: std::collections::HashMap::new(),
        defer_scopes: Vec::new(),
        loop_targets: Vec::new(),
    };
    let mut params = Vec::new();
    let mut abi_params = Vec::new();
    let mut abi_float_params = Vec::new();
    if builder.is_inline_aggregate(&function.result) {
        let incoming = builder.value();
        builder.return_pointer = Some(incoming);
        abi_params.push(incoming);
    }
    for (name, ty) in &function.params {
        let incoming = builder.value();
        if let Some(width) = builder.float_abi_width(ty) {
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
        if builder.is_slice_abi_type(ty)
            && let HirType::Slice(element) = ty
        {
            let length = builder.value();
            abi_params.push(length);
            let length_slot = builder.slot();
            builder.slice_lengths.insert(name.clone(), length_slot);
            builder
                .slice_scales
                .insert(name.clone(), builder.element_stride(element));
            builder.emit(MirInstruction::StoreSlot {
                slot: length_slot,
                value: length,
            });
        }
        if let HirType::Pointer(element) = ty {
            builder.pointer_scales.insert(
                name.clone(),
                pointer_scale_from_hir_type(element, layout.pointer_bytes()),
            );
        }
        if let HirType::Function { params, result } = ty {
            builder
                .variable_function_signatures
                .insert(name.clone(), (params.clone(), (**result).clone()));
            let environment = builder.value();
            abi_params.push(environment);
            let environment_slot = builder.slot();
            builder
                .function_environment_slots
                .insert(name.clone(), environment_slot);
            builder.emit(MirInstruction::StoreSlot {
                slot: environment_slot,
                value: environment,
            });
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
    if !captures.is_empty() {
        let environment = builder.value();
        abi_params.push(environment);
        let mut offset = 0u64;
        for (name, ty) in captures {
            offset = (offset + 7) & !7;
            let address = builder.byte_offset_address(environment, offset);
            let layout_size = builder.type_layouts.layout(ty)?.size_bytes.max(1);
            let slot = builder.slot();
            builder.variables.insert(name.clone(), Variable::Slot(slot));
            builder.variable_types.insert(name.clone(), ty.clone());
            if builder.is_inline_aggregate(ty) {
                builder.emit(MirInstruction::StoreSlot {
                    slot,
                    value: address,
                });
            } else if matches!(ty, HirType::Slice(_)) {
                let value = builder.load_slice_descriptor(address, ty)?;
                builder.emit(MirInstruction::StoreSlot { slot, value });
                let length = builder.slice_value_lengths.get(&value).copied()?;
                let length_slot = builder.slot();
                builder.slice_lengths.insert(name.clone(), length_slot);
                builder.emit(MirInstruction::StoreSlot {
                    slot: length_slot,
                    value: length,
                });
                if let HirType::Slice(element) = ty {
                    builder
                        .slice_scales
                        .insert(name.clone(), builder.element_stride(element));
                }
            } else if matches!(ty, HirType::Function { .. }) {
                let value = builder.load_function_descriptor(address, ty)?;
                builder.emit(MirInstruction::StoreSlot { slot, value });
                let environment = builder.function_value_environments.get(&value).copied()?;
                let environment_slot = builder.slot();
                builder
                    .function_environment_slots
                    .insert(name.clone(), environment_slot);
                builder.emit(MirInstruction::StoreSlot {
                    slot: environment_slot,
                    value: environment,
                });
            } else {
                let value = builder.value();
                builder.emit_load_indirect(value, address, Some(ty));
                builder.emit(MirInstruction::StoreSlot { slot, value });
            }
            if let HirType::Array { length, .. } = ty {
                builder.array_lengths.insert(name.clone(), *length);
            }
            if let Some(struct_name) = struct_name_from_hir_type(ty, &builder.struct_fields) {
                builder.variable_structs.insert(name.clone(), struct_name);
            }
            offset = offset.checked_add((layout_size + 7) & !7)?;
        }
    }
    let result = builder.expression_expected(&function.body, &function.result);
    let result = if function.result == HirType::Void {
        None
    } else {
        result
    };
    let result = builder
        .copy_return_value(result)
        .expect("aggregate function result must have a hidden return buffer");
    if builder.is_open(builder.current) {
        let slice_length = result.and_then(|value| {
            builder
                .is_slice_abi_type(&function.result)
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
    let mir_function = MirFunction {
        name: function.name.clone(),
        params,
        abi_params,
        abi_float_params,
        result: function.result.clone(),
        slot_count: builder.next_slot,
        blocks,
    };
    Some((mir_function, builder.closure_capture_types))
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
    lower_hir_with_layout(module, TargetDataLayout::HOST_64)
}

pub(crate) fn lower_hir_with_layout(module: &HirModule, layout: TargetDataLayout) -> MirModule {
    let enum_variants = module
        .enums
        .iter()
        .flat_map(|enumeration| {
            let uses_storage = enumeration
                .variants
                .iter()
                .any(|(_, payload)| payload.is_some());
            let generic = enumeration.name.contains('<');
            enumeration
                .variants
                .iter()
                .enumerate()
                .map(move |(tag, (variant, payload))| {
                    (
                        if generic {
                            format!("{}::{variant}", enumeration.name)
                        } else {
                            variant.clone()
                        },
                        EnumVariant {
                            owner: enumeration.name.clone(),
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
    let enum_layouts = module
        .enums
        .iter()
        .map(|enumeration| {
            (
                enumeration.name.clone(),
                enumeration
                    .variants
                    .iter()
                    .map(|(_, payload)| payload.clone())
                    .collect(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let type_layouts = TypeLayouts::new(layout, struct_field_types.clone(), enum_layouts)
        .with_c_records(module.c_record_layouts.clone());
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
                            bound_symbol: structure
                                .bound_methods
                                .iter()
                                .find(|(name, _)| name == method)
                                .map(|(_, symbol)| symbol.clone()),
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
    let variadic_functions = module
        .extern_functions
        .iter()
        .filter(|function| function.variadic)
        .map(|function| function.name.clone())
        .collect::<std::collections::HashSet<_>>();
    let external_symbols = module
        .extern_functions
        .iter()
        .map(|function| (function.name.clone(), function.link_name.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let external_globals = module
        .extern_globals
        .iter()
        .map(|global| (global.name.clone(), global.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let global_constants = module
        .constants
        .iter()
        .map(|constant| (constant.name.clone(), constant.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let lambda_captures = module
        .functions
        .iter()
        .filter(|function| !function.captures.is_empty())
        .map(|function| (function.name.clone(), function.captures.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut capture_types = std::collections::HashMap::<String, Vec<(String, HirType)>>::new();
    let mut functions = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        let captures = capture_types
            .get(&function.name)
            .cloned()
            .unwrap_or_default();
        let (lowered, discovered) = lower_function(
            function,
            &captures,
            &lambda_captures,
            &enum_variants,
            &struct_fields,
            &struct_field_types,
            &function_slice_params,
            &method_targets,
            &function_signatures,
            &variadic_functions,
            &external_symbols,
            &external_globals,
            &global_constants,
            &type_layouts,
            layout,
        )
        .expect("MIR lowering failed for a type-checked function");
        functions.push(lowered);
        capture_types.extend(discovered);
    }
    MirModule {
        data_layout: layout,
        functions,
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

fn pointer_scale_from_hir_type(data_type: &HirType, pointer_bytes: u64) -> u64 {
    pointer_stride_from_hir_type(data_type, pointer_bytes).unwrap_or(pointer_bytes)
}

fn pointer_stride_from_hir_type(data_type: &HirType, pointer_bytes: u64) -> Option<u64> {
    if let Some(size) = scalar_storage_size(data_type) {
        return Some(u64::from(size));
    }
    match data_type {
        HirType::Pointer(_) | HirType::Function { .. } => Some(pointer_bytes),
        HirType::Array { length, element }
            if *length > 0
                && (scalar_storage_size(element).is_some()
                    || matches!(
                        element.as_ref(),
                        HirType::Pointer(_) | HirType::Function { .. }
                    )) =>
        {
            length.checked_mul(pointer_stride_from_hir_type(element, pointer_bytes)?)
        }
        _ => None,
    }
}

fn f64_to_f16_bits(value: f64) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 48) & 0x8000) as u16;
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    if exponent == 0 {
        return sign;
    }
    if exponent == 0x7ff {
        if fraction == 0 {
            return sign | 0x7c00;
        }
        return sign | 0x7e00;
    }

    let unbiased = exponent - 1023;
    if unbiased > 15 {
        return sign | 0x7c00;
    }
    if unbiased >= -14 {
        let half_exponent = (unbiased + 15) as u16;
        let significand = (1u64 << 52) | fraction;
        let rounded = significand + (1u64 << 41) - 1 + ((significand >> 42) & 1);
        let rounded = rounded >> 42;
        if rounded >= 0x800 {
            let next_exponent = half_exponent + 1;
            return sign
                | if next_exponent >= 0x1f {
                    0x7c00
                } else {
                    next_exponent << 10
                };
        }
        return sign | (half_exponent << 10) | ((rounded as u16) & 0x03ff);
    }
    if unbiased < -25 {
        return sign;
    }

    let significand = (1u64 << 52) | fraction;
    let shift = (28 - unbiased) as u32;
    let halfway = 1u64 << (shift - 1);
    let rounded = significand >> shift;
    let remainder = significand & ((1u64 << shift) - 1);
    let rounded =
        rounded + u64::from(remainder > halfway || (remainder == halfway && rounded & 1 != 0));
    sign | (rounded as u16)
}

fn pointer_scale_from_data_type(data_type: &crate::parser::DataType, pointer_bytes: u64) -> u64 {
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
        crate::parser::DataType::Array(length, element) if *length > 0 => {
            let element_scale = match **element {
                crate::parser::DataType::U8
                | crate::parser::DataType::I8
                | crate::parser::DataType::F8
                | crate::parser::DataType::Bool => 1,
                crate::parser::DataType::U16
                | crate::parser::DataType::I16
                | crate::parser::DataType::F16 => 2,
                crate::parser::DataType::U32
                | crate::parser::DataType::I32
                | crate::parser::DataType::F32 => 4,
                crate::parser::DataType::U64
                | crate::parser::DataType::I64
                | crate::parser::DataType::F64
                | crate::parser::DataType::Pointer(_)
                | crate::parser::DataType::Function(_, _) => pointer_bytes,
                _ => return pointer_bytes,
            };
            length.checked_mul(element_scale).unwrap_or(pointer_bytes)
        }
        _ => pointer_bytes,
    }
}
