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
    module
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
