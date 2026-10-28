use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::lexer::tokenize;
use crate::parser::{ASTNode, DataType, Expression, Pattern};

const LOOP_CONTEXT: &str = "\0mimi_loop_context";

#[derive(Debug, PartialEq)]
pub(crate) struct TypeError {
    pub(crate) message: String,
}

#[derive(Clone, Copy)]
struct Symbol {
    data_type: DataType,
    mutable: bool,
}

#[derive(Clone)]
struct Function {
    parameters: Vec<DataType>,
    return_type: DataType,
}

struct StructInfo {
    fields: HashMap<String, FieldInfo>,
    variants: HashMap<String, Option<DataType>>,
    methods: HashMap<String, Function>,
}

struct FieldInfo {
    data_type: DataType,
    default: Option<Expression>,
    mutable: bool,
}

struct ImportedExports {
    values: HashMap<String, DataType>,
    types: HashMap<&'static str, StructInfo>,
}

fn resolve_self(data_type: DataType, owner: &'static str) -> DataType {
    match data_type {
        DataType::SelfType => DataType::Struct(owner),
        DataType::Pointer(inner) => {
            DataType::Pointer(Box::leak(Box::new(resolve_self(*inner, owner))))
        }
        _ => data_type,
    }
}

fn imported_exports(
    path: &str,
    package: &str,
    import_paths: &[std::path::PathBuf],
) -> Result<ImportedExports, String> {
    let source_path = resolve_import_path(path, import_paths)?;
    let source = std::fs::read_to_string(&source_path)
        .map_err(|err| format!("cannot read '{}': {err}", source_path.display()))?;
    let ast = crate::parser::parse(tokenize(source.chars()));
    let callable_types = imported_callable_types(&ast);
    let all_type_names = ast
        .iter()
        .map(|node| match node {
            ASTNode::Public(declaration) => declaration.as_ref(),
            declaration => declaration,
        })
        .filter_map(|node| match node {
            ASTNode::StructDefinition { name, .. } | ASTNode::EnumDefinition { name, .. } => {
                Some(*name)
            }
            _ => None,
        })
        .collect::<HashSet<_>>();
    let local_type_names =
        ast.iter()
            .filter_map(|node| match node {
                ASTNode::Public(declaration) => match declaration.as_ref() {
                    ASTNode::StructDefinition { name, .. }
                    | ASTNode::EnumDefinition { name, .. } => Some(*name),
                    _ => None,
                },
                _ => None,
            })
            .collect::<HashSet<_>>();
    let enum_variants = ast
        .iter()
        .map(|node| match node {
            ASTNode::Public(declaration) => declaration.as_ref(),
            declaration => declaration,
        })
        .filter_map(|node| match node {
            ASTNode::EnumDefinition { name, variants } => Some(
                variants
                    .iter()
                    .map(|(variant, payload)| (variant.clone(), (*name, *payload)))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect::<HashMap<_, _>>();
    let mut constant_types = HashMap::new();
    for node in &ast {
        let declaration = match node {
            ASTNode::Public(declaration) => declaration.as_ref(),
            declaration => declaration,
        };
        if let ASTNode::CompileTimeConstant {
            name,
            data_type,
            value,
        } = declaration
            && let Some(data_type) =
                data_type.or_else(|| imported_constant_type(value, &constant_types, &enum_variants))
            && is_constant_type(data_type, &all_type_names)
            && imported_constant_expression(
                value,
                &constant_types,
                &enum_variants,
                &all_type_names,
                Some(data_type),
            )
        {
            constant_types.insert(name.clone(), data_type);
        }
    }
    let mut exports = HashMap::new();
    let mut types = HashMap::new();
    for node in ast {
        let ASTNode::Public(declaration) = node else {
            continue;
        };
        match *declaration {
            ASTNode::FunctionDefinition {
                name,
                parameters,
                return_type,
                ..
            } => {
                exports.insert(
                    name,
                    DataType::Function(
                        Box::leak(
                            parameters
                                .into_iter()
                                .map(|(_, data_type)| {
                                    qualify_imported_type(data_type, package, &local_type_names)
                                })
                                .collect::<Vec<_>>()
                                .into_boxed_slice(),
                        ),
                        Box::leak(Box::new(qualify_imported_type(
                            return_type,
                            package,
                            &local_type_names,
                        ))),
                    ),
                );
            }
            ASTNode::ExternalFunction {
                name,
                parameters,
                return_type,
                ..
            } => {
                exports.insert(
                    name,
                    DataType::Function(
                        Box::leak(
                            parameters
                                .into_iter()
                                .map(|(_, data_type)| {
                                    qualify_imported_type(data_type, package, &local_type_names)
                                })
                                .collect::<Vec<_>>()
                                .into_boxed_slice(),
                        ),
                        Box::leak(Box::new(qualify_imported_type(
                            return_type,
                            package,
                            &local_type_names,
                        ))),
                    ),
                );
            }
            ASTNode::CompileTimeConstant {
                name, data_type, ..
            } => {
                let function_type = data_type
                    .filter(|ty| matches!(ty, DataType::Function(_, _)))
                    .or_else(|| callable_types.get(&name).copied());
                let constant_type = constant_types
                    .get(&name)
                    .copied()
                    .filter(|ty| is_importable_constant_type(*ty, &local_type_names));
                if let Some(data_type) = function_type.or(constant_type) {
                    exports.insert(
                        name,
                        qualify_imported_type(data_type, package, &local_type_names),
                    );
                }
            }
            ASTNode::VariableDefinition {
                name,
                data_type,
                value,
            } => {
                if let Some(data_type) = data_type.or_else(|| imported_literal_type(&value)) {
                    exports.insert(
                        name,
                        qualify_imported_type(data_type, package, &local_type_names),
                    );
                }
            }
            ASTNode::StructDefinition {
                name,
                fields,
                methods,
            } => {
                let qualified_name: &'static str =
                    Box::leak(format!("{package}.{name}").into_boxed_str());
                exports.insert(name.to_string(), DataType::Struct(qualified_name));
                types.insert(
                    qualified_name,
                    StructInfo {
                        fields: fields
                            .into_iter()
                            .map(|(field_name, data_type, default, mutable)| {
                                (
                                    field_name,
                                    FieldInfo {
                                        data_type: qualify_imported_type(
                                            data_type,
                                            package,
                                            &local_type_names,
                                        ),
                                        default,
                                        mutable,
                                    },
                                )
                            })
                            .collect(),
                        variants: HashMap::new(),
                        methods: methods
                            .into_iter()
                            .map(|method| {
                                let parameters = method
                                    .parameters
                                    .into_iter()
                                    .map(|(_, data_type)| {
                                        resolve_self(
                                            qualify_imported_type(
                                                data_type,
                                                package,
                                                &local_type_names,
                                            ),
                                            qualified_name,
                                        )
                                    })
                                    .collect();
                                let return_type = resolve_self(
                                    qualify_imported_type(
                                        method.return_type,
                                        package,
                                        &local_type_names,
                                    ),
                                    qualified_name,
                                );
                                (
                                    method.name,
                                    Function {
                                        parameters,
                                        return_type,
                                    },
                                )
                            })
                            .collect(),
                    },
                );
            }
            ASTNode::EnumDefinition { name, variants } => {
                let qualified_name: &'static str =
                    Box::leak(format!("{package}.{name}").into_boxed_str());
                exports.insert(name.to_string(), DataType::Enum(qualified_name));
                types.insert(
                    qualified_name,
                    StructInfo {
                        fields: HashMap::new(),
                        variants: variants
                            .into_iter()
                            .map(|(variant, payload)| {
                                (
                                    variant,
                                    payload.map(|data_type| {
                                        qualify_imported_type(data_type, package, &local_type_names)
                                    }),
                                )
                            })
                            .collect(),
                        methods: HashMap::new(),
                    },
                );
            }
            _ => {}
        }
    }
    Ok(ImportedExports {
        values: exports,
        types,
    })
}

fn qualify_imported_type(
    data_type: DataType,
    package: &str,
    local_type_names: &HashSet<&'static str>,
) -> DataType {
    match data_type {
        DataType::Struct(name) if local_type_names.contains(name) => {
            DataType::Struct(Box::leak(format!("{package}.{name}").into_boxed_str()))
        }
        DataType::Enum(name) if local_type_names.contains(name) => {
            DataType::Enum(Box::leak(format!("{package}.{name}").into_boxed_str()))
        }
        DataType::Array(length, element) => DataType::Array(
            length,
            Box::leak(Box::new(qualify_imported_type(
                *element,
                package,
                local_type_names,
            ))),
        ),
        DataType::Slice(element) => DataType::Slice(Box::leak(Box::new(qualify_imported_type(
            *element,
            package,
            local_type_names,
        )))),
        DataType::Pointer(element) => DataType::Pointer(Box::leak(Box::new(
            qualify_imported_type(*element, package, local_type_names),
        ))),
        DataType::Function(parameters, result) => DataType::Function(
            Box::leak(
                parameters
                    .iter()
                    .copied()
                    .map(|parameter| qualify_imported_type(parameter, package, local_type_names))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            Box::leak(Box::new(qualify_imported_type(
                *result,
                package,
                local_type_names,
            ))),
        ),
        other => other,
    }
}

fn imported_callable_types(nodes: &[ASTNode]) -> HashMap<String, DataType> {
    let mut types = HashMap::new();
    let mut aliases = Vec::new();
    for node in nodes {
        let node = match node {
            ASTNode::Public(declaration) => declaration.as_ref(),
            declaration => declaration,
        };
        match node {
            ASTNode::FunctionDefinition {
                name,
                parameters,
                return_type,
                ..
            }
            | ASTNode::ExternalFunction {
                name,
                parameters,
                return_type,
                ..
            } => {
                types.insert(
                    name.clone(),
                    DataType::Function(
                        Box::leak(
                            parameters
                                .iter()
                                .map(|(_, data_type)| *data_type)
                                .collect::<Vec<_>>()
                                .into_boxed_slice(),
                        ),
                        Box::leak(Box::new(*return_type)),
                    ),
                );
            }
            ASTNode::CompileTimeConstant {
                name,
                data_type,
                value,
            } => aliases.push((name.clone(), *data_type, value.clone())),
            _ => {}
        }
    }

    loop {
        let mut resolved_alias = false;
        for (name, declared_type, value) in &aliases {
            if types.contains_key(name) {
                continue;
            }
            let data_type = declared_type
                .filter(|data_type| matches!(data_type, DataType::Function(_, _)))
                .or_else(|| match value {
                    Expression::Identifier(target) => types.get(target).copied(),
                    _ => None,
                });
            if let Some(data_type) = data_type {
                types.insert(name.clone(), data_type);
                resolved_alias = true;
            }
        }
        if !resolved_alias {
            break;
        }
    }
    types
}

pub(crate) fn resolve_import_path(path: &str, import_paths: &[PathBuf]) -> Result<PathBuf, String> {
    let requested = PathBuf::from(path);
    let mut candidates = vec![requested.clone()];
    candidates.extend(
        import_paths
            .iter()
            .map(|directory| directory.join(&requested)),
    );
    let mut source_path = candidates
        .iter()
        .find(|candidate| candidate.exists())
        .cloned()
        .unwrap_or_else(|| requested.clone());
    if !source_path.exists() && source_path.extension().is_none() {
        source_path.set_extension("mimi");
        if !source_path.exists() {
            source_path = import_paths
                .iter()
                .map(|directory| {
                    let mut candidate = directory.join(&requested);
                    candidate.set_extension("mimi");
                    candidate
                })
                .find(|candidate| candidate.exists())
                .unwrap_or(source_path);
        }
    }
    if !source_path.exists() {
        return Err(format!("cannot resolve import '{path}'"));
    }
    Ok(source_path)
}

fn imported_literal_type(expression: &Expression) -> Option<DataType> {
    match expression {
        Expression::LiteralBool(_) => Some(DataType::Bool),
        Expression::LiteralNumber(value) if value.contains('.') => Some(DataType::F64),
        Expression::LiteralNumber(value) => imported_integer_literal_type(value),
        Expression::Negate(inner) if matches!(inner.as_ref(), Expression::LiteralNumber(_)) => {
            let Expression::LiteralNumber(value) = inner.as_ref() else {
                unreachable!();
            };
            imported_negative_integer_literal_type(value)
        }
        _ => None,
    }
}

fn imported_integer_literal_type(value: &str) -> Option<DataType> {
    let value = value.parse::<u128>().ok()?;
    if value <= i32::MAX as u128 {
        Some(DataType::I32)
    } else if value <= i64::MAX as u128 {
        Some(DataType::I64)
    } else if value <= u64::MAX as u128 {
        Some(DataType::U64)
    } else {
        None
    }
}

fn imported_negative_integer_literal_type(value: &str) -> Option<DataType> {
    let magnitude = value.parse::<i128>().ok()?;
    let value = magnitude.checked_neg()?;
    if value >= i32::MIN as i128 {
        Some(DataType::I32)
    } else if value >= i64::MIN as i128 {
        Some(DataType::I64)
    } else {
        None
    }
}

fn imported_constant_type(
    expression: &Expression,
    constants: &HashMap<String, DataType>,
    enum_variants: &HashMap<String, (&'static str, Option<DataType>)>,
) -> Option<DataType> {
    match expression {
        Expression::Identifier(name) => constants.get(name).copied().or_else(|| {
            enum_variants
                .get(name)
                .filter(|(_, payload)| payload.is_none())
                .map(|(name, _)| DataType::Enum(name))
        }),
        Expression::LiteralString(_) => Some(DataType::Slice(Box::leak(Box::new(DataType::U8)))),
        Expression::LiteralBool(_) => Some(DataType::Bool),
        Expression::LiteralNumber(value) if value.contains('.') => Some(DataType::F64),
        Expression::LiteralNumber(value) => imported_integer_literal_type(value),
        Expression::Negate(inner) if matches!(inner.as_ref(), Expression::LiteralNumber(_)) => {
            let Expression::LiteralNumber(value) = inner.as_ref() else {
                unreachable!();
            };
            imported_negative_integer_literal_type(value)
        }
        Expression::Negate(value) => imported_constant_type(value, constants, enum_variants),
        Expression::Not(_) => Some(DataType::Bool),
        Expression::And(_, _)
        | Expression::Or(_, _)
        | Expression::Equal(_, _)
        | Expression::NotEqual(_, _)
        | Expression::LessThan(_, _)
        | Expression::LessThanOrEqual(_, _)
        | Expression::GreaterThan(_, _)
        | Expression::GreaterThanOrEqual(_, _) => Some(DataType::Bool),
        Expression::Add(lhs, rhs)
        | Expression::Subtract(lhs, rhs)
        | Expression::Multiply(lhs, rhs)
        | Expression::Divide(lhs, rhs)
        | Expression::Remainder(lhs, rhs) => {
            let lhs = imported_constant_type(lhs, constants, enum_variants)?;
            let rhs = imported_constant_type(rhs, constants, enum_variants)?;
            common_numeric_type(lhs, rhs)
        }
        Expression::StructLiteral { type_name, .. } => type_name.map(DataType::Struct),
        Expression::ArrayLiteral(values) => {
            let element_type = values
                .first()
                .and_then(|value| imported_constant_type(value, constants, enum_variants))?;
            Some(DataType::Array(
                values.len() as u64,
                Box::leak(Box::new(element_type)),
            ))
        }
        Expression::Call { name, .. } => enum_variants
            .get(name)
            .map(|(name, _)| DataType::Enum(name)),
        Expression::If {
            then_branch,
            else_branch,
            ..
        } => {
            let then_type = imported_constant_type(then_branch, constants, enum_variants)?;
            let else_type = imported_constant_type(else_branch, constants, enum_variants)?;
            (then_type == else_type)
                .then_some(then_type)
                .or_else(|| common_numeric_type(then_type, else_type))
        }
        _ => None,
    }
}

fn imported_constant_expression(
    expression: &Expression,
    constants: &HashMap<String, DataType>,
    enum_variants: &HashMap<String, (&'static str, Option<DataType>)>,
    local_type_names: &HashSet<&'static str>,
    expected: Option<DataType>,
) -> bool {
    match expression {
        Expression::LiteralNumber(_) | Expression::LiteralBool(_) => true,
        Expression::LiteralString(_) => {
            matches!(expected, Some(DataType::Slice(element)) if *element == DataType::U8)
        }
        Expression::Identifier(name) => {
            constants.contains_key(name)
                || enum_variants
                    .get(name)
                    .is_some_and(|(_, payload)| payload.is_none())
        }
        Expression::Negate(inner) | Expression::Not(inner) => {
            imported_constant_expression(inner, constants, enum_variants, local_type_names, None)
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
            imported_constant_expression(lhs, constants, enum_variants, local_type_names, None)
                && imported_constant_expression(
                    rhs,
                    constants,
                    enum_variants,
                    local_type_names,
                    None,
                )
        }
        Expression::StructLiteral { fields, .. } => fields.iter().all(|(_, value)| {
            imported_constant_expression(value, constants, enum_variants, local_type_names, None)
        }),
        Expression::ArrayLiteral(values) => {
            if matches!(expected, Some(DataType::Slice(_))) {
                return false;
            }
            let element_type = match expected {
                Some(DataType::Array(_, element)) => Some(*element),
                _ => None,
            };
            values.iter().all(|value| {
                imported_constant_expression(
                    value,
                    constants,
                    enum_variants,
                    local_type_names,
                    element_type,
                )
            })
        }
        Expression::Call { name, arguments } => {
            enum_variants.contains_key(name)
                && arguments.iter().all(|argument| {
                    imported_constant_expression(
                        argument,
                        constants,
                        enum_variants,
                        local_type_names,
                        None,
                    )
                })
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            imported_constant_expression(
                condition,
                constants,
                enum_variants,
                local_type_names,
                None,
            ) && imported_constant_expression(
                then_branch,
                constants,
                enum_variants,
                local_type_names,
                expected,
            ) && imported_constant_expression(
                else_branch,
                constants,
                enum_variants,
                local_type_names,
                expected,
            )
        }
        _ => false,
    }
}

fn is_constant_type(data_type: DataType, local_type_names: &HashSet<&'static str>) -> bool {
    matches!(
        data_type,
        DataType::U8
            | DataType::U16
            | DataType::U32
            | DataType::U64
            | DataType::I8
            | DataType::I16
            | DataType::I32
            | DataType::I64
            | DataType::F16
            | DataType::F32
            | DataType::F64
            | DataType::Bool
    ) || match data_type {
        DataType::Struct(name) | DataType::Enum(name) => local_type_names.contains(name),
        DataType::Array(_, element) => {
            !matches!(*element, DataType::Slice(_)) && is_constant_type(*element, local_type_names)
        }
        DataType::Slice(element) => *element == DataType::U8,
        _ => false,
    }
}

fn is_importable_constant_type(
    data_type: DataType,
    public_type_names: &HashSet<&'static str>,
) -> bool {
    matches!(
        data_type,
        DataType::U8
            | DataType::U16
            | DataType::U32
            | DataType::U64
            | DataType::I8
            | DataType::I16
            | DataType::I32
            | DataType::I64
            | DataType::F16
            | DataType::F32
            | DataType::F64
            | DataType::Bool
    ) || match data_type {
        DataType::Struct(name) | DataType::Enum(name) => public_type_names.contains(name),
        DataType::Array(_, element) => {
            !matches!(*element, DataType::Slice(_))
                && is_importable_constant_type(*element, public_type_names)
        }
        DataType::Slice(element) => *element == DataType::U8,
        _ => false,
    }
}

pub(crate) fn check(nodes: &[ASTNode]) -> Result<HashMap<String, DataType>, Vec<TypeError>> {
    check_with_import_paths(nodes, &[])
}

pub(crate) fn check_with_import_paths(
    nodes: &[ASTNode],
    import_paths: &[std::path::PathBuf],
) -> Result<HashMap<String, DataType>, Vec<TypeError>> {
    let mut scopes = vec![HashMap::new()];
    let mut functions = HashMap::new();
    let mut structs = HashMap::new();
    let mut errors = Vec::new();
    let mut expanded_nodes = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            ASTNode::Public(inner) => expanded_nodes.push((**inner).clone()),
            other => expanded_nodes.push(other.clone()),
        }
    }
    let nodes = expanded_nodes.as_slice();

    for node in nodes {
        if let ASTNode::Import { alias, path } = node {
            match imported_exports(path, alias, import_paths) {
                Ok(exports) => {
                    for (name, data_type) in exports.values {
                        scopes[0].insert(
                            format!("{alias}.{name}"),
                            Symbol {
                                data_type,
                                mutable: false,
                            },
                        );
                    }
                    for (name, info) in exports.types {
                        structs.insert(name, info);
                    }
                }
                Err(message) => errors.push(error(format!("import '{path}': {message}"))),
            }
        }
    }
    let imported_symbols = scopes[0]
        .iter()
        .filter(|(name, _)| name.contains('.'))
        .map(|(name, symbol)| (name.clone(), *symbol))
        .collect::<HashMap<_, _>>();

    for node in nodes {
        if let ASTNode::EnumDefinition { name, variants } = node {
            structs.insert(
                *name,
                StructInfo {
                    fields: HashMap::new(),
                    variants: variants.iter().cloned().collect(),
                    methods: HashMap::new(),
                },
            );
        }
        if let ASTNode::StructDefinition {
            name,
            fields,
            methods,
        } = node
        {
            let mut field_names = std::collections::HashSet::new();
            for (field_name, _, _, _) in fields {
                if !field_names.insert(field_name) {
                    errors.push(error(format!(
                        "duplicate field '{field_name}' in struct '{name}'"
                    )));
                }
            }
            if structs
                .insert(
                    *name,
                    StructInfo {
                        fields: fields
                            .iter()
                            .map(|(name, data_type, default, mutable)| {
                                (
                                    name.clone(),
                                    FieldInfo {
                                        data_type: *data_type,
                                        default: default.clone(),
                                        mutable: *mutable,
                                    },
                                )
                            })
                            .collect(),
                        variants: HashMap::new(),
                        methods: methods
                            .iter()
                            .map(|method| {
                                (
                                    method.name.clone(),
                                    Function {
                                        parameters: method
                                            .parameters
                                            .iter()
                                            .map(|(_, ty)| resolve_self(*ty, *name))
                                            .collect(),
                                        return_type: resolve_self(method.return_type, *name),
                                    },
                                )
                            })
                            .collect(),
                    },
                )
                .is_some()
            {
                errors.push(error(format!("struct '{name}' is already defined")));
            }
        }
        if let ASTNode::FunctionDefinition {
            name,
            parameters,
            return_type,
            ..
        } = node
        {
            if functions
                .insert(
                    name.clone(),
                    Function {
                        parameters: parameters.iter().map(|(_, ty)| *ty).collect(),
                        return_type: *return_type,
                    },
                )
                .is_some()
            {
                errors.push(error(format!("function '{name}' is already defined")));
            }
        }
        if let ASTNode::ExternalFunction {
            name,
            parameters,
            return_type,
            ..
        } = node
        {
            if matches!(return_type, DataType::Slice(_)) {
                errors.push(error(format!(
                    "external function '{name}' cannot return a slice"
                )));
            }
            if functions
                .insert(
                    name.clone(),
                    Function {
                        parameters: parameters.iter().map(|(_, ty)| *ty).collect(),
                        return_type: *return_type,
                    },
                )
                .is_some()
            {
                errors.push(error(format!("function '{name}' is already defined")));
            }
        }
    }

    for node in nodes {
        match node {
            ASTNode::StructDefinition { name, methods, .. } => {
                for method in methods {
                    let mut method_scopes = vec![imported_symbols.clone(), HashMap::new()];
                    for (parameter_name, data_type) in &method.parameters {
                        method_scopes[1].insert(
                            parameter_name.clone(),
                            Symbol {
                                data_type: resolve_self(*data_type, name),
                                mutable: true,
                            },
                        );
                    }
                    if let Err(err) = infer_expression(
                        &method.body,
                        &mut method_scopes,
                        &functions,
                        &structs,
                        Some(resolve_self(method.return_type, name)),
                    ) {
                        errors.push(error(format!(
                            "method '{}::{}': {}",
                            name, method.name, err.message
                        )));
                    }
                }
            }
            ASTNode::FunctionDefinition {
                name,
                parameters,
                return_type,
                body,
            } => {
                let mut function_scopes = vec![imported_symbols.clone(), HashMap::new()];
                for (parameter_name, data_type) in parameters {
                    if function_scopes[1]
                        .insert(
                            parameter_name.clone(),
                            Symbol {
                                data_type: *data_type,
                                mutable: true,
                            },
                        )
                        .is_some()
                    {
                        errors.push(error(format!(
                            "duplicate parameter '{parameter_name}' in function '{name}'"
                        )));
                    }
                }
                if let Err(err) = infer_expression(
                    body,
                    &mut function_scopes,
                    &functions,
                    &structs,
                    Some(*return_type),
                ) {
                    errors.push(error(format!("function '{name}': {}", err.message)));
                }
            }
            ASTNode::ExternalFunction { .. } => {}
            ASTNode::Return(_) => errors.push(error("return is only valid inside a function")),
            _ => {
                if let Err(err) = check_statement(node, &mut scopes, &functions, &structs) {
                    errors.push(err);
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(scopes[0]
            .iter()
            .map(|(name, symbol)| (name.clone(), symbol.data_type))
            .collect())
    } else {
        Err(errors)
    }
}

fn check_statement(
    node: &ASTNode,
    scopes: &mut Vec<HashMap<String, Symbol>>,
    functions: &HashMap<String, Function>,
    structs: &HashMap<&'static str, StructInfo>,
) -> Result<(), TypeError> {
    match node {
        ASTNode::VariableDefinition {
            name,
            data_type,
            value,
        } => {
            if scopes.last().unwrap().contains_key(name) {
                return Err(error(format!("'{name}' is already defined")));
            }
            match infer_expression(value, scopes, functions, structs, *data_type).and_then(
                |inferred| {
                    require_compatible(inferred, *data_type)?;
                    Ok(inferred)
                },
            ) {
                Ok(inferred) => {
                    scopes.last_mut().unwrap().insert(
                        name.clone(),
                        Symbol {
                            data_type: data_type.unwrap_or(inferred),
                            mutable: true,
                        },
                    );
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
        ASTNode::CompileTimeConstant {
            name,
            data_type,
            value,
        } => {
            if scopes.last().unwrap().contains_key(name) {
                return Err(error(format!("'{name}' is already defined")));
            }
            match infer_expression(value, scopes, functions, structs, *data_type).and_then(
                |inferred| {
                    require_compatible(inferred, *data_type)?;
                    Ok(inferred)
                },
            ) {
                Ok(inferred) => {
                    scopes.last_mut().unwrap().insert(
                        name.clone(),
                        Symbol {
                            data_type: data_type.unwrap_or(inferred),
                            mutable: false,
                        },
                    );
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
        ASTNode::Assignment { name, value } => match lookup(scopes, name).copied() {
            None => Err(error(format!(
                "cannot assign to undefined variable '{name}'"
            ))),
            Some(symbol) if !symbol.mutable => {
                Err(error(format!("cannot assign to constant '{name}'")))
            }
            Some(symbol) => {
                if let Err(err) =
                    infer_expression(value, scopes, functions, structs, Some(symbol.data_type))
                        .and_then(|inferred| require_compatible(inferred, Some(symbol.data_type)))
                {
                    Err(error(format!("assignment to '{name}': {}", err.message)))
                } else {
                    Ok(())
                }
            }
        },
        ASTNode::IndexAssignment {
            object,
            index,
            value,
        } => {
            ensure_mutable_index_target(object, scopes, functions, structs)?;
            let object_type = infer_expression(object, scopes, functions, structs, None)?;
            let element_type = match object_type {
                DataType::Array(_, element)
                | DataType::Slice(element)
                | DataType::Pointer(element) => *element,
                _ => {
                    return Err(error(
                        "indexed assignment requires an array, slice, or pointer",
                    ));
                }
            };
            let index_type = infer_expression(index, scopes, functions, structs, None)?;
            if !index_type.is_integer() {
                return Err(error("array index must be an integer"));
            }
            infer_expression(value, scopes, functions, structs, Some(element_type))?;
            Ok(())
        }
        ASTNode::PointerAssignment { pointer, value } => {
            let pointer_type = infer_expression(pointer, scopes, functions, structs, None)?;
            let DataType::Pointer(element_type) = pointer_type else {
                return Err(error("dereference assignment requires a pointer"));
            };
            infer_expression(value, scopes, functions, structs, Some(*element_type))?;
            Ok(())
        }
        ASTNode::MemberAssignment {
            object,
            member,
            value,
        } => {
            let object_type = infer_expression(object, scopes, functions, structs, None)?;
            let struct_name = match object_type {
                DataType::Struct(name) => name,
                DataType::Pointer(inner) => match *inner {
                    DataType::Struct(name) => name,
                    _ => return Err(error("member assignment requires a struct value")),
                },
                _ => return Err(error("member assignment requires a struct value")),
            };
            let field = structs
                .get(struct_name)
                .and_then(|info| info.fields.get(member))
                .ok_or_else(|| error(format!("unknown field '{member}' in '{struct_name}'")))?;
            if !field.mutable {
                return Err(error(format!("cannot assign to fixed field '{member}'")));
            }
            infer_expression(value, scopes, functions, structs, Some(field.data_type))?;
            Ok(())
        }
        ASTNode::Expression(_) => Err(error("expression statements are only valid inside blocks")),
        ASTNode::FunctionDefinition { .. } => {
            Err(error("nested function definitions are unsupported"))
        }
        ASTNode::ExternalFunction { .. } => Ok(()),
        ASTNode::Return(_) => Err(error("return is only valid inside a function")),
        ASTNode::Break | ASTNode::Continue => {
            if scopes.iter().any(|scope| scope.contains_key(LOOP_CONTEXT)) {
                Ok(())
            } else if matches!(node, ASTNode::Break) {
                Err(error("break is only valid inside a loop"))
            } else {
                Err(error("continue is only valid inside a loop"))
            }
        }
        ASTNode::StructDefinition { .. } => Ok(()),
        ASTNode::EnumDefinition { .. } => Ok(()),
        ASTNode::Import { .. } | ASTNode::Public(_) => Ok(()),
        ASTNode::IfLet {
            variant,
            binding,
            value,
            then_branch,
            else_branch,
        } => {
            let value_type = infer_expression(value, scopes, functions, structs, None)?;
            let enum_name = match value_type {
                DataType::Enum(name) | DataType::Struct(name) => name,
                _ => return Err(error("if-let requires an enum value")),
            };
            let info = structs
                .get(enum_name)
                .ok_or_else(|| error("unknown enum type"))?;
            let payload = info
                .variants
                .get(variant)
                .ok_or_else(|| error(format!("unknown enum variant '{variant}'")))?;
            scopes.push(HashMap::new());
            if let (Some(binding), Some(payload)) = (binding, payload) {
                scopes.last_mut().unwrap().insert(
                    binding.clone(),
                    Symbol {
                        data_type: *payload,
                        mutable: false,
                    },
                );
            }
            infer_expression(then_branch, scopes, functions, structs, None)?;
            scopes.pop();
            if let Some(branch) = else_branch {
                infer_expression(branch, scopes, functions, structs, None)?;
            }
            Ok(())
        }
        ASTNode::If {
            condition,
            then_branch,
            else_branch,
        } => {
            infer_expression(condition, scopes, functions, structs, Some(DataType::Bool))?;
            infer_expression(then_branch, scopes, functions, structs, None)?;
            if let Some(branch) = else_branch {
                infer_expression(branch, scopes, functions, structs, None)?;
            }
            Ok(())
        }
        ASTNode::While { condition, body } => {
            scopes.push(loop_scope());
            let result =
                infer_expression(condition, scopes, functions, structs, Some(DataType::Bool))
                    .and_then(|_| infer_expression(body, scopes, functions, structs, None));
            scopes.pop();
            result.map(|_| ())
        }
        ASTNode::DoWhile { body, condition } => {
            scopes.push(loop_scope());
            let result = infer_expression(body, scopes, functions, structs, None).and_then(|_| {
                infer_expression(condition, scopes, functions, structs, Some(DataType::Bool))
            });
            scopes.pop();
            result.map(|_| ())
        }
        ASTNode::For {
            binding,
            iterable,
            body,
        } => {
            let iterable_type = infer_expression(iterable, scopes, functions, structs, None)?;
            let item_type = match iterable_type {
                DataType::Array(_, element) | DataType::Slice(element) => *element,
                _ => return Err(error("for loop requires an array or slice")),
            };
            let mut loop_scope = loop_scope();
            loop_scope.insert(
                binding.clone(),
                Symbol {
                    data_type: item_type,
                    mutable: false,
                },
            );
            scopes.push(loop_scope);
            let result = infer_expression(body, scopes, functions, structs, None);
            scopes.pop();
            result?;
            Ok(())
        }
        ASTNode::Loop { body } => {
            scopes.push(loop_scope());
            let result = infer_expression(body, scopes, functions, structs, None);
            scopes.pop();
            result.map(|_| ())
        }
        ASTNode::Defer { body } => {
            infer_expression(body, scopes, functions, structs, None)?;
            Ok(())
        }
    }
}

fn loop_scope() -> HashMap<String, Symbol> {
    HashMap::from([(
        LOOP_CONTEXT.to_string(),
        Symbol {
            data_type: DataType::Void,
            mutable: false,
        },
    )])
}

fn lookup<'a>(scopes: &'a [HashMap<String, Symbol>], name: &str) -> Option<&'a Symbol> {
    scopes.iter().rev().find_map(|scope| scope.get(name))
}

fn ensure_mutable_index_target(
    expression: &Expression,
    scopes: &mut Vec<HashMap<String, Symbol>>,
    functions: &HashMap<String, Function>,
    structs: &HashMap<&'static str, StructInfo>,
) -> Result<(), TypeError> {
    match expression {
        Expression::Identifier(name) => {
            let symbol = lookup(scopes, name).ok_or_else(|| {
                error(format!("cannot assign through undefined variable '{name}'"))
            })?;
            if symbol.mutable {
                Ok(())
            } else {
                Err(error(format!("cannot assign through constant '{name}'")))
            }
        }
        Expression::MemberAccess { object, member } => {
            let object_type = infer_expression(object, scopes, functions, structs, None)?;
            let struct_name = match object_type {
                DataType::Struct(name) => name,
                DataType::Pointer(inner) => match *inner {
                    DataType::Struct(name) => name,
                    _ => return Err(error("indexed assignment requires a mutable array field")),
                },
                _ => return Err(error("indexed assignment requires a mutable array field")),
            };
            let field = structs
                .get(struct_name)
                .and_then(|info| info.fields.get(member))
                .ok_or_else(|| error(format!("unknown field '{member}' in '{struct_name}'")))?;
            if field.mutable {
                Ok(())
            } else {
                Err(error(format!(
                    "cannot assign through fixed field '{member}'"
                )))
            }
        }
        Expression::Index { object, .. } => {
            ensure_mutable_index_target(object, scopes, functions, structs)
        }
        _ => Err(error("indexed assignment target is not mutable")),
    }
}

fn infer_expression(
    expression: &Expression,
    scopes: &mut Vec<HashMap<String, Symbol>>,
    functions: &HashMap<String, Function>,
    structs: &HashMap<&'static str, StructInfo>,
    expected: Option<DataType>,
) -> Result<DataType, TypeError> {
    match expression {
        Expression::Identifier(name) => {
            let data_type = if let Some(symbol) = lookup(scopes, name) {
                symbol.data_type
            } else if let Some(function) = functions.get(name) {
                DataType::Function(
                    Box::leak(function.parameters.clone().into_boxed_slice()),
                    Box::leak(Box::new(function.return_type)),
                )
            } else {
                let matches: Vec<_> = structs
                    .iter()
                    .filter_map(|(enum_name, info)| {
                        info.variants
                            .get(name)
                            .filter(|payload| payload.is_none())
                            .map(|_| *enum_name)
                    })
                    .collect();
                match matches.as_slice() {
                    [enum_name] => DataType::Enum(enum_name),
                    [] => return Err(error(format!("use of undefined identifier '{name}'"))),
                    _ => return Err(error(format!("ambiguous enum variant '{name}'"))),
                }
            };
            require_compatible(data_type, expected)?;
            Ok(expected.unwrap_or(data_type))
        }
        Expression::LiteralNumber(text) => infer_number(text, expected),
        Expression::LiteralString(_) => {
            let string_type = DataType::Slice(Box::leak(Box::new(DataType::U8)));
            require_compatible(string_type, expected)?;
            Ok(string_type)
        }
        Expression::LiteralBool(_) => {
            require_compatible(DataType::Bool, expected)?;
            Ok(DataType::Bool)
        }
        Expression::Negate(inner) => {
            if let Expression::LiteralNumber(text) = inner.as_ref()
                && !text.contains('.')
            {
                return infer_negative_integer(text, expected);
            }
            let inner_type = infer_expression(inner, scopes, functions, structs, expected)?;
            if inner_type.is_float() || inner_type.is_signed_integer() {
                Ok(inner_type)
            } else {
                Err(error(format!(
                    "unary '-' cannot be applied to {inner_type}"
                )))
            }
        }
        Expression::AddressOf(inner) => {
            if !is_addressable(inner, scopes) {
                return Err(error(
                    "address-of requires a mutable variable or array element",
                ));
            }
            let inner_type = infer_expression(inner, scopes, functions, structs, None)?;
            Ok(DataType::Pointer(Box::leak(Box::new(inner_type))))
        }
        Expression::Dereference(pointer) => {
            match infer_expression(pointer, scopes, functions, structs, None)? {
                DataType::Pointer(element) => Ok(*element),
                _ => Err(error("dereference requires a pointer")),
            }
        }
        Expression::Not(inner) => {
            infer_expression(inner, scopes, functions, structs, Some(DataType::Bool))?;
            Ok(DataType::Bool)
        }
        Expression::And(lhs, rhs) | Expression::Or(lhs, rhs) => {
            infer_expression(lhs, scopes, functions, structs, Some(DataType::Bool))?;
            infer_expression(rhs, scopes, functions, structs, Some(DataType::Bool))?;
            Ok(DataType::Bool)
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
        } => {
            infer_expression(condition, scopes, functions, structs, Some(DataType::Bool))?;
            let then_type = infer_expression(then_branch, scopes, functions, structs, expected)?;
            let else_type = infer_expression(
                else_branch,
                scopes,
                functions,
                structs,
                expected.or(Some(then_type)),
            )?;
            let result_type = if then_type == else_type {
                then_type
            } else if let Some(common) = common_numeric_type(then_type, else_type) {
                common
            } else {
                return Err(error(format!(
                    "if branches have incompatible types {then_type} and {else_type}"
                )));
            };
            require_compatible(result_type, expected)?;
            Ok(result_type)
        }
        Expression::Add(lhs, rhs) => {
            let lhs_type = infer_expression(lhs, scopes, functions, structs, None)?;
            let rhs_type = infer_expression(rhs, scopes, functions, structs, None)?;
            if let (DataType::Pointer(inner), offset_type) = (lhs_type, rhs_type)
                && offset_type.is_integer()
                && pointer_stride(*inner, structs).is_some()
            {
                return Ok(lhs_type);
            }
            if let (offset_type, DataType::Pointer(inner)) = (lhs_type, rhs_type)
                && offset_type.is_integer()
                && pointer_stride(*inner, structs).is_some()
            {
                return Ok(rhs_type);
            }
            let (lhs_type, rhs_type) =
                infer_numeric_pair(lhs, rhs, scopes, functions, structs, expected)?;
            common_numeric_type(lhs_type, rhs_type).ok_or_else(|| {
                error(format!(
                    "incompatible numeric types {lhs_type} and {rhs_type}"
                ))
            })
        }
        Expression::Subtract(lhs, rhs) => {
            let lhs_type = infer_expression(lhs, scopes, functions, structs, None)?;
            let rhs_type = infer_expression(rhs, scopes, functions, structs, None)?;
            if let (DataType::Pointer(lhs_element), DataType::Pointer(rhs_element)) =
                (lhs_type, rhs_type)
                && lhs_element == rhs_element
                && pointer_stride(*lhs_element, structs).is_some()
            {
                return Ok(DataType::I64);
            }
            if let (DataType::Pointer(inner), offset_type) = (lhs_type, rhs_type)
                && offset_type.is_integer()
                && pointer_stride(*inner, structs).is_some()
            {
                return Ok(lhs_type);
            }
            let (lhs_type, rhs_type) =
                infer_numeric_pair(lhs, rhs, scopes, functions, structs, expected)?;
            common_numeric_type(lhs_type, rhs_type).ok_or_else(|| {
                error(format!(
                    "incompatible numeric types {lhs_type} and {rhs_type}"
                ))
            })
        }
        Expression::Multiply(lhs, rhs)
        | Expression::Divide(lhs, rhs)
        | Expression::Remainder(lhs, rhs) => {
            let (lhs_type, rhs_type) =
                infer_numeric_pair(lhs, rhs, scopes, functions, structs, expected)?;
            if matches!(expression, Expression::Remainder(_, _))
                && (!lhs_type.is_integer() || !rhs_type.is_integer())
            {
                return Err(error("'%' requires integer operands"));
            }
            common_numeric_type(lhs_type, rhs_type).ok_or_else(|| {
                error(format!(
                    "incompatible numeric types {lhs_type} and {rhs_type}"
                ))
            })
        }
        Expression::Equal(lhs, rhs) | Expression::NotEqual(lhs, rhs) => {
            let lhs_type = infer_expression(lhs, scopes, functions, structs, None)?;
            let rhs_type = infer_expression(rhs, scopes, functions, structs, Some(lhs_type))
                .or_else(|_| {
                    let rhs_type = infer_expression(rhs, scopes, functions, structs, None)?;
                    common_numeric_type(lhs_type, rhs_type)
                        .map(|_| rhs_type)
                        .ok_or_else(|| error(format!("cannot compare {lhs_type} with {rhs_type}")))
                })?;
            if lhs_type == rhs_type || common_numeric_type(lhs_type, rhs_type).is_some() {
                Ok(DataType::Bool)
            } else {
                Err(error(format!("cannot compare {lhs_type} with {rhs_type}")))
            }
        }
        Expression::LessThan(lhs, rhs)
        | Expression::LessThanOrEqual(lhs, rhs)
        | Expression::GreaterThan(lhs, rhs)
        | Expression::GreaterThanOrEqual(lhs, rhs) => {
            let (lhs_type, rhs_type) =
                infer_numeric_pair(lhs, rhs, scopes, functions, structs, None)?;
            common_numeric_type(lhs_type, rhs_type)
                .map(|_| DataType::Bool)
                .ok_or_else(|| error(format!("cannot order {lhs_type} and {rhs_type}")))
        }
        Expression::Block(items) => {
            scopes.push(HashMap::new());
            let result = (|| {
                let mut result_type = DataType::Void;
                for (index, item) in items.iter().enumerate() {
                    let is_last = index + 1 == items.len();
                    match item {
                        ASTNode::Expression(expression) => {
                            result_type = infer_expression(
                                expression,
                                scopes,
                                functions,
                                structs,
                                if is_last { expected } else { None },
                            )?;
                        }
                        ASTNode::Return(value) => {
                            let returned_type = match value {
                                Some(value) => {
                                    infer_expression(value, scopes, functions, structs, expected)?
                                }
                                None => DataType::Void,
                            };
                            require_compatible(returned_type, expected)?;
                            result_type = returned_type;
                        }
                        _ => {
                            check_statement(item, scopes, functions, structs)?;
                            result_type = DataType::Void;
                        }
                    }
                }
                require_compatible(result_type, expected)?;
                Ok(result_type)
            })();
            scopes.pop();
            result
        }
        Expression::Call { name, arguments } => {
            if let Some(Symbol {
                data_type: DataType::Function(parameters, return_type),
                ..
            }) = lookup(scopes, name).copied()
            {
                if arguments.len() != parameters.len() {
                    return Err(error("wrong number of function arguments"));
                }
                for (argument, parameter_type) in arguments.iter().zip(parameters.iter()) {
                    infer_expression(argument, scopes, functions, structs, Some(*parameter_type))?;
                }
                require_compatible(*return_type, expected)?;
                return Ok(*return_type);
            }
            for (enum_name, info) in structs {
                if let Some(payload) = info.variants.get(name) {
                    if arguments.len() != usize::from(payload.is_some()) {
                        return Err(error(format!("invalid payload for enum variant '{name}'")));
                    }
                    if let (Some(payload), Some(argument)) = (payload, arguments.first()) {
                        infer_expression(argument, scopes, functions, structs, Some(*payload))?;
                    }
                    return Ok(DataType::Enum(enum_name));
                }
            }
            for (enum_name, info) in structs {
                if let Some(payload) = info.variants.get(name) {
                    if arguments.len() != usize::from(payload.is_some()) {
                        return Err(error(format!("invalid payload for enum variant '{name}'")));
                    }
                    if let (Some(payload), Some(argument)) = (payload, arguments.first()) {
                        infer_expression(argument, scopes, functions, structs, Some(*payload))?;
                    }
                    return Ok(DataType::Enum(enum_name));
                }
            }
            let function = functions
                .get(name)
                .ok_or_else(|| error(format!("call to undefined function '{name}'")))?;
            if arguments.len() != function.parameters.len() {
                return Err(error(format!(
                    "function '{name}' expects {} arguments, got {}",
                    function.parameters.len(),
                    arguments.len()
                )));
            }
            for (argument, parameter_type) in arguments.iter().zip(&function.parameters) {
                let actual =
                    infer_expression(argument, scopes, functions, structs, Some(*parameter_type))?;
                require_compatible(actual, Some(*parameter_type))?;
            }
            require_compatible(function.return_type, expected)?;
            Ok(function.return_type)
        }
        Expression::StructLiteral { type_name, fields } => {
            let result = match (type_name, expected) {
                (Some(name), _) => Ok(DataType::Struct(name)),
                (None, Some(data_type @ DataType::Struct(_))) => Ok(data_type),
                (None, _) => Err(error("cannot infer anonymous struct type")),
            }?;
            let struct_name = match result {
                DataType::Struct(name) => name,
                _ => unreachable!(),
            };
            let info = structs
                .get(struct_name)
                .ok_or_else(|| error(format!("undefined struct type '{struct_name}'")))?;
            let mut provided = HashSet::new();
            for (field_name, value) in fields {
                let field_type = info.fields.get(field_name).ok_or_else(|| {
                    error(format!(
                        "unknown field '{field_name}' in struct '{struct_name}'"
                    ))
                })?;
                if !provided.insert(field_name) {
                    return Err(error(format!(
                        "field '{field_name}' provided more than once"
                    )));
                }
                infer_expression(
                    value,
                    scopes,
                    functions,
                    structs,
                    Some(field_type.data_type),
                )?;
            }
            for (field_name, field) in &info.fields {
                if let Some(default) = &field.default {
                    if !field.mutable && !is_constant_expression(default) {
                        return Err(error(format!(
                            "compile-time default for '{field_name}' must be constant"
                        )));
                    }
                    infer_expression(default, scopes, functions, structs, Some(field.data_type))?;
                }
            }
            for (field_name, field) in &info.fields {
                if !provided.contains(field_name) {
                    if field.default.is_none() {
                        return Err(error(format!(
                            "missing required field '{field_name}' in '{struct_name}'"
                        )));
                    }
                }
            }
            Ok(result)
        }
        Expression::MemberAccess { object, member } => {
            if let Expression::Identifier(package) = object.as_ref() {
                let qualified = format!("{package}.{member}");
                if let Some(symbol) = lookup(scopes, &qualified) {
                    return Ok(symbol.data_type);
                }
            }
            let object_type = infer_expression(object, scopes, functions, structs, None)?;
            let struct_name = match object_type {
                DataType::Struct(name) => name,
                DataType::Pointer(element) => match *element {
                    DataType::Struct(name) => name,
                    _ => return Err(error("member access requires a struct value")),
                },
                _ => return Err(error("member access requires a struct value")),
            };
            if let Some(info) = structs.get(struct_name) {
                if let Some(field) = info.fields.get(member) {
                    return Ok(field.data_type);
                }
                if let Some(method) = info.methods.get(member) {
                    let params = method
                        .parameters
                        .iter()
                        .skip(1)
                        .copied()
                        .collect::<Vec<_>>();
                    return Ok(DataType::Function(
                        Box::leak(params.into_boxed_slice()),
                        Box::leak(Box::new(method.return_type)),
                    ));
                }
            }
            Err(error(format!(
                "unknown member '{member}' in '{struct_name}'"
            )))
        }
        Expression::CallValue { callee, arguments } => {
            if let Expression::MemberAccess { object, member } = callee.as_ref()
                && let Expression::Identifier(package) = object.as_ref()
            {
                let qualified = format!("{package}.{member}");
                if let Some(Symbol {
                    data_type: DataType::Function(parameters, return_type),
                    ..
                }) = lookup(scopes, &qualified).copied()
                {
                    if arguments.len() != parameters.len() {
                        return Err(error(format!(
                            "wrong number of arguments for '{qualified}'"
                        )));
                    }
                    for (argument, parameter_type) in arguments.iter().zip(parameters.iter()) {
                        infer_expression(
                            argument,
                            scopes,
                            functions,
                            structs,
                            Some(*parameter_type),
                        )?;
                    }
                    require_compatible(*return_type, expected)?;
                    return Ok(*return_type);
                }
            }
            if let Expression::MemberAccess { object, member } = callee.as_ref() {
                let object_type = infer_expression(object, scopes, functions, structs, None)?;
                let struct_name = match object_type {
                    DataType::Struct(name) => name,
                    DataType::Pointer(element) => match *element {
                        DataType::Struct(name) => name,
                        _ => return Err(error("method call requires a struct value")),
                    },
                    _ => return Err(error("method call requires a struct value")),
                };
                let method = structs
                    .get(struct_name)
                    .and_then(|info| info.methods.get(member))
                    .ok_or_else(|| {
                        error(format!("unknown method '{member}' in '{struct_name}'"))
                    })?;
                let Some(receiver_parameter) = method.parameters.first().copied() else {
                    return Err(error("method must declare a receiver parameter"));
                };
                if arguments.len() == method.parameters.len() {
                    for (argument, parameter_type) in arguments.iter().zip(method.parameters.iter())
                    {
                        infer_expression(
                            argument,
                            scopes,
                            functions,
                            structs,
                            Some(*parameter_type),
                        )?;
                    }
                } else if arguments.len() + 1 == method.parameters.len() {
                    check_receiver_compatibility(
                        object_type,
                        receiver_parameter,
                        object,
                        scopes,
                        functions,
                        structs,
                    )?;
                    for (argument, parameter_type) in
                        arguments.iter().zip(method.parameters.iter().skip(1))
                    {
                        infer_expression(
                            argument,
                            scopes,
                            functions,
                            structs,
                            Some(*parameter_type),
                        )?;
                    }
                } else {
                    return Err(error("wrong number of method arguments"));
                }
                require_compatible(method.return_type, expected)?;
                return Ok(method.return_type);
            }
            let function_type = infer_expression(callee, scopes, functions, structs, None)?;
            let (parameters, return_type) = match function_type {
                DataType::Function(p, r) => (p, r),
                _ => return Err(error("value is not callable")),
            };
            if arguments.len() != parameters.len() {
                return Err(error("wrong number of function arguments"));
            }
            for (argument, expected_type) in arguments.iter().zip(parameters.iter()) {
                infer_expression(argument, scopes, functions, structs, Some(*expected_type))?;
            }
            require_compatible(*return_type, expected)?;
            Ok(*return_type)
        }
        Expression::Lambda {
            parameters,
            return_type,
            body,
        } => {
            let parameter_types: Vec<_> = parameters.iter().map(|(_, ty)| *ty).collect();
            let global_scope = scopes.first().cloned().unwrap_or_default();
            let outer_scopes = std::mem::replace(scopes, vec![global_scope]);
            scopes.push(HashMap::new());
            for (index, (name, data_type)) in parameters.iter().enumerate() {
                if let Some(name) = name {
                    scopes.last_mut().unwrap().insert(
                        name.clone(),
                        Symbol {
                            data_type: *data_type,
                            mutable: true,
                        },
                    );
                }
                let _ = index;
            }
            let body_result =
                infer_expression(body, scopes, functions, structs, Some(*return_type));
            *scopes = outer_scopes;
            body_result?;
            Ok(DataType::Function(
                Box::leak(parameter_types.into_boxed_slice()),
                Box::leak(Box::new(*return_type)),
            ))
        }
        Expression::StaticAccess { type_name, member } => {
            let info = structs
                .get(type_name.as_str())
                .ok_or_else(|| error(format!("unknown type '{type_name}'")))?;
            let method = info.methods.get(member);
            if let Some(method) = method {
                return Ok(DataType::Function(
                    Box::leak(method.parameters.clone().into_boxed_slice()),
                    Box::leak(Box::new(method.return_type)),
                ));
            }
            if let Some(field) = info.fields.get(member) {
                if !field.mutable && field.default.is_some() {
                    return Ok(field.data_type);
                }
            }
            Err(error("unknown static member"))
        }
        Expression::ArrayLiteral(values) => {
            let expected_element = match expected {
                Some(DataType::Array(_, element)) | Some(DataType::Slice(element)) => {
                    Some(*element)
                }
                _ => None,
            };
            let mut element_type = expected_element;
            for value in values {
                let value_type = infer_expression(value, scopes, functions, structs, element_type)?;
                if let Some(existing) = element_type {
                    require_compatible(value_type, Some(existing))?;
                } else {
                    element_type = Some(value_type);
                }
            }
            let element_type =
                element_type.ok_or_else(|| error("cannot infer empty array type"))?;
            match expected {
                Some(DataType::Array(length, element)) => {
                    if length != values.len() as u64 {
                        return Err(error(format!(
                            "array expects {length} elements, got {}",
                            values.len()
                        )));
                    }
                    require_compatible(element_type, Some(*element))?;
                    Ok(DataType::Array(length, element))
                }
                Some(DataType::Slice(element)) => {
                    require_compatible(element_type, Some(*element))?;
                    Ok(DataType::Slice(element))
                }
                None => Ok(DataType::Array(
                    values.len() as u64,
                    Box::leak(Box::new(element_type)),
                )),
                _ => Err(error("array value requires an array or slice type")),
            }
        }
        Expression::Index { object, index } => {
            let object_type = infer_expression(object, scopes, functions, structs, None)?;
            let index_type = infer_expression(index, scopes, functions, structs, None)?;
            if !index_type.is_integer() {
                return Err(error("array index must be an integer"));
            }
            match object_type {
                DataType::Array(_, element) | DataType::Slice(element) => Ok(*element),
                DataType::Pointer(element) if pointer_stride(*element, structs).is_some() => {
                    Ok(*element)
                }
                DataType::Pointer(_) => Err(error("pointer element layout is not supported")),
                _ => Err(error("indexing requires an array, slice, or pointer")),
            }
        }
        Expression::Slice { object, start, end } => {
            let object_type = infer_expression(object, scopes, functions, structs, None)?;
            for bound in start.iter().chain(end.iter()) {
                let bound_type = infer_expression(bound, scopes, functions, structs, None)?;
                if !bound_type.is_integer() {
                    return Err(error("slice bounds must be integers"));
                }
            }
            match object_type {
                DataType::Array(_, element) | DataType::Slice(element) => {
                    Ok(DataType::Slice(element))
                }
                _ => Err(error("slicing requires an array or slice")),
            }
        }
        Expression::Match { value, arms } => {
            let value_type = infer_expression(value, scopes, functions, structs, None)?;
            let enum_name = match value_type {
                DataType::Enum(name) | DataType::Struct(name) => name,
                _ => return Err(error("match requires an enum value")),
            };
            let info = structs
                .get(enum_name)
                .ok_or_else(|| error("unknown enum type"))?;
            let mut result = None;
            let mut covered = HashSet::new();
            for (pattern, body) in arms {
                let Pattern::Variant { name, binding } = pattern;
                covered.insert(name.clone());
                let payload = info
                    .variants
                    .get(name)
                    .ok_or_else(|| error(format!("unknown enum variant '{name}'")))?;
                if binding.is_some() && payload.is_none() {
                    return Err(error("variant has no payload"));
                }
                scopes.push(HashMap::new());
                if let (Some(binding), Some(payload)) = (binding, payload) {
                    scopes.last_mut().unwrap().insert(
                        binding.clone(),
                        Symbol {
                            data_type: *payload,
                            mutable: false,
                        },
                    );
                }
                let body_type =
                    infer_expression(body, scopes, functions, structs, expected.or(result))?;
                scopes.pop();
                if let Some(previous) = result {
                    if body_type != previous {
                        result = common_numeric_type(previous, body_type);
                        if result.is_none() {
                            require_compatible(body_type, Some(previous))?;
                        }
                    }
                } else {
                    result = Some(body_type);
                }
            }
            if covered.len() != info.variants.len() {
                return Err(error("match is not exhaustive"));
            }
            Ok(result.unwrap_or(DataType::Void))
        }
    }
}

fn is_addressable(expression: &Expression, scopes: &[HashMap<String, Symbol>]) -> bool {
    match expression {
        Expression::Identifier(name) => lookup(scopes, name).is_some_and(|symbol| symbol.mutable),
        Expression::Index { object, .. } => {
            matches!(object.as_ref(), Expression::Identifier(name) if lookup(scopes, name).is_some_and(|symbol| symbol.mutable))
        }
        _ => false,
    }
}

fn check_receiver_compatibility(
    actual: DataType,
    expected: DataType,
    expression: &Expression,
    scopes: &mut Vec<HashMap<String, Symbol>>,
    functions: &HashMap<String, Function>,
    structs: &HashMap<&'static str, StructInfo>,
) -> Result<(), TypeError> {
    if actual == expected {
        return Ok(());
    }
    match (actual, expected) {
        (DataType::Struct(actual_name), DataType::Pointer(expected_inner))
            if *expected_inner == DataType::Struct(actual_name) =>
        {
            let address = Expression::AddressOf(Box::new(expression.clone()));
            let inferred = infer_expression(&address, scopes, functions, structs, None)?;
            require_compatible(inferred, Some(expected))
        }
        (DataType::Pointer(actual_inner), DataType::Struct(expected_name))
            if *actual_inner == DataType::Struct(expected_name) =>
        {
            Ok(())
        }
        _ => require_compatible(actual, Some(expected)),
    }
}

fn pointer_stride(data_type: DataType, structs: &HashMap<&'static str, StructInfo>) -> Option<u64> {
    type_storage_stride(data_type, structs, &mut HashSet::new())
}

fn type_storage_stride(
    data_type: DataType,
    structs: &HashMap<&'static str, StructInfo>,
    visiting: &mut HashSet<&'static str>,
) -> Option<u64> {
    if data_type.is_numeric() || data_type == DataType::Bool {
        return Some(u64::from(data_type.bit_width().div_ceil(8)));
    }
    match data_type {
        DataType::Pointer(_) | DataType::Function(_, _) => Some(8),
        DataType::Array(length, element) if length > 0 => {
            length.checked_mul(type_storage_stride(*element, structs, visiting)?)
        }
        DataType::Struct(name) | DataType::Enum(name) => {
            let info = structs.get(name)?;
            if !info.variants.is_empty() {
                return Some(if info.variants.values().any(Option::is_some) {
                    16
                } else {
                    8
                });
            }
            if !visiting.insert(name) {
                return None;
            }
            let mut slots = 0u64;
            for field in info.fields.values() {
                let bytes = match field.data_type {
                    DataType::Array(_, _) | DataType::Struct(_) | DataType::Enum(_) => {
                        type_storage_stride(field.data_type, structs, visiting)?
                    }
                    _ => 8,
                };
                slots = slots.checked_add(bytes.div_ceil(8))?;
            }
            visiting.remove(name);
            slots.checked_mul(8)
        }
        _ => None,
    }
}

fn is_constant_expression(expression: &Expression) -> bool {
    match expression {
        Expression::LiteralNumber(_) | Expression::LiteralBool(_) => true,
        Expression::ArrayLiteral(values) => values.iter().all(is_constant_expression),
        Expression::Negate(value) | Expression::Not(value) => is_constant_expression(value),
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
        | Expression::Or(lhs, rhs) => is_constant_expression(lhs) && is_constant_expression(rhs),
        _ => false,
    }
}

fn infer_numeric_pair(
    lhs: &Expression,
    rhs: &Expression,
    scopes: &mut Vec<HashMap<String, Symbol>>,
    functions: &HashMap<String, Function>,
    structs: &HashMap<&'static str, StructInfo>,
    expected: Option<DataType>,
) -> Result<(DataType, DataType), TypeError> {
    let lhs_is_literal = is_numeric_literal(lhs);
    let rhs_is_literal = is_numeric_literal(rhs);

    let (lhs_type, rhs_type) = if lhs_is_literal && !rhs_is_literal {
        let rhs_type = infer_expression(rhs, scopes, functions, structs, expected)?;
        let lhs_type = infer_expression(lhs, scopes, functions, structs, Some(rhs_type))
            .or_else(|_| infer_expression(lhs, scopes, functions, structs, None))?;
        (lhs_type, rhs_type)
    } else {
        let lhs_type = infer_expression(lhs, scopes, functions, structs, expected)?;
        let rhs_type = infer_expression(rhs, scopes, functions, structs, Some(lhs_type))
            .or_else(|_| infer_expression(rhs, scopes, functions, structs, None))?;
        (lhs_type, rhs_type)
    };

    if !lhs_type.is_numeric() || !rhs_type.is_numeric() {
        return Err(error(format!(
            "expected numeric operands, found {lhs_type} and {rhs_type}"
        )));
    }
    Ok((lhs_type, rhs_type))
}

fn is_numeric_literal(expression: &Expression) -> bool {
    matches!(expression, Expression::LiteralNumber(_))
        || matches!(expression, Expression::Negate(inner) if matches!(inner.as_ref(), Expression::LiteralNumber(_)))
}

fn infer_number(text: &str, expected: Option<DataType>) -> Result<DataType, TypeError> {
    if text.contains('.') {
        let value = text
            .parse::<f64>()
            .map_err(|_| error(format!("invalid floating-point literal '{text}'")))?;
        let data_type = match expected {
            Some(data_type) if data_type.is_float() => data_type,
            Some(data_type) => {
                return Err(error(format!(
                    "floating-point literal cannot have type {data_type}"
                )));
            }
            None => DataType::F64,
        };
        if !value.is_finite() || value.abs() > data_type.max_float_value() {
            return Err(error(format!(
                "literal '{text}' is out of range for {data_type}"
            )));
        }
        Ok(data_type)
    } else {
        let value = text
            .parse::<u128>()
            .map_err(|_| error(format!("integer literal '{text}' is out of range")))?;
        let data_type = match expected {
            Some(data_type) if data_type.is_integer() => data_type,
            Some(data_type) if data_type.is_float() => {
                if value as f64 > data_type.max_float_value() {
                    return Err(error(format!(
                        "literal '{text}' is out of range for {data_type}"
                    )));
                }
                return Ok(data_type);
            }
            Some(data_type) => {
                return Err(error(format!(
                    "numeric literal cannot have type {data_type}"
                )));
            }
            None if value <= i32::MAX as u128 => DataType::I32,
            None if value <= i64::MAX as u128 => DataType::I64,
            None if value <= u64::MAX as u128 => DataType::U64,
            None => return Err(error(format!("integer literal '{text}' is out of range"))),
        };
        if !i128::try_from(value)
            .ok()
            .is_some_and(|value| data_type.contains_integer(value))
        {
            return Err(error(format!(
                "literal '{text}' is out of range for {data_type}"
            )));
        }
        Ok(data_type)
    }
}

fn infer_negative_integer(text: &str, expected: Option<DataType>) -> Result<DataType, TypeError> {
    let magnitude = text
        .parse::<i128>()
        .map_err(|_| error(format!("integer literal '-{text}' is out of range")))?;
    let value = -magnitude;
    let data_type = match expected {
        Some(data_type) if data_type.is_integer() => data_type,
        Some(data_type) if data_type.is_float() => {
            if magnitude as f64 > data_type.max_float_value() {
                return Err(error(format!(
                    "literal '-{text}' is out of range for {data_type}"
                )));
            }
            return Ok(data_type);
        }
        Some(data_type) => {
            return Err(error(format!(
                "numeric literal cannot have type {data_type}"
            )));
        }
        None if value >= i32::MIN as i128 => DataType::I32,
        None if value >= i64::MIN as i128 => DataType::I64,
        None => return Err(error(format!("integer literal '-{text}' is out of range"))),
    };
    if value < 0 && !data_type.contains_integer(value) {
        return Err(error(format!(
            "literal '-{text}' is out of range for {data_type}"
        )));
    }
    Ok(data_type)
}

fn common_numeric_type(lhs: DataType, rhs: DataType) -> Option<DataType> {
    if lhs == rhs {
        return Some(lhs);
    }
    if lhs.is_float() && rhs.is_float() {
        return Some(if lhs.float_rank() >= rhs.float_rank() {
            lhs
        } else {
            rhs
        });
    }
    if lhs.is_float() || rhs.is_float() {
        return Some(if lhs.is_float() { lhs } else { rhs });
    }
    if lhs.is_integer() && rhs.is_integer() {
        if lhs.is_signed_integer() == rhs.is_signed_integer() {
            return Some(if lhs.bit_width() >= rhs.bit_width() {
                lhs
            } else {
                rhs
            });
        }
        let signed = if lhs.is_signed_integer() { lhs } else { rhs };
        let unsigned = if lhs.is_signed_integer() { rhs } else { lhs };
        return (signed.bit_width() > unsigned.bit_width()).then_some(signed);
    }
    None
}

fn require_compatible(actual: DataType, expected: Option<DataType>) -> Result<(), TypeError> {
    if let Some(expected) = expected
        && actual != expected
        && !matches!((actual, expected), (DataType::Struct(a), DataType::Enum(b)) | (DataType::Enum(a), DataType::Struct(b)) if a == b)
        && !(actual.is_integer() && expected.is_float())
    {
        return Err(error(format!("expected {expected}, found {actual}")));
    }
    Ok(())
}

fn error(message: impl Into<String>) -> TypeError {
    TypeError {
        message: message.into(),
    }
}

impl DataType {
    fn is_integer(self) -> bool {
        matches!(
            self,
            Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
        )
    }

    fn is_signed_integer(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    fn is_float(self) -> bool {
        matches!(self, Self::F8 | Self::F16 | Self::F32 | Self::F64)
    }

    fn is_numeric(self) -> bool {
        self.is_integer() || self.is_float()
    }

    fn float_rank(self) -> u8 {
        match self {
            Self::F8 => 1,
            Self::F16 => 2,
            Self::F32 => 3,
            Self::F64 => 4,
            _ => 0,
        }
    }

    fn bit_width(self) -> u8 {
        match self {
            Self::U8 | Self::I8 | Self::F8 => 8,
            Self::U16 | Self::I16 | Self::F16 => 16,
            Self::U32 | Self::I32 | Self::F32 => 32,
            Self::U64 | Self::I64 | Self::F64 => 64,
            Self::Bool => 1,
            Self::Void => 0,
            Self::Struct(_) => 0,
            Self::Array(_, _) | Self::Slice(_) => 0,
            Self::Enum(_) => 0,
            Self::Function(_, _) | Self::SelfType | Self::Pointer(_) => 0,
        }
    }

    fn contains_integer(self, value: i128) -> bool {
        match self {
            Self::U8 => (0..=u8::MAX as i128).contains(&value),
            Self::U16 => (0..=u16::MAX as i128).contains(&value),
            Self::U32 => (0..=u32::MAX as i128).contains(&value),
            Self::U64 => value >= 0 && value as u128 <= u64::MAX as u128,
            Self::I8 => (i8::MIN as i128..=i8::MAX as i128).contains(&value),
            Self::I16 => (i16::MIN as i128..=i16::MAX as i128).contains(&value),
            Self::I32 => (i32::MIN as i128..=i32::MAX as i128).contains(&value),
            Self::I64 => (i64::MIN as i128..=i64::MAX as i128).contains(&value),
            _ => false,
        }
    }

    fn max_float_value(self) -> f64 {
        match self {
            Self::F8 => 448.0,
            Self::F16 => 65_504.0,
            Self::F32 => f32::MAX as f64,
            Self::F64 => f64::MAX,
            _ => 0.0,
        }
    }
}

impl std::fmt::Display for DataType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F8 => "f8",
            Self::F16 => "f16",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "bool",
            Self::Void => "void",
            Self::Struct(name) => name,
            Self::Array(length, element) => return write!(formatter, "[{length}]{element}"),
            Self::Slice(element) => return write!(formatter, "[]{element}"),
            Self::Enum(name) => name,
            Self::SelfType => "Self",
            Self::Function(parameters, return_type) => {
                return write!(formatter, "fn({parameters:?}){return_type}");
            }
            Self::Pointer(element) => return write!(formatter, "*{element}"),
        };
        formatter.write_str(name)
    }
}
