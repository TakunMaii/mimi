#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CType {
    Void,
    Bool,
    Integer {
        bits: u16,
        signed: bool,
    },
    Float {
        bits: u16,
    },
    Pointer(Box<CType>),
    Array {
        length: Option<u64>,
        element: Box<CType>,
    },
    Function {
        parameters: Vec<CType>,
        result: Box<CType>,
        variadic: bool,
    },
    Record {
        kind: String,
        name: String,
    },
    Enum(String),
    Typedef(String),
    Qualified {
        qualifiers: Vec<String>,
        base: Box<CType>,
    },
    Unknown(String),
}

pub(crate) fn from_clang_spelling(spelling: &str) -> CType {
    let spelling = spelling.trim();
    if let Some(pointed_to) = spelling.strip_suffix('*') {
        return CType::Pointer(Box::new(from_clang_spelling(pointed_to)));
    }
    if let Some((base, qualifier)) = spelling.rsplit_once(' ')
        && matches!(qualifier, "const" | "volatile" | "restrict" | "_Atomic")
    {
        return CType::Qualified {
            qualifiers: vec![qualifier.to_string()],
            base: Box::new(from_clang_spelling(base)),
        };
    }
    let mut base = spelling;
    let mut qualifiers = Vec::new();
    loop {
        let Some((qualifier, rest)) = base.split_once(' ') else {
            break;
        };
        if matches!(qualifier, "const" | "volatile" | "restrict" | "_Atomic") {
            qualifiers.push(qualifier.to_string());
            base = rest.trim_start();
        } else {
            break;
        }
    }
    let parsed = parse_base(base);
    if qualifiers.is_empty() {
        parsed
    } else {
        CType::Qualified {
            qualifiers,
            base: Box::new(parsed),
        }
    }
}

fn parse_base(spelling: &str) -> CType {
    let spelling = spelling.trim();
    if let Some((base, array)) = spelling.rsplit_once('[')
        && let Some(length) = array.strip_suffix(']')
    {
        return CType::Array {
            length: (!length.is_empty()).then(|| length.parse().ok()).flatten(),
            element: Box::new(from_clang_spelling(base)),
        };
    }
    if let Some((base, pointer_suffix)) = spelling.split_once(" (*")
        && let Some(parameters) = pointer_suffix
            .strip_prefix(")(")
            .and_then(|rest| rest.strip_suffix(')'))
    {
        let result = from_clang_spelling(base);
        let (parameters, variadic) = parse_parameters(parameters);
        return CType::Pointer(Box::new(CType::Function {
            parameters,
            result: Box::new(result),
            variadic,
        }));
    }
    if let Some(pointer) = spelling.strip_suffix('*') {
        return CType::Pointer(Box::new(from_clang_spelling(pointer)));
    }
    if let Some((result, parameters)) = split_function_type(spelling) {
        let result = from_clang_spelling(result);
        let (parameters, variadic) = parse_parameters(parameters);
        return CType::Function {
            parameters,
            result: Box::new(result),
            variadic,
        };
    }
    if let Some(name) = spelling.strip_prefix("struct ") {
        return CType::Record {
            kind: "struct".into(),
            name: name.trim().into(),
        };
    }
    if let Some(name) = spelling.strip_prefix("union ") {
        return CType::Record {
            kind: "union".into(),
            name: name.trim().into(),
        };
    }
    if let Some(name) = spelling.strip_prefix("enum ") {
        return CType::Enum(name.trim().into());
    }
    match spelling {
        "void" => CType::Void,
        "_Bool" | "bool" => CType::Bool,
        "char" | "signed char" | "int8_t" => CType::Integer {
            bits: 8,
            signed: true,
        },
        "unsigned char" | "uint8_t" => CType::Integer {
            bits: 8,
            signed: false,
        },
        "short" | "short int" | "signed short" | "signed short int" | "int16_t" => CType::Integer {
            bits: 16,
            signed: true,
        },
        "unsigned short" | "unsigned short int" | "uint16_t" => CType::Integer {
            bits: 16,
            signed: false,
        },
        "int" | "signed" | "signed int" | "int32_t" => CType::Integer {
            bits: 32,
            signed: true,
        },
        "unsigned" | "unsigned int" | "uint32_t" => CType::Integer {
            bits: 32,
            signed: false,
        },
        "long" | "long int" | "signed long" | "signed long int" | "long long" | "long long int"
        | "int64_t" => CType::Integer {
            bits: 64,
            signed: true,
        },
        "unsigned long"
        | "unsigned long int"
        | "unsigned long long"
        | "unsigned long long int"
        | "uint64_t" => CType::Integer {
            bits: 64,
            signed: false,
        },
        "float" => CType::Float { bits: 32 },
        "double" => CType::Float { bits: 64 },
        _ if spelling.is_empty() => CType::Unknown(spelling.into()),
        _ => CType::Typedef(spelling.into()),
    }
}

fn split_function_type(spelling: &str) -> Option<(&str, &str)> {
    let open = spelling.find(" (")?;
    let params = spelling.get(open + 2..)?.strip_suffix(')')?;
    Some((spelling[..open].trim(), params))
}

fn parse_parameters(parameters: &str) -> (Vec<CType>, bool) {
    let parameters = parameters.trim();
    if parameters.is_empty() || parameters == "void" {
        return (Vec::new(), false);
    }
    let parts = split_top_level(parameters);
    let variadic = parts.last().is_some_and(|part| *part == "...");
    let parameters = parts
        .into_iter()
        .filter(|part| *part != "...")
        .map(from_clang_spelling)
        .collect();
    (parameters, variadic)
}

fn split_top_level(value: &str) -> Vec<&str> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut parts = Vec::new();
    for (index, character) in value.char_indices() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(value[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());
    parts
}

#[cfg(test)]
mod tests {
    use super::{CType, from_clang_spelling};

    #[test]
    fn parses_c_function_pointers_and_qualifiers_independently_of_mimi_types() {
        assert_eq!(
            from_clang_spelling("int (*)(const char *, void *)"),
            CType::Pointer(Box::new(CType::Function {
                parameters: vec![
                    CType::Pointer(Box::new(CType::Qualified {
                        qualifiers: vec!["const".into()],
                        base: Box::new(CType::Integer {
                            bits: 8,
                            signed: true
                        }),
                    })),
                    CType::Pointer(Box::new(CType::Void)),
                ],
                result: Box::new(CType::Integer {
                    bits: 32,
                    signed: true
                }),
                variadic: false,
            }))
        );
    }
}
