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
    Tuple(Vec<HirType>),
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
            DataType::Struct(name) | DataType::Enum(name) | DataType::CEnum(name) => {
                Self::Named(name.to_string())
            }
            DataType::GenericStruct(name, arguments) | DataType::GenericEnum(name, arguments) => {
                Self::Named(crate::parser::generic_instance_name(name, arguments))
            }
            DataType::Function(params, result) => Self::Function {
                params: params.iter().copied().map(Into::into).collect(),
                result: Box::new((*result).into()),
            },
            DataType::Tuple(elements) => {
                Self::Tuple(elements.iter().copied().map(Into::into).collect())
            }
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
    pub(crate) captures: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirExternalFunction {
    pub(crate) name: String,
    pub(crate) link_name: String,
    pub(crate) params: Vec<HirType>,
    pub(crate) result: HirType,
    pub(crate) variadic: bool,
    pub(crate) calling_convention: String,
    pub(crate) parameter_qualifiers: Vec<Vec<String>>,
    pub(crate) return_qualifiers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirExternalGlobal {
    pub(crate) name: String,
    pub(crate) link_name: String,
    pub(crate) data_type: HirType,
    pub(crate) mutable: bool,
    pub(crate) thread_local: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirConstant {
    pub(crate) name: String,
    pub(crate) data_type: HirType,
    pub(crate) value: Expression,
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
    pub(crate) bound_methods: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirCRecordLayout {
    pub(crate) size_bytes: u64,
    pub(crate) align_bytes: u64,
    pub(crate) is_union: bool,
    pub(crate) field_offsets: Vec<(String, u64)>,
    pub(crate) bit_fields: Vec<(String, u64, u16)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HirCEnumRepresentation {
    pub(crate) underlying_type: HirType,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HirModule {
    pub(crate) functions: Vec<HirFunction>,
    pub(crate) extern_functions: Vec<HirExternalFunction>,
    pub(crate) extern_globals: Vec<HirExternalGlobal>,
    pub(crate) constants: Vec<HirConstant>,
    pub(crate) enums: Vec<HirEnum>,
    pub(crate) structs: Vec<HirStruct>,
    pub(crate) c_record_layouts: std::collections::HashMap<String, HirCRecordLayout>,
    pub(crate) c_enum_representations: std::collections::HashMap<String, HirCEnumRepresentation>,
    pub(crate) c_types: std::collections::HashMap<String, crate::c_type::CType>,
}

pub(crate) fn lower_ast(nodes: &[ASTNode]) -> HirModule {
    let mut module = HirModule::default();
    for node in nodes {
        if let ASTNode::CTypeInfo { name, c_type } = node {
            module.c_types.insert(name.clone(), c_type.clone());
        }
        if let ASTNode::CTypeEnumRepresentation {
            name,
            underlying_type,
        } = node
        {
            module.c_enum_representations.insert(
                name.clone(),
                HirCEnumRepresentation {
                    underlying_type: (*underlying_type).into(),
                },
            );
        }
        if let ASTNode::CTypeRecordLayout {
            name,
            size_bytes,
            align_bytes,
            is_union,
            field_offsets,
            bit_fields,
        } = node
        {
            module.c_record_layouts.insert(
                name.clone(),
                HirCRecordLayout {
                    size_bytes: *size_bytes,
                    align_bytes: *align_bytes,
                    is_union: *is_union,
                    field_offsets: field_offsets.clone(),
                    bit_fields: bit_fields.clone(),
                },
            );
        }
    }
    let mut constant_types = std::collections::HashMap::new();
    let enum_variants = nodes
        .iter()
        .flat_map(|node| {
            let node = match node {
                ASTNode::Public(declaration) => declaration.as_ref(),
                node => node,
            };
            match node {
                ASTNode::EnumDefinition { name, variants, .. } => variants
                    .iter()
                    .map(|(variant, payload)| {
                        (
                            variant.clone(),
                            (HirType::Named((*name).to_string()), payload.is_some()),
                        )
                    })
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            }
        })
        .collect::<std::collections::HashMap<_, _>>();
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
                captures: Vec::new(),
            });
        }
        if let ASTNode::ExternalFunction {
            name,
            link_name,
            parameters,
            return_type,
            variadic,
            calling_convention,
            parameter_qualifiers,
            return_qualifiers,
        } = node
        {
            module.extern_functions.push(HirExternalFunction {
                name: name.clone(),
                link_name: link_name.clone().unwrap_or_else(|| name.clone()),
                params: parameters.iter().map(|(_, ty)| (*ty).into()).collect(),
                result: (*return_type).into(),
                variadic: *variadic,
                calling_convention: calling_convention.clone(),
                parameter_qualifiers: parameter_qualifiers.clone(),
                return_qualifiers: return_qualifiers.clone(),
            });
        }
        if let ASTNode::ExternalGlobal {
            name,
            link_name,
            data_type,
            mutable,
            thread_local,
        } = node
        {
            module.extern_globals.push(HirExternalGlobal {
                name: name.clone(),
                link_name: link_name.clone().unwrap_or_else(|| name.clone()),
                data_type: (*data_type).into(),
                mutable: *mutable,
                thread_local: *thread_local,
            });
        }
        if let ASTNode::CompileTimeConstant {
            name,
            data_type,
            value,
        } = node
        {
            let namespace = name.rsplit_once('.').map(|(namespace, _)| namespace);
            let data_type = data_type.map(Into::into).or_else(|| {
                infer_constant_type(value, None, &constant_types, namespace, &enum_variants)
            });
            if let Some(data_type) = data_type.filter(is_constant_type)
                && is_constant_expression(
                    value,
                    &constant_types,
                    namespace,
                    &enum_variants,
                    Some(&data_type),
                )
            {
                module.constants.push(HirConstant {
                    name: name.clone(),
                    data_type: data_type.clone(),
                    value: value.clone(),
                });
                constant_types.insert(name.clone(), data_type);
            }
        }
        if let ASTNode::EnumDefinition { name, variants, .. } = node {
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
            ..
        } = node
        {
            let mut method_symbols = Vec::new();
            let mut bound_methods = Vec::new();
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
                    captures: Vec::new(),
                });
                if has_receiver {
                    let bound_symbol = format!("mimi_bound_{}__{}", name, method.name);
                    let receiver_type = method_type(method.parameters[0].1, *name);
                    let mut bound_params = method
                        .parameters
                        .iter()
                        .skip(1)
                        .map(|(parameter, ty)| (parameter.clone(), method_type(*ty, *name)))
                        .collect::<Vec<_>>();
                    let receiver_name = (0..)
                        .map(|index| format!("__mimi_bound_receiver_{index}"))
                        .find(|candidate| {
                            method
                                .parameters
                                .iter()
                                .all(|(parameter, _)| parameter != candidate)
                        })
                        .expect("generated receiver name must be available");
                    let mut arguments = vec![Expression::Identifier(receiver_name.clone())];
                    arguments.extend(
                        method
                            .parameters
                            .iter()
                            .skip(1)
                            .map(|(parameter, _)| Expression::Identifier(parameter.clone())),
                    );
                    bound_params.push((receiver_name, receiver_type));
                    module.functions.push(HirFunction {
                        name: bound_symbol.clone(),
                        params: bound_params,
                        result: method_type(method.return_type, *name),
                        body: Expression::Call {
                            name: symbol.clone(),
                            arguments,
                        },
                        captures: Vec::new(),
                    });
                    bound_methods.push((method.name.clone(), bound_symbol));
                }
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
                bound_methods,
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
    for (template_name, arguments) in generic_applications(nodes) {
        let instance_name = crate::parser::generic_instance_name(&template_name, &arguments);
        if module.structs.iter().any(|item| item.name == instance_name)
            || module.enums.iter().any(|item| item.name == instance_name)
        {
            continue;
        }
        let Some(template) = nodes.iter().find_map(|node| match node {
            ASTNode::StructDefinition {
                name,
                parameters,
                fields,
                ..
            } if *name == template_name => Some((parameters, fields)),
            _ => None,
        }) else {
            if let Some((parameters, variants)) = nodes.iter().find_map(|node| match node {
                ASTNode::EnumDefinition {
                    name,
                    parameters,
                    variants,
                } if *name == template_name => Some((parameters, variants)),
                _ => None,
            }) {
                if parameters.len() != arguments.len() {
                    continue;
                }
                let parameter_types = generic_enum_parameter_types(parameters, variants);
                let types = generic_type_arguments(parameters, &arguments, &parameter_types);
                module.enums.push(HirEnum {
                    name: instance_name,
                    variants: variants
                        .iter()
                        .map(|(variant, payload)| {
                            (
                                variant.clone(),
                                payload.map(|ty| substitute_generic_data_type(ty, &types).into()),
                            )
                        })
                        .collect(),
                });
            }
            continue;
        };
        let (parameters, fields) = template;
        if parameters.len() != arguments.len() {
            continue;
        }
        let parameter_types = generic_struct_parameter_types(parameters, fields);
        let types = generic_type_arguments(parameters, &arguments, &parameter_types);
        let values = generic_value_arguments(parameters, &arguments, &parameter_types);
        module.structs.push(HirStruct {
            name: instance_name,
            fields: fields
                .iter()
                .map(|(field, ty, default, _)| {
                    (
                        field.clone(),
                        substitute_generic_data_type(*ty, &types).into(),
                        default
                            .as_ref()
                            .map(|value| substitute_generic_expression(value, &values)),
                    )
                })
                .collect(),
            methods: Vec::new(),
            bound_methods: Vec::new(),
        });
    }
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
    module
}

fn infer_constant_type(
    expression: &Expression,
    expected: Option<&HirType>,
    constants: &std::collections::HashMap<String, HirType>,
    namespace: Option<&str>,
    enum_variants: &std::collections::HashMap<String, (HirType, bool)>,
) -> Option<HirType> {
    match expression {
        Expression::LiteralBool(_) => Some(HirType::Bool),
        Expression::LiteralString(_) => expected
            .filter(|ty| matches!(ty, HirType::Slice(inner) if **inner == HirType::Integer { bits: 8, signed: false }))
            .cloned()
            .or_else(|| {
                Some(HirType::Slice(Box::new(HirType::Integer {
                    bits: 8,
                    signed: false,
                })))
            }),
        Expression::LiteralNumber(text) => expected
            .filter(|data_type| is_scalar_numeric_type(data_type))
            .cloned()
            .or_else(|| inferred_integer_type(text))
            .or_else(|| text.contains('.').then_some(HirType::Float { bits: 64 })),
        Expression::Identifier(name) => constants
            .get(name)
            .cloned()
            .or_else(|| {
                namespace
                    .and_then(|namespace| constants.get(&format!("{namespace}.{name}")))
                    .cloned()
            })
            .or_else(|| {
                enum_variants
                    .get(name)
                    .filter(|(_, has_payload)| !has_payload)
                    .map(|(owner, _)| owner.clone())
            }),
        Expression::Negate(inner) => match inner.as_ref() {
            Expression::LiteralNumber(text) if expected.is_none() => {
                inferred_negative_integer_type(text)
                    .or_else(|| text.contains('.').then_some(HirType::Float { bits: 64 }))
            }
            _ => infer_constant_type(inner, expected, constants, namespace, enum_variants)
                .filter(is_scalar_numeric_type),
        },
        Expression::Not(_) | Expression::And(_, _) | Expression::Or(_, _) => Some(HirType::Bool),
        Expression::Equal(_, _)
        | Expression::NotEqual(_, _)
        | Expression::LessThan(_, _)
        | Expression::LessThanOrEqual(_, _)
        | Expression::GreaterThan(_, _)
        | Expression::GreaterThanOrEqual(_, _) => Some(HirType::Bool),
        Expression::Add(lhs, rhs)
        | Expression::Subtract(lhs, rhs)
        | Expression::Multiply(lhs, rhs)
        | Expression::Divide(lhs, rhs)
        | Expression::Remainder(lhs, rhs) => {
            let lhs_type = infer_constant_type(lhs, expected, constants, namespace, enum_variants)?;
            let rhs_type =
                infer_constant_type(rhs, Some(&lhs_type), constants, namespace, enum_variants)
                    .or_else(|| {
                        infer_constant_type(rhs, expected, constants, namespace, enum_variants)
                    })?;
            common_numeric_type(&lhs_type, &rhs_type)
        }
        Expression::StructLiteral { type_name, .. } => type_name
            .map(|name| HirType::Named(name.to_string()))
            .or_else(|| {
                expected
                    .cloned()
                    .filter(|ty| matches!(ty, HirType::Named(_)))
            }),
        Expression::ArrayLiteral(values) => {
            if let Some(array_type @ HirType::Array { .. }) = expected {
                return Some(array_type.clone());
            }
            let element_type = values.first().and_then(|first| {
                infer_constant_type(first, None, constants, namespace, enum_variants)
            })?;
            Some(HirType::Array {
                length: values.len() as u64,
                element: Box::new(element_type),
            })
        }
        Expression::TupleLiteral(values) => {
            if let Some(HirType::Tuple(elements)) = expected
                && elements.len() == values.len()
            {
                return Some(HirType::Tuple(elements.clone()));
            }
            Some(HirType::Tuple(
                values
                    .iter()
                    .map(|value| {
                        infer_constant_type(value, None, constants, namespace, enum_variants)
                    })
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        Expression::Call { name, .. } => enum_variants.get(name).map(|(owner, _)| owner.clone()),
        Expression::If {
            then_branch,
            else_branch,
            ..
        } => {
            let then_type =
                infer_constant_type(then_branch, expected, constants, namespace, enum_variants)?;
            let else_type = infer_constant_type(
                else_branch,
                Some(&then_type),
                constants,
                namespace,
                enum_variants,
            )
            .or_else(|| {
                infer_constant_type(else_branch, expected, constants, namespace, enum_variants)
            })?;
            (then_type == else_type)
                .then_some(then_type.clone())
                .or_else(|| common_numeric_type(&then_type, &else_type))
        }
        _ => None,
    }
}

fn is_constant_expression(
    expression: &Expression,
    constants: &std::collections::HashMap<String, HirType>,
    namespace: Option<&str>,
    enum_variants: &std::collections::HashMap<String, (HirType, bool)>,
    expected: Option<&HirType>,
) -> bool {
    match expression {
        Expression::LiteralBool(_) | Expression::LiteralNumber(_) => true,
        Expression::LiteralString(_) => {
            matches!(expected, Some(HirType::Slice(inner)) if **inner == HirType::Integer { bits: 8, signed: false })
        }
        Expression::Identifier(name) => {
            constants.contains_key(name)
                || namespace
                    .is_some_and(|namespace| constants.contains_key(&format!("{namespace}.{name}")))
                || enum_variants
                    .get(name)
                    .is_some_and(|(_, has_payload)| !has_payload)
        }
        Expression::Negate(inner) | Expression::Not(inner) => {
            is_constant_expression(inner, constants, namespace, enum_variants, None)
        }
        Expression::Add(lhs, rhs)
        | Expression::Subtract(lhs, rhs)
        | Expression::Multiply(lhs, rhs)
        | Expression::Divide(lhs, rhs)
        | Expression::Remainder(lhs, rhs)
        | Expression::Equal(lhs, rhs)
        | Expression::NotEqual(lhs, rhs)
        | Expression::LessThan(lhs, rhs)
        | Expression::LessThanOrEqual(lhs, rhs)
        | Expression::GreaterThan(lhs, rhs)
        | Expression::GreaterThanOrEqual(lhs, rhs)
        | Expression::And(lhs, rhs)
        | Expression::Or(lhs, rhs) => {
            is_constant_expression(lhs, constants, namespace, enum_variants, None)
                && is_constant_expression(rhs, constants, namespace, enum_variants, None)
        }
        Expression::StructLiteral { fields, .. } => fields.iter().all(|(_, value)| {
            is_constant_expression(value, constants, namespace, enum_variants, None)
        }),
        Expression::ArrayLiteral(values) => {
            if matches!(expected, Some(HirType::Slice(_))) {
                return false;
            }
            let element_type = match expected {
                Some(HirType::Array { element, .. }) => Some(element.as_ref()),
                _ => None,
            };
            values.iter().all(|value| {
                is_constant_expression(value, constants, namespace, enum_variants, element_type)
            })
        }
        Expression::TupleLiteral(values) => {
            let expected_elements = match expected {
                Some(HirType::Tuple(elements)) if elements.len() == values.len() => {
                    Some(elements.as_slice())
                }
                Some(HirType::Tuple(_)) => return false,
                _ => None,
            };
            values.iter().enumerate().all(|(index, value)| {
                is_constant_expression(
                    value,
                    constants,
                    namespace,
                    enum_variants,
                    expected_elements.map(|elements| &elements[index]),
                )
            })
        }
        Expression::Call { name, arguments } => {
            enum_variants.contains_key(name)
                && arguments.iter().all(|argument| {
                    is_constant_expression(argument, constants, namespace, enum_variants, None)
                })
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            is_constant_expression(condition, constants, namespace, enum_variants, None)
                && is_constant_expression(
                    then_branch,
                    constants,
                    namespace,
                    enum_variants,
                    expected,
                )
                && is_constant_expression(
                    else_branch,
                    constants,
                    namespace,
                    enum_variants,
                    expected,
                )
        }
        _ => false,
    }
}

fn is_constant_type(data_type: &HirType) -> bool {
    matches!(
        data_type,
        HirType::Integer { .. }
            | HirType::Float {
                bits: 8 | 16 | 32 | 64
            }
            | HirType::Bool
            | HirType::Named(_)
    ) || matches!(data_type, HirType::Array { element, .. } if !matches!(element.as_ref(), HirType::Slice(_)) && is_constant_type(element))
        || matches!(data_type, HirType::Tuple(elements) if elements.iter().all(is_constant_type))
        || matches!(data_type, HirType::Slice(inner) if **inner == HirType::Integer { bits: 8, signed: false })
}

fn is_scalar_numeric_type(data_type: &HirType) -> bool {
    matches!(data_type, HirType::Integer { .. } | HirType::Float { .. })
}

fn common_numeric_type(lhs: &HirType, rhs: &HirType) -> Option<HirType> {
    match (lhs, rhs) {
        (HirType::Float { bits: lhs }, HirType::Float { bits: rhs }) => Some(HirType::Float {
            bits: (*lhs).max(*rhs),
        }),
        (HirType::Float { bits }, HirType::Integer { .. })
        | (HirType::Integer { .. }, HirType::Float { bits }) => {
            Some(HirType::Float { bits: *bits })
        }
        (
            HirType::Integer {
                bits: lhs_bits,
                signed: lhs_signed,
            },
            HirType::Integer {
                bits: rhs_bits,
                signed: rhs_signed,
            },
        ) if lhs_signed == rhs_signed => Some(HirType::Integer {
            bits: (*lhs_bits).max(*rhs_bits),
            signed: *lhs_signed,
        }),
        (
            HirType::Integer {
                bits: signed_bits,
                signed: true,
            },
            HirType::Integer {
                bits: unsigned_bits,
                signed: false,
            },
        )
        | (
            HirType::Integer {
                bits: unsigned_bits,
                signed: false,
            },
            HirType::Integer {
                bits: signed_bits,
                signed: true,
            },
        ) if signed_bits > unsigned_bits => Some(HirType::Integer {
            bits: *signed_bits,
            signed: true,
        }),
        _ => None,
    }
}

fn inferred_integer_type(text: &str) -> Option<HirType> {
    let value = text.parse::<u128>().ok()?;
    if value <= i32::MAX as u128 {
        Some(HirType::Integer {
            bits: 32,
            signed: true,
        })
    } else if value <= i64::MAX as u128 {
        Some(HirType::Integer {
            bits: 64,
            signed: true,
        })
    } else if value <= u64::MAX as u128 {
        Some(HirType::Integer {
            bits: 64,
            signed: false,
        })
    } else {
        None
    }
}

fn inferred_negative_integer_type(text: &str) -> Option<HirType> {
    let magnitude = text.parse::<i128>().ok()?;
    let value = magnitude.checked_neg()?;
    if value >= i32::MIN as i128 {
        Some(HirType::Integer {
            bits: 32,
            signed: true,
        })
    } else if value >= i64::MIN as i128 {
        Some(HirType::Integer {
            bits: 64,
            signed: true,
        })
    } else {
        None
    }
}

fn generic_applications(
    nodes: &[ASTNode],
) -> Vec<(String, &'static [crate::parser::GenericArgument])> {
    fn add_type(
        data_type: DataType,
        output: &mut Vec<(String, &'static [crate::parser::GenericArgument])>,
    ) {
        match data_type {
            DataType::GenericStruct(name, args) | DataType::GenericEnum(name, args) => {
                let instance = crate::parser::generic_instance_name(name, args);
                if !output.iter().any(|(existing, _)| *existing == instance) {
                    output.push((name.to_string(), args));
                }
                for arg in args {
                    if let crate::parser::GenericArgument::Type(data_type) = arg {
                        add_type(*data_type, output);
                    }
                }
            }
            DataType::Array(_, inner) | DataType::Slice(inner) | DataType::Pointer(inner) => {
                add_type(*inner, output)
            }
            DataType::Function(params, result) => {
                for parameter in params {
                    add_type(*parameter, output);
                }
                add_type(*result, output);
            }
            DataType::Tuple(elements) => {
                for element in elements {
                    add_type(*element, output);
                }
            }
            _ => {}
        }
    }
    fn visit_expr(
        expression: &Expression,
        output: &mut Vec<(String, &'static [crate::parser::GenericArgument])>,
    ) {
        match expression {
            Expression::StructLiteral { type_name, fields } => {
                if let Some(name) = type_name
                    && let Some((base, arguments)) =
                        crate::parser::decode_generic_instance_name(name)
                {
                    let instance = crate::parser::generic_instance_name(base, arguments);
                    if !output.iter().any(|(existing, _)| *existing == instance) {
                        output.push((base.to_string(), arguments));
                    }
                }
                for (_, value) in fields {
                    visit_expr(value, output);
                }
            }
            Expression::Negate(value)
            | Expression::Not(value)
            | Expression::AddressOf(value)
            | Expression::Dereference(value) => visit_expr(value, output),
            Expression::Add(a, b)
            | Expression::Subtract(a, b)
            | Expression::Multiply(a, b)
            | Expression::Divide(a, b)
            | Expression::Remainder(a, b)
            | Expression::Equal(a, b)
            | Expression::NotEqual(a, b)
            | Expression::LessThan(a, b)
            | Expression::LessThanOrEqual(a, b)
            | Expression::GreaterThan(a, b)
            | Expression::GreaterThanOrEqual(a, b)
            | Expression::And(a, b)
            | Expression::Or(a, b) => {
                visit_expr(a, output);
                visit_expr(b, output);
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
            } => {
                visit_expr(condition, output);
                visit_expr(then_branch, output);
                visit_expr(else_branch, output);
            }
            Expression::Block(items) => visit_nodes(items, output),
            Expression::Call { arguments, .. }
            | Expression::CallValue { arguments, .. }
            | Expression::ArrayLiteral(arguments)
            | Expression::TupleLiteral(arguments) => {
                for argument in arguments {
                    visit_expr(argument, output);
                }
            }
            Expression::MemberAccess { object, .. } => visit_expr(object, output),
            Expression::Index { object, index } => {
                visit_expr(object, output);
                visit_expr(index, output);
            }
            Expression::Slice { object, start, end } => {
                visit_expr(object, output);
                for bound in start.iter().chain(end.iter()) {
                    visit_expr(bound, output);
                }
            }
            Expression::Match { value, arms } => {
                visit_expr(value, output);
                for (_, body) in arms {
                    visit_expr(body, output);
                }
            }
            Expression::Lambda { body, .. } => visit_expr(body, output),
            Expression::Identifier(_)
            | Expression::LiteralNumber(_)
            | Expression::LiteralString(_)
            | Expression::LiteralBool(_)
            | Expression::StaticAccess { .. } => {}
        }
    }
    fn visit_nodes(
        nodes: &[ASTNode],
        output: &mut Vec<(String, &'static [crate::parser::GenericArgument])>,
    ) {
        for node in nodes {
            match node {
                ASTNode::VariableDefinition {
                    data_type, value, ..
                }
                | ASTNode::CompileTimeConstant {
                    data_type, value, ..
                } => {
                    if let Some(data_type) = data_type {
                        add_type(*data_type, output);
                    }
                    visit_expr(value, output);
                }
                ASTNode::TupleDestructuring { value, .. } => visit_expr(value, output),
                ASTNode::Assignment { value, .. } | ASTNode::Expression(value) => {
                    visit_expr(value, output)
                }
                ASTNode::IndexAssignment {
                    object,
                    index,
                    value,
                } => {
                    visit_expr(object, output);
                    visit_expr(index, output);
                    visit_expr(value, output);
                }
                ASTNode::PointerAssignment { pointer, value } => {
                    visit_expr(pointer, output);
                    visit_expr(value, output);
                }
                ASTNode::MemberAssignment { object, value, .. } => {
                    visit_expr(object, output);
                    visit_expr(value, output);
                }
                ASTNode::FunctionDefinition {
                    parameters,
                    return_type,
                    body,
                    ..
                } => {
                    for (_, ty) in parameters {
                        add_type(*ty, output);
                    }
                    add_type(*return_type, output);
                    visit_expr(body, output);
                }
                ASTNode::ExternalFunction {
                    parameters,
                    return_type,
                    ..
                } => {
                    for (_, ty) in parameters {
                        add_type(*ty, output);
                    }
                    add_type(*return_type, output);
                }
                ASTNode::Return(value) => {
                    if let Some(value) = value {
                        visit_expr(value, output);
                    }
                }
                ASTNode::StructDefinition {
                    fields, methods, ..
                } => {
                    for (_, ty, default, _) in fields {
                        add_type(*ty, output);
                        if let Some(default) = default {
                            visit_expr(default, output);
                        }
                    }
                    for method in methods {
                        for (_, ty) in &method.parameters {
                            add_type(*ty, output);
                        }
                        add_type(method.return_type, output);
                        visit_expr(&method.body, output);
                    }
                }
                ASTNode::EnumDefinition { variants, .. } => {
                    for (_, payload) in variants {
                        if let Some(payload) = payload {
                            add_type(*payload, output);
                        }
                    }
                }
                ASTNode::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    visit_expr(condition, output);
                    visit_expr(then_branch, output);
                    if let Some(branch) = else_branch {
                        visit_expr(branch, output);
                    }
                }
                ASTNode::While { condition, body } | ASTNode::DoWhile { condition, body } => {
                    visit_expr(condition, output);
                    visit_expr(body, output);
                }
                ASTNode::For { iterable, body, .. } => {
                    visit_expr(iterable, output);
                    visit_expr(body, output);
                }
                ASTNode::Loop { body } | ASTNode::Defer { body } => visit_expr(body, output),
                ASTNode::IfLet {
                    value,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    visit_expr(value, output);
                    visit_expr(then_branch, output);
                    if let Some(branch) = else_branch {
                        visit_expr(branch, output);
                    }
                }
                ASTNode::Public(inner) => visit_nodes(std::slice::from_ref(inner), output),
                ASTNode::Break
                | ASTNode::Continue
                | ASTNode::Import { .. }
                | ASTNode::CImport { .. }
                | ASTNode::CDefine { .. }
                | ASTNode::CTypeAlias { .. }
                | ASTNode::CTypeEnumRepresentation { .. }
                | ASTNode::CTypeInfo { .. }
                | ASTNode::ExternalGlobal { .. }
                | ASTNode::CTypeRecordLayout { .. } => {}
            }
        }
    }
    let mut output = Vec::new();
    visit_nodes(nodes, &mut output);
    output
}

fn generic_type_arguments(
    parameters: &[String],
    arguments: &[crate::parser::GenericArgument],
    parameter_types: &[bool],
) -> std::collections::HashMap<String, DataType> {
    parameters
        .iter()
        .zip(arguments)
        .enumerate()
        .filter_map(|(index, (name, argument))| match argument {
            crate::parser::GenericArgument::Type(data_type) => Some((name.clone(), *data_type)),
            crate::parser::GenericArgument::Name(type_name)
                if parameter_types.get(index).copied().unwrap_or(false) =>
            {
                Some((name.clone(), DataType::Struct(type_name)))
            }
            _ => None,
        })
        .collect()
}

fn generic_value_arguments(
    parameters: &[String],
    arguments: &[crate::parser::GenericArgument],
    parameter_types: &[bool],
) -> std::collections::HashMap<String, Expression> {
    parameters
        .iter()
        .zip(arguments)
        .enumerate()
        .filter_map(|(index, (name, argument))| match argument {
            crate::parser::GenericArgument::Value(value) => Some((name.clone(), (**value).clone())),
            crate::parser::GenericArgument::Name(value)
                if !parameter_types.get(index).copied().unwrap_or(false) =>
            {
                Some((name.clone(), Expression::Identifier(value.to_string())))
            }
            _ => None,
        })
        .collect()
}

fn generic_struct_parameter_types(
    parameters: &[String],
    fields: &[(String, DataType, Option<Expression>, bool)],
) -> Vec<bool> {
    parameters
        .iter()
        .map(|parameter| {
            fields
                .iter()
                .any(|(_, data_type, _, _)| hir_data_type_contains(*data_type, parameter))
                || parameter.chars().next().is_some_and(char::is_uppercase)
        })
        .collect()
}

fn generic_enum_parameter_types(
    parameters: &[String],
    variants: &[(String, Option<DataType>)],
) -> Vec<bool> {
    parameters
        .iter()
        .map(|parameter| {
            variants
                .iter()
                .filter_map(|(_, payload)| *payload)
                .any(|payload| hir_data_type_contains(payload, parameter))
                || parameter.chars().next().is_some_and(char::is_uppercase)
        })
        .collect()
}

fn hir_data_type_contains(data_type: DataType, parameter: &str) -> bool {
    match data_type {
        DataType::Struct(name) => name == parameter,
        DataType::Array(_, inner) | DataType::Slice(inner) | DataType::Pointer(inner) => {
            hir_data_type_contains(*inner, parameter)
        }
        DataType::Function(parameters, result) => {
            parameters
                .iter()
                .any(|ty| hir_data_type_contains(*ty, parameter))
                || hir_data_type_contains(*result, parameter)
        }
        DataType::Tuple(elements) => elements
            .iter()
            .any(|element| hir_data_type_contains(*element, parameter)),
        _ => false,
    }
}

fn substitute_generic_data_type(
    data_type: DataType,
    arguments: &std::collections::HashMap<String, DataType>,
) -> DataType {
    match data_type {
        DataType::Struct(name) => arguments.get(name).copied().unwrap_or(data_type),
        DataType::Array(length, inner) => DataType::Array(
            length,
            Box::leak(Box::new(substitute_generic_data_type(*inner, arguments))),
        ),
        DataType::Slice(inner) => DataType::Slice(Box::leak(Box::new(
            substitute_generic_data_type(*inner, arguments),
        ))),
        DataType::Pointer(inner) => DataType::Pointer(Box::leak(Box::new(
            substitute_generic_data_type(*inner, arguments),
        ))),
        DataType::Function(params, result) => DataType::Function(
            Box::leak(
                params
                    .iter()
                    .map(|param| substitute_generic_data_type(*param, arguments))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            Box::leak(Box::new(substitute_generic_data_type(*result, arguments))),
        ),
        DataType::Tuple(elements) => DataType::Tuple(Box::leak(
            elements
                .iter()
                .map(|element| substitute_generic_data_type(*element, arguments))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )),
        _ => data_type,
    }
}

fn substitute_generic_expression(
    expression: &Expression,
    arguments: &std::collections::HashMap<String, Expression>,
) -> Expression {
    match expression {
        Expression::Identifier(name) => arguments
            .get(name)
            .cloned()
            .unwrap_or_else(|| expression.clone()),
        _ => expression.clone(),
    }
}

fn collect_free_names(
    expression: &Expression,
    bound: &mut std::collections::HashSet<String>,
    captures: &mut Vec<String>,
) {
    fn add(name: &str, bound: &std::collections::HashSet<String>, captures: &mut Vec<String>) {
        if !bound.contains(name) && !captures.iter().any(|capture| capture == name) {
            captures.push(name.to_string());
        }
    }
    fn node(
        item: &ASTNode,
        bound: &mut std::collections::HashSet<String>,
        captures: &mut Vec<String>,
    ) {
        match item {
            ASTNode::VariableDefinition { name, value, .. }
            | ASTNode::CompileTimeConstant { name, value, .. } => {
                collect_free_names(value, bound, captures);
                bound.insert(name.clone());
            }
            ASTNode::TupleDestructuring { names, value } => {
                collect_free_names(value, bound, captures);
                bound.extend(names.iter().cloned());
            }
            ASTNode::Assignment { name, value } => {
                add(name, bound, captures);
                collect_free_names(value, bound, captures);
            }
            ASTNode::IndexAssignment {
                object,
                index,
                value,
            } => {
                collect_free_names(object, bound, captures);
                collect_free_names(index, bound, captures);
                collect_free_names(value, bound, captures);
            }
            ASTNode::PointerAssignment { pointer, value } => {
                collect_free_names(pointer, bound, captures);
                collect_free_names(value, bound, captures);
            }
            ASTNode::MemberAssignment { object, value, .. } => {
                collect_free_names(object, bound, captures);
                collect_free_names(value, bound, captures);
            }
            ASTNode::Expression(value) => collect_free_names(value, bound, captures),
            ASTNode::Return(value) => {
                if let Some(value) = value {
                    collect_free_names(value, bound, captures);
                }
            }
            ASTNode::FunctionDefinition { .. }
            | ASTNode::ExternalFunction { .. }
            | ASTNode::Break
            | ASTNode::Continue
            | ASTNode::StructDefinition { .. }
            | ASTNode::EnumDefinition { .. }
            | ASTNode::Import { .. }
            | ASTNode::CImport { .. }
            | ASTNode::CDefine { .. }
            | ASTNode::CTypeAlias { .. }
            | ASTNode::CTypeEnumRepresentation { .. }
            | ASTNode::ExternalGlobal { .. }
            | ASTNode::CTypeRecordLayout { .. } => {}
            ASTNode::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_free_names(condition, bound, captures);
                collect_free_names(then_branch, &mut bound.clone(), captures);
                if let Some(branch) = else_branch {
                    collect_free_names(branch, &mut bound.clone(), captures);
                }
            }
            ASTNode::While { condition, body } | ASTNode::DoWhile { body, condition } => {
                collect_free_names(condition, bound, captures);
                collect_free_names(body, &mut bound.clone(), captures);
            }
            ASTNode::For {
                binding,
                iterable,
                body,
            } => {
                collect_free_names(iterable, bound, captures);
                let mut nested = bound.clone();
                nested.insert(binding.clone());
                collect_free_names(body, &mut nested, captures);
            }
            ASTNode::Loop { body } | ASTNode::Defer { body } => {
                collect_free_names(body, &mut bound.clone(), captures);
            }
            ASTNode::IfLet {
                binding,
                value,
                then_branch,
                else_branch,
                ..
            } => {
                collect_free_names(value, bound, captures);
                let mut nested = bound.clone();
                if let Some(binding) = binding {
                    nested.insert(binding.clone());
                }
                collect_free_names(then_branch, &mut nested, captures);
                if let Some(branch) = else_branch {
                    collect_free_names(branch, &mut bound.clone(), captures);
                }
            }
            ASTNode::Public(inner) => node(inner, bound, captures),
            ASTNode::CTypeInfo { .. } => {}
        }
    }
    match expression {
        Expression::Identifier(name) => add(name, bound, captures),
        Expression::Negate(value)
        | Expression::Not(value)
        | Expression::AddressOf(value)
        | Expression::Dereference(value) => collect_free_names(value, bound, captures),
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
            collect_free_names(left, bound, captures);
            collect_free_names(right, bound, captures);
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_free_names(condition, bound, captures);
            collect_free_names(then_branch, &mut bound.clone(), captures);
            collect_free_names(else_branch, &mut bound.clone(), captures);
        }
        Expression::Block(items) => {
            let mut nested = bound.clone();
            for item in items {
                node(item, &mut nested, captures);
            }
        }
        Expression::Call { name, arguments } => {
            add(name, bound, captures);
            for argument in arguments {
                collect_free_names(argument, bound, captures);
            }
        }
        Expression::CallValue { callee, arguments } => {
            collect_free_names(callee, bound, captures);
            for argument in arguments {
                collect_free_names(argument, bound, captures);
            }
        }
        Expression::StructLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_free_names(value, bound, captures);
            }
        }
        Expression::MemberAccess { object, .. } => collect_free_names(object, bound, captures),
        Expression::ArrayLiteral(values) | Expression::TupleLiteral(values) => {
            for value in values {
                collect_free_names(value, bound, captures);
            }
        }
        Expression::Index { object, index } => {
            collect_free_names(object, bound, captures);
            collect_free_names(index, bound, captures);
        }
        Expression::Slice { object, start, end } => {
            collect_free_names(object, bound, captures);
            for value in start.iter().chain(end.iter()) {
                collect_free_names(value, bound, captures);
            }
        }
        Expression::Match { value, arms } => {
            collect_free_names(value, bound, captures);
            for (pattern, body) in arms {
                let mut nested = bound.clone();
                if let crate::parser::Pattern::Variant {
                    binding: Some(binding),
                    ..
                } = pattern
                {
                    nested.insert(binding.clone());
                }
                collect_free_names(body, &mut nested, captures);
            }
        }
        Expression::Lambda {
            parameters, body, ..
        } => {
            let mut nested = bound.clone();
            nested.extend(parameters.iter().filter_map(|(name, _)| name.clone()));
            collect_free_names(body, &mut nested, captures);
        }
        Expression::LiteralNumber(_)
        | Expression::LiteralString(_)
        | Expression::LiteralBool(_)
        | Expression::StaticAccess { .. } => {}
    }
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
        Expression::Call { arguments, .. }
        | Expression::ArrayLiteral(arguments)
        | Expression::TupleLiteral(arguments) => {
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
            let name = loop {
                let candidate = format!("__mimi_lambda_{}", *next_lambda);
                *next_lambda += 1;
                if used_names.insert(candidate.clone()) {
                    break candidate;
                }
            };
            let parameters: Vec<(String, HirType)> = parameters
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
            let mut bound = parameters.iter().map(|(name, _)| name.clone()).collect();
            let mut captures = Vec::new();
            collect_free_names(body, &mut bound, &mut captures);
            functions.push(HirFunction {
                name: name.clone(),
                params: parameters,
                result: (*return_type).into(),
                body: (**body).clone(),
                captures,
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
        | ASTNode::TupleDestructuring { value, .. }
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
        | ASTNode::Import { .. }
        | ASTNode::CImport { .. }
        | ASTNode::CDefine { .. }
        | ASTNode::CTypeAlias { .. }
        | ASTNode::CTypeEnumRepresentation { .. }
        | ASTNode::CTypeInfo { .. }
        | ASTNode::ExternalGlobal { .. }
        | ASTNode::CTypeRecordLayout { .. } => {}
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
