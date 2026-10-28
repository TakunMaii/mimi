use crate::parser::{ASTNode, DataType, Expression};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HirType {
    Void,
    Integer {
        bits: u16,
        signed: bool,
    },
    Float {
        bits: u16,
    },
    Bool,
    Pointer(Box<HirType>),
    Array {
        length: u64,
        element: Box<HirType>,
    },
    Slice(Box<HirType>),
    Named(String),
    Function {
        params: Vec<HirType>,
        result: Box<HirType>,
    },
}

impl From<DataType> for HirType {
    fn from(value: DataType) -> Self {
        match value {
            DataType::Void => Self::Void,
            DataType::Bool => Self::Bool,
            DataType::U8 | DataType::U16 | DataType::U32 | DataType::U64 => Self::Integer {
                bits: value.bits(),
                signed: false,
            },
            DataType::I8 | DataType::I16 | DataType::I32 | DataType::I64 => Self::Integer {
                bits: value.bits(),
                signed: true,
            },
            DataType::F8 | DataType::F16 | DataType::F32 | DataType::F64 => {
                Self::Float { bits: value.bits() }
            }
            DataType::Pointer(inner) => Self::Pointer(Box::new((*inner).into())),
            DataType::Array(length, element) => Self::Array {
                length,
                element: Box::new((*element).into()),
            },
            DataType::Slice(element) => Self::Slice(Box::new((*element).into())),
            DataType::Struct(name) | DataType::Enum(name) => Self::Named(name.to_string()),
            DataType::Function(params, result) => Self::Function {
                params: params.iter().copied().map(Into::into).collect(),
                result: Box::new((*result).into()),
            },
            DataType::SelfType => Self::Named("Self".into()),
        }
    }
}

impl DataType {
    fn bits(self) -> u16 {
        match self {
            Self::U8 | Self::I8 | Self::F8 => 8,
            Self::U16 | Self::I16 | Self::F16 => 16,
            Self::U32 | Self::I32 | Self::F32 => 32,
            Self::U64 | Self::I64 | Self::F64 => 64,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirFunction {
    pub(crate) name: String,
    pub(crate) params: Vec<(String, HirType)>,
    pub(crate) result: HirType,
    pub(crate) body: Expression,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirExternalFunction {
    pub(crate) name: String,
    pub(crate) params: Vec<HirType>,
    pub(crate) result: HirType,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirEnum {
    pub(crate) name: String,
    pub(crate) variants: Vec<(String, Option<HirType>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirStruct {
    pub(crate) name: String,
    pub(crate) fields: Vec<(String, HirType, Option<Expression>)>,
    pub(crate) methods: Vec<(String, String, bool)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HirModule {
    pub(crate) functions: Vec<HirFunction>,
    pub(crate) extern_functions: Vec<HirExternalFunction>,
    pub(crate) enums: Vec<HirEnum>,
    pub(crate) structs: Vec<HirStruct>,
}

pub(crate) fn lower_ast(nodes: &[ASTNode]) -> HirModule {
    let mut module = HirModule::default();
    for node in nodes {
        let node = match node {
            ASTNode::Public(inner) => inner.as_ref(),
            node => node,
        };
        if let ASTNode::FunctionDefinition {
            name,
            parameters,
            return_type,
            body,
        } = node
        {
            module.functions.push(HirFunction {
                name: name.clone(),
                params: parameters
                    .iter()
                    .map(|(name, ty)| (name.clone(), (*ty).into()))
                    .collect(),
                result: (*return_type).into(),
                body: body.clone(),
            });
        }
        if let ASTNode::ExternalFunction {
            name,
            parameters,
            return_type,
        } = node
        {
            module.extern_functions.push(HirExternalFunction {
                name: name.clone(),
                params: parameters.iter().map(|(_, ty)| (*ty).into()).collect(),
                result: (*return_type).into(),
            });
        }
        if let ASTNode::EnumDefinition { name, variants } = node {
            module.enums.push(HirEnum {
                name: (*name).to_string(),
                variants: variants
                    .iter()
                    .map(|(variant, payload)| (variant.clone(), payload.map(Into::into)))
                    .collect(),
            });
        }
        if let ASTNode::StructDefinition {
            name,
            fields,
            methods,
        } = node
        {
            let mut method_symbols = Vec::new();
            for method in methods {
                let symbol = format!("mimi_{}__{}", name, method.name);
                let has_receiver = method
                    .parameters
                    .first()
                    .is_some_and(|(_, ty)| is_receiver_type(*ty, *name));
                module.functions.push(HirFunction {
                    name: symbol.clone(),
                    params: method
                        .parameters
                        .iter()
                        .map(|(parameter, ty)| (parameter.clone(), method_type(*ty, *name)))
                        .collect(),
                    result: method_type(method.return_type, *name),
                    body: method.body.clone(),
                });
                method_symbols.push((method.name.clone(), symbol, has_receiver));
            }
            module.structs.push(HirStruct {
                name: (*name).to_string(),
                fields: fields
                    .iter()
                    .map(|(field, data_type, default, _)| {
                        (field.clone(), (*data_type).into(), default.clone())
                    })
                    .collect(),
                methods: method_symbols,
            });
        }
    }

    let mut used_names = module
        .functions
        .iter()
        .map(|function| function.name.clone())
        .chain(
            module
                .extern_functions
                .iter()
                .map(|function| function.name.clone()),
        )
        .collect::<std::collections::HashSet<_>>();
    let mut next_lambda = 0;
    let mut index = 0;
    while index < module.functions.len() {
        let mut body = module.functions[index].body.clone();
        lift_lambdas_in_expression(
            &mut body,
            &mut module.functions,
            &mut used_names,
            &mut next_lambda,
        );
        module.functions[index].body = body;
        index += 1;
    }
    for structure in &mut module.structs {
        for (_, _, default) in &mut structure.fields {
            if let Some(default) = default {
                lift_lambdas_in_expression(
                    default,
                    &mut module.functions,
                    &mut used_names,
                    &mut next_lambda,
                );
            }
        }
    }
    module
}

fn lift_lambdas_in_expression(
    expression: &mut Expression,
    functions: &mut Vec<HirFunction>,
    used_names: &mut std::collections::HashSet<String>,
    next_lambda: &mut usize,
) {
    match expression {
        Expression::Negate(value)
        | Expression::Not(value)
        | Expression::AddressOf(value)
        | Expression::Dereference(value) => {
            lift_lambdas_in_expression(value, functions, used_names, next_lambda);
        }
        Expression::Add(left, right)
        | Expression::Subtract(left, right)
        | Expression::Multiply(left, right)
        | Expression::Divide(left, right)
        | Expression::Remainder(left, right)
        | Expression::Equal(left, right)
        | Expression::NotEqual(left, right)
        | Expression::LessThan(left, right)
        | Expression::LessThanOrEqual(left, right)
        | Expression::GreaterThan(left, right)
        | Expression::GreaterThanOrEqual(left, right)
        | Expression::And(left, right)
        | Expression::Or(left, right) => {
            lift_lambdas_in_expression(left, functions, used_names, next_lambda);
            lift_lambdas_in_expression(right, functions, used_names, next_lambda);
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            lift_lambdas_in_expression(condition, functions, used_names, next_lambda);
            lift_lambdas_in_expression(then_branch, functions, used_names, next_lambda);
            lift_lambdas_in_expression(else_branch, functions, used_names, next_lambda);
        }
        Expression::Block(items) => {
            for item in items {
                lift_lambdas_in_node(item, functions, used_names, next_lambda);
            }
        }
        Expression::Call { arguments, .. } | Expression::ArrayLiteral(arguments) => {
            for argument in arguments {
                lift_lambdas_in_expression(argument, functions, used_names, next_lambda);
            }
        }
        Expression::StructLiteral { fields, .. } => {
            for (_, value) in fields {
                lift_lambdas_in_expression(value, functions, used_names, next_lambda);
            }
        }
        Expression::MemberAccess { object, .. } => {
            lift_lambdas_in_expression(object, functions, used_names, next_lambda);
        }
        Expression::Index { object, index } => {
            lift_lambdas_in_expression(object, functions, used_names, next_lambda);
            lift_lambdas_in_expression(index, functions, used_names, next_lambda);
        }
        Expression::Slice { object, start, end } => {
            lift_lambdas_in_expression(object, functions, used_names, next_lambda);
            for bound in start.iter_mut().chain(end.iter_mut()) {
                lift_lambdas_in_expression(bound, functions, used_names, next_lambda);
            }
        }
        Expression::Match { value, arms } => {
            lift_lambdas_in_expression(value, functions, used_names, next_lambda);
            for (_, body) in arms {
                lift_lambdas_in_expression(body, functions, used_names, next_lambda);
            }
        }
        Expression::Lambda {
            parameters,
            return_type,
            body,
        } => {
            lift_lambdas_in_expression(body, functions, used_names, next_lambda);
            let name = loop {
                let candidate = format!("__mimi_lambda_{}", *next_lambda);
                *next_lambda += 1;
                if used_names.insert(candidate.clone()) {
                    break candidate;
                }
            };
            let parameters = parameters
                .iter()
                .enumerate()
                .map(|(index, (name, data_type))| {
                    (
                        name.clone()
                            .unwrap_or_else(|| format!("__mimi_lambda_arg_{index}")),
                        (*data_type).into(),
                    )
                })
                .collect();
            functions.push(HirFunction {
                name: name.clone(),
                params: parameters,
                result: (*return_type).into(),
                body: (**body).clone(),
            });
            *expression = Expression::Identifier(name);
        }
        Expression::CallValue { callee, arguments } => {
            lift_lambdas_in_expression(callee, functions, used_names, next_lambda);
            for argument in arguments {
                lift_lambdas_in_expression(argument, functions, used_names, next_lambda);
            }
        }
        Expression::Identifier(_)
        | Expression::LiteralNumber(_)
        | Expression::LiteralString(_)
        | Expression::LiteralBool(_)
        | Expression::StaticAccess { .. } => {}
    }
}

fn lift_lambdas_in_node(
    node: &mut ASTNode,
    functions: &mut Vec<HirFunction>,
    used_names: &mut std::collections::HashSet<String>,
    next_lambda: &mut usize,
) {
    match node {
        ASTNode::VariableDefinition { value, .. }
        | ASTNode::Assignment { value, .. }
        | ASTNode::CompileTimeConstant { value, .. }
        | ASTNode::Expression(value) => {
            lift_lambdas_in_expression(value, functions, used_names, next_lambda);
        }
        ASTNode::IndexAssignment {
            object,
            index,
            value,
        } => {
            lift_lambdas_in_expression(object, functions, used_names, next_lambda);
            lift_lambdas_in_expression(index, functions, used_names, next_lambda);
            lift_lambdas_in_expression(value, functions, used_names, next_lambda);
        }
        ASTNode::PointerAssignment { pointer, value } => {
            lift_lambdas_in_expression(pointer, functions, used_names, next_lambda);
            lift_lambdas_in_expression(value, functions, used_names, next_lambda);
        }
        ASTNode::MemberAssignment { object, value, .. } => {
            lift_lambdas_in_expression(object, functions, used_names, next_lambda);
            lift_lambdas_in_expression(value, functions, used_names, next_lambda);
        }
        ASTNode::FunctionDefinition { body, .. }
        | ASTNode::While { body, .. }
        | ASTNode::DoWhile { body, .. }
        | ASTNode::Loop { body }
        | ASTNode::Defer { body } => {
            lift_lambdas_in_expression(body, functions, used_names, next_lambda);
            if let ASTNode::While { condition, .. } | ASTNode::DoWhile { condition, .. } = node {
                lift_lambdas_in_expression(condition, functions, used_names, next_lambda);
            }
        }
        ASTNode::Return(value) => {
            if let Some(value) = value {
                lift_lambdas_in_expression(value, functions, used_names, next_lambda);
            }
        }
        ASTNode::If {
            condition,
            then_branch,
            else_branch,
        } => {
            lift_lambdas_in_expression(condition, functions, used_names, next_lambda);
            lift_lambdas_in_expression(then_branch, functions, used_names, next_lambda);
            if let Some(branch) = else_branch {
                lift_lambdas_in_expression(branch, functions, used_names, next_lambda);
            }
        }
        ASTNode::For { iterable, body, .. } => {
            lift_lambdas_in_expression(iterable, functions, used_names, next_lambda);
            lift_lambdas_in_expression(body, functions, used_names, next_lambda);
        }
        ASTNode::IfLet {
            value,
            then_branch,
            else_branch,
            ..
        } => {
            lift_lambdas_in_expression(value, functions, used_names, next_lambda);
            lift_lambdas_in_expression(then_branch, functions, used_names, next_lambda);
            if let Some(branch) = else_branch {
                lift_lambdas_in_expression(branch, functions, used_names, next_lambda);
            }
        }
        ASTNode::Public(inner) => {
            lift_lambdas_in_node(inner, functions, used_names, next_lambda);
        }
        ASTNode::Break
        | ASTNode::Continue
        | ASTNode::ExternalFunction { .. }
        | ASTNode::StructDefinition { .. }
        | ASTNode::EnumDefinition { .. }
        | ASTNode::Import { .. } => {}
    }
}

fn method_type(data_type: DataType, owner: &'static str) -> HirType {
    match data_type {
        DataType::SelfType => HirType::Named(owner.to_string()),
        DataType::Pointer(inner) => HirType::Pointer(Box::new(method_type(*inner, owner))),
        _ => data_type.into(),
    }
}

fn is_receiver_type(data_type: DataType, owner: &'static str) -> bool {
    match data_type {
        DataType::SelfType => true,
        DataType::Struct(name) => name == owner,
        DataType::Pointer(inner) => is_receiver_type(*inner, owner),
        _ => false,
    }
}
