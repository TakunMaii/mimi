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

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HirModule {
    pub(crate) functions: Vec<HirFunction>,
    pub(crate) extern_functions: Vec<HirExternalFunction>,
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
    }
    module
}
