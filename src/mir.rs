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

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MirInstruction {
    ConstInt {
        result: ValueId,
        value: i128,
    },
    ConstBool {
        result: ValueId,
        value: bool,
    },
    ConstString {
        result: ValueId,
        bytes: Vec<u8>,
    },
    Add {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
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
    Phi {
        result: ValueId,
        incoming: Vec<(BlockId, ValueId)>,
    },
    Call {
        result: Option<ValueId>,
        name: String,
        args: Vec<ValueId>,
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
    },
    LoadIndirect {
        result: ValueId,
        address: ValueId,
    },
    StoreIndirect {
        address: ValueId,
        value: ValueId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MirTerminator {
    Return(Option<ValueId>),
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
    defer_scopes: Vec<Vec<Expression>>,
    loop_targets: Vec<LoopTargets>,
}

#[derive(Clone, Copy)]
enum Variable {
    Value(ValueId),
    Slot(SlotId),
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
            Expression::Identifier(name) => match self.variables.get(name).copied()? {
                Variable::Value(value) => Some(value),
                Variable::Slot(slot) => {
                    let result = self.value();
                    self.emit(MirInstruction::LoadSlot { result, slot });
                    Some(result)
                }
            },
            Expression::ArrayLiteral(values) => {
                let base = self.slot();
                self.next_slot += values.len().saturating_sub(1) as u32;
                for (index, expression) in values.iter().enumerate() {
                    let value = self.expression(expression)?;
                    self.emit(MirInstruction::StoreSlot {
                        slot: SlotId(base.0 + index as u32),
                        value,
                    });
                }
                let result = self.value();
                self.emit(MirInstruction::SlotAddress { result, slot: base });
                Some(result)
            }
            Expression::Index { object, index } => {
                let base = self.expression(object)?;
                let index = self.expression(index)?;
                let address = self.value();
                self.emit(MirInstruction::IndexAddress {
                    result: address,
                    base,
                    index,
                });
                let result = self.value();
                self.emit(MirInstruction::LoadIndirect { result, address });
                Some(result)
            }
            Expression::LiteralNumber(text) => {
                let result = self.value();
                let value = text.parse().ok()?;
                self.emit(MirInstruction::ConstInt { result, value });
                Some(result)
            }
            Expression::LiteralBool(value) => {
                let result = self.value();
                self.emit(MirInstruction::ConstBool {
                    result,
                    value: *value,
                });
                Some(result)
            }
            Expression::LiteralString(value) => {
                let result = self.value();
                self.emit(MirInstruction::ConstString {
                    result,
                    bytes: value.as_bytes().to_vec(),
                });
                Some(result)
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
                Some(result)
            }
            Expression::Call { name, arguments } => {
                let mut args = Vec::new();
                for argument in arguments {
                    let value = self.expression(argument)?;
                    args.push(value);
                    if let Expression::LiteralString(text) = argument {
                        let length = self.value();
                        self.emit(MirInstruction::ConstInt {
                            result: length,
                            value: text.len() as i128,
                        });
                        args.push(length);
                    }
                }
                let result = self.value();
                self.emit(MirInstruction::Call {
                    result: Some(result),
                    name: name.clone(),
                    args,
                });
                Some(result)
            }
            Expression::Add(lhs, rhs) => self.binary(lhs, rhs, 0),
            Expression::Subtract(lhs, rhs) => self.binary(lhs, rhs, 1),
            Expression::Multiply(lhs, rhs) => self.binary(lhs, rhs, 2),
            Expression::Divide(lhs, rhs) => self.binary(lhs, rhs, 3),
            Expression::Remainder(lhs, rhs) => self.binary(lhs, rhs, 4),
            Expression::Negate(value) => {
                let value = self.expression(value)?;
                let result = self.value();
                self.emit(MirInstruction::Neg { result, value });
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
                let lhs = self.expression(lhs)?;
                let rhs = self.expression(rhs)?;
                let kind = match expression {
                    Expression::Equal(_, _) => CompareKind::Equal,
                    Expression::NotEqual(_, _) => CompareKind::NotEqual,
                    Expression::LessThan(_, _) => CompareKind::Less,
                    Expression::LessThanOrEqual(_, _) => CompareKind::LessEqual,
                    Expression::GreaterThan(_, _) => CompareKind::Greater,
                    _ => CompareKind::GreaterEqual,
                };
                let result = self.value();
                self.emit(MirInstruction::Compare {
                    result,
                    lhs,
                    rhs,
                    kind,
                });
                Some(result)
            }
            Expression::Block(items) => {
                let outer_variables = self.variables.clone();
                let outer_array_lengths = self.array_lengths.clone();
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
                                let array_length = match data_type {
                                    Some(crate::parser::DataType::Array(length, _)) => {
                                        Some(*length)
                                    }
                                    _ => match value {
                                        Expression::ArrayLiteral(values) => {
                                            Some(values.len() as u64)
                                        }
                                        _ => None,
                                    },
                                };
                                let value = self.expression(value)?;
                                let slot = self.slot();
                                self.emit(MirInstruction::StoreSlot { slot, value });
                                self.variables.insert(name.clone(), Variable::Slot(slot));
                                if let Some(length) = array_length {
                                    self.array_lengths.insert(name.clone(), length);
                                } else {
                                    self.array_lengths.remove(name);
                                }
                            }
                            crate::parser::ASTNode::CompileTimeConstant { name, value, .. } => {
                                let value = self.expression(value)?;
                                self.variables.insert(name.clone(), Variable::Value(value));
                            }
                            crate::parser::ASTNode::Assignment { name, value } => {
                                let value = self.expression(value)?;
                                match self.variables.get(name).copied() {
                                    Some(Variable::Slot(slot)) => {
                                        self.emit(MirInstruction::StoreSlot { slot, value });
                                    }
                                    _ => {
                                        self.variables.insert(name.clone(), Variable::Value(value));
                                    }
                                }
                            }
                            crate::parser::ASTNode::IndexAssignment {
                                object,
                                index,
                                value,
                            } => {
                                let base = self.expression(object)?;
                                let index = self.expression(index)?;
                                let value = self.expression(value)?;
                                let address = self.value();
                                self.emit(MirInstruction::IndexAddress {
                                    result: address,
                                    base,
                                    index,
                                });
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
                                let value = value
                                    .as_ref()
                                    .and_then(|expression| self.expression(expression));
                                self.run_all_defers();
                                self.terminate(MirTerminator::Return(value));
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
                self.array_lengths = outer_array_lengths;
                result
            }
            _ => None,
        }
    }

    fn binary(&mut self, lhs: &Expression, rhs: &Expression, kind: u8) -> Option<ValueId> {
        let lhs = self.expression(lhs)?;
        let rhs = self.expression(rhs)?;
        let result = self.value();
        let instruction = match kind {
            0 => MirInstruction::Add { result, lhs, rhs },
            1 => MirInstruction::Sub { result, lhs, rhs },
            2 => MirInstruction::Mul { result, lhs, rhs },
            3 => MirInstruction::Div { result, lhs, rhs },
            _ => MirInstruction::Rem { result, lhs, rhs },
        };
        self.emit(instruction);
        Some(result)
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
        let length = match iterable {
            Expression::ArrayLiteral(values) => values.len() as u64,
            Expression::Identifier(name) => *self.array_lengths.get(name)?,
            _ => return None,
        };
        let array = self.expression(iterable)?;
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
        let length_value = self.value();
        self.emit(MirInstruction::ConstInt {
            result: length_value,
            value: length as i128,
        });
        self.emit(MirInstruction::StoreSlot {
            slot: length_slot,
            value: length_value,
        });
        let binding_slot = self.slot();
        let previous_binding = self
            .variables
            .insert(binding.to_string(), Variable::Slot(binding_slot));
        let previous_length = self.array_lengths.remove(binding);

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
        });
        let item = self.value();
        self.emit(MirInstruction::LoadIndirect {
            result: item,
            address,
        });
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
        if let Some(length) = previous_length {
            self.array_lengths.insert(binding.to_string(), length);
        }
        Some(())
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
}

fn lower_function(function: &HirFunction) -> MirFunction {
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
        defer_scopes: Vec::new(),
        loop_targets: Vec::new(),
    };
    let params = function
        .params
        .iter()
        .map(|(name, ty)| {
            let incoming = builder.value();
            let slot = builder.slot();
            builder.variables.insert(name.clone(), Variable::Slot(slot));
            if let HirType::Array { length, .. } = ty {
                builder.array_lengths.insert(name.clone(), *length);
            }
            builder.emit(MirInstruction::StoreSlot {
                slot,
                value: incoming,
            });
            (name.clone(), ty.clone(), incoming)
        })
        .collect();
    let result = builder.expression(&function.body);
    let result = if function.result == HirType::Void {
        None
    } else {
        result
    };
    if builder.is_open(builder.current) {
        builder.terminate(MirTerminator::Return(result));
    }
    let mut blocks = builder.blocks;
    prune_unreachable_blocks(&mut blocks);
    MirFunction {
        name: function.name.clone(),
        params,
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
            MirTerminator::Return(_) | MirTerminator::Unreachable => {}
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
            MirTerminator::Return(_) | MirTerminator::Unreachable => {}
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
    MirModule {
        functions: module.functions.iter().map(lower_function).collect(),
    }
}
