use crate::hir::{HirFunction, HirModule, HirType};
use crate::parser::Expression;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ValueId(pub(crate) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BlockId(pub(crate) u32);

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
    And {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
    },
    Or {
        result: ValueId,
        lhs: ValueId,
        rhs: ValueId,
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
    pub(crate) blocks: Vec<MirBlock>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MirModule {
    pub(crate) functions: Vec<MirFunction>,
}

struct Builder {
    next_value: u32,
    blocks: Vec<MirBlock>,
    current: BlockId,
    variables: std::collections::HashMap<String, ValueId>,
    returned: Option<Option<ValueId>>,
}

impl Builder {
    fn value(&mut self) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
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
            Expression::Identifier(name) => self.variables.get(name).copied(),
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
                let then_value = self.expression(then_branch)?;
                let then_end = self.current;
                self.terminate(MirTerminator::Jump(merge_block));
                self.current = else_block;
                let else_value = self.expression(else_branch)?;
                let else_end = self.current;
                self.terminate(MirTerminator::Jump(merge_block));
                self.current = merge_block;
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
                let mut result = None;
                for item in items {
                    match item {
                        crate::parser::ASTNode::VariableDefinition { name, value, .. }
                        | crate::parser::ASTNode::CompileTimeConstant { name, value, .. } => {
                            let Some(value) = self.expression(value) else {
                                self.variables = outer_variables;
                                return None;
                            };
                            self.variables.insert(name.clone(), value);
                        }
                        crate::parser::ASTNode::Assignment { name, value } => {
                            let Some(value) = self.expression(value) else {
                                self.variables = outer_variables;
                                return None;
                            };
                            self.variables.insert(name.clone(), value);
                        }
                        crate::parser::ASTNode::Expression(expression) => {
                            result = self.expression(expression);
                        }
                        crate::parser::ASTNode::Return(value) => {
                            self.returned = Some(
                                value
                                    .as_ref()
                                    .and_then(|expression| self.expression(expression)),
                            );
                            break;
                        }
                        _ => {}
                    }
                }
                self.variables = outer_variables;
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
        blocks: vec![MirBlock {
            id: BlockId(0),
            instructions: Vec::new(),
            terminator: MirTerminator::Unreachable,
        }],
        current: BlockId(0),
        variables: std::collections::HashMap::new(),
        returned: None,
    };
    let params = function
        .params
        .iter()
        .map(|(name, ty)| {
            let value = builder.value();
            builder.variables.insert(name.clone(), value);
            (name.clone(), ty.clone(), value)
        })
        .collect();
    let result = builder.expression(&function.body);
    let result = builder.returned.unwrap_or(result);
    let result = if function.result == HirType::Void {
        None
    } else {
        result
    };
    MirFunction {
        name: function.name.clone(),
        params,
        result: function.result.clone(),
        blocks: {
            builder.terminate(MirTerminator::Return(result));
            builder.blocks
        },
    }
}

pub(crate) fn lower_hir(module: &HirModule) -> MirModule {
    MirModule {
        functions: module.functions.iter().map(lower_function).collect(),
    }
}
