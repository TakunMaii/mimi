use crate::hir::{HirFunction, HirModule, HirType};
use crate::parser::Expression;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ValueId(pub(crate) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BlockId(pub(crate) u32);

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
    instructions: Vec<MirInstruction>,
    variables: std::collections::HashMap<String, ValueId>,
}

impl Builder {
    fn value(&mut self) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
    }

    fn expression(&mut self, expression: &Expression) -> Option<ValueId> {
        match expression {
            Expression::Identifier(name) => self.variables.get(name).copied(),
            Expression::LiteralNumber(text) => {
                let result = self.value();
                let value = text.parse().ok()?;
                self.instructions
                    .push(MirInstruction::ConstInt { result, value });
                Some(result)
            }
            Expression::LiteralBool(value) => {
                let result = self.value();
                self.instructions.push(MirInstruction::ConstBool {
                    result,
                    value: *value,
                });
                Some(result)
            }
            Expression::Call { name, arguments } => {
                let args = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.value();
                self.instructions.push(MirInstruction::Call {
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
            Expression::Block(items) => items.iter().rev().find_map(|item| match item {
                crate::parser::ASTNode::Expression(value) => self.expression(value),
                crate::parser::ASTNode::Return(value) => {
                    value.as_ref().and_then(|value| self.expression(value))
                }
                _ => None,
            }),
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
            _ => MirInstruction::Div { result, lhs, rhs },
        };
        self.instructions.push(instruction);
        Some(result)
    }
}

fn lower_function(function: &HirFunction) -> MirFunction {
    let mut builder = Builder {
        next_value: 0,
        instructions: Vec::new(),
        variables: std::collections::HashMap::new(),
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
    MirFunction {
        name: function.name.clone(),
        params,
        result: function.result.clone(),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instructions: builder.instructions,
            terminator: MirTerminator::Return(result),
        }],
    }
}

pub(crate) fn lower_hir(module: &HirModule) -> MirModule {
    MirModule {
        functions: module.functions.iter().map(lower_function).collect(),
    }
}
