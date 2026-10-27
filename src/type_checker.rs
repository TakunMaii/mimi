use std::collections::HashMap;

use crate::parser::{ASTNode, DataType, Expression};

#[derive(Debug, PartialEq)]
pub(crate) struct TypeError {
    pub(crate) message: String,
}

#[derive(Clone, Copy)]
struct Symbol {
    data_type: DataType,
    mutable: bool,
}

pub(crate) fn check(nodes: &[ASTNode]) -> Result<HashMap<String, DataType>, Vec<TypeError>> {
    let mut scopes = vec![HashMap::new()];
    let mut errors = Vec::new();

    for node in nodes {
        if let Err(err) = check_statement(node, &mut scopes) {
            errors.push(err);
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
            match infer_expression(value, scopes, *data_type).and_then(|inferred| {
                require_compatible(inferred, *data_type)?;
                Ok(inferred)
            }) {
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
            match infer_expression(value, scopes, *data_type).and_then(|inferred| {
                require_compatible(inferred, *data_type)?;
                Ok(inferred)
            }) {
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
                if let Err(err) = infer_expression(value, scopes, Some(symbol.data_type))
                    .and_then(|inferred| require_compatible(inferred, Some(symbol.data_type)))
                {
                    Err(error(format!("assignment to '{name}': {}", err.message)))
                } else {
                    Ok(())
                }
            }
        },
        ASTNode::Expression(_) => Err(error("expression statements are only valid inside blocks")),
    }
}

fn lookup<'a>(scopes: &'a [HashMap<String, Symbol>], name: &str) -> Option<&'a Symbol> {
    scopes.iter().rev().find_map(|scope| scope.get(name))
}

fn infer_expression(
    expression: &Expression,
    scopes: &mut Vec<HashMap<String, Symbol>>,
    expected: Option<DataType>,
) -> Result<DataType, TypeError> {
    match expression {
        Expression::Identifier(name) => {
            let data_type = lookup(scopes, name)
                .map(|symbol| symbol.data_type)
                .ok_or_else(|| error(format!("use of undefined identifier '{name}'")))?;
            require_compatible(data_type, expected)?;
            Ok(expected.unwrap_or(data_type))
        }
        Expression::LiteralNumber(text) => infer_number(text, expected),
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
            let inner_type = infer_expression(inner, scopes, expected)?;
            if inner_type.is_float() || inner_type.is_signed_integer() {
                Ok(inner_type)
            } else {
                Err(error(format!(
                    "unary '-' cannot be applied to {inner_type}"
                )))
            }
        }
        Expression::Not(inner) => {
            infer_expression(inner, scopes, Some(DataType::Bool))?;
            Ok(DataType::Bool)
        }
        Expression::And(lhs, rhs) | Expression::Or(lhs, rhs) => {
            infer_expression(lhs, scopes, Some(DataType::Bool))?;
            infer_expression(rhs, scopes, Some(DataType::Bool))?;
            Ok(DataType::Bool)
        }
        Expression::Add(lhs, rhs)
        | Expression::Subtract(lhs, rhs)
        | Expression::Multiply(lhs, rhs)
        | Expression::Divide(lhs, rhs)
        | Expression::Remainder(lhs, rhs) => {
            let (lhs_type, rhs_type) = infer_numeric_pair(lhs, rhs, scopes, expected)?;
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
            let lhs_type = infer_expression(lhs, scopes, None)?;
            let rhs_type = infer_expression(rhs, scopes, Some(lhs_type)).or_else(|_| {
                let rhs_type = infer_expression(rhs, scopes, None)?;
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
            let (lhs_type, rhs_type) = infer_numeric_pair(lhs, rhs, scopes, None)?;
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
                                if is_last { expected } else { None },
                            )?;
                        }
                        _ => {
                            check_statement(item, scopes)?;
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
    }
}

fn infer_numeric_pair(
    lhs: &Expression,
    rhs: &Expression,
    scopes: &mut Vec<HashMap<String, Symbol>>,
    expected: Option<DataType>,
) -> Result<(DataType, DataType), TypeError> {
    let lhs_is_literal = is_numeric_literal(lhs);
    let rhs_is_literal = is_numeric_literal(rhs);

    let (lhs_type, rhs_type) = if lhs_is_literal && !rhs_is_literal {
        let rhs_type = infer_expression(rhs, scopes, expected)?;
        let lhs_type = infer_expression(lhs, scopes, Some(rhs_type))
            .or_else(|_| infer_expression(lhs, scopes, None))?;
        (lhs_type, rhs_type)
    } else {
        let lhs_type = infer_expression(lhs, scopes, expected)?;
        let rhs_type = infer_expression(rhs, scopes, Some(lhs_type))
            .or_else(|_| infer_expression(rhs, scopes, None))?;
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
        };
        formatter.write_str(name)
    }
}
