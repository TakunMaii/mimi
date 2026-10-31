use std::env;

fn ranges(predicate: fn(char) -> bool) -> Vec<(u32, u32)> {
    let mut result = Vec::new();
    let mut start = None;
    let mut previous = 0;

    for value in 0..=0x10ffff {
        let accepted = char::from_u32(value).is_some_and(predicate);
        if accepted {
            if start.is_none() {
                start = Some(value);
            }
            previous = value;
        } else if let Some(first) = start.take() {
            result.push((first, previous));
        }
    }
    if let Some(first) = start {
        result.push((first, previous));
    }
    result
}

fn emit(name: &str, entries: &[(u32, u32)]) {
    println!("static const MimiUnicodeRange {name}[] = {{");
    for (start, end) in entries {
        println!("    {{ UINT32_C({start}), UINT32_C({end}) }},");
    }
    println!("}};");
}

fn main() {
    let output = env::args().nth(1).unwrap_or_else(|| "c".to_owned());
    let alphabetic = ranges(char::is_alphabetic);
    let alphanumeric = ranges(char::is_alphanumeric);
    if output == "counts" {
        println!("alphabetic={} alphanumeric={}", alphabetic.len(), alphanumeric.len());
        return;
    }

    println!("typedef struct {{ uint32_t first; uint32_t last; }} MimiUnicodeRange;");
    emit("MIMI_UNICODE_ALPHABETIC", &alphabetic);
    emit("MIMI_UNICODE_ALPHANUMERIC", &alphanumeric);
    println!("static int mimi_unicode_in_ranges(uint32_t value, const MimiUnicodeRange *ranges, size_t count) {{");
    println!("    size_t low = 0, high = count;");
    println!("    while (low < high) {{");
    println!("        size_t middle = low + (high - low) / 2;");
    println!("        if (value < ranges[middle].first) high = middle;");
    println!("        else if (value > ranges[middle].last) low = middle + 1;");
    println!("        else return 1;");
    println!("    }}");
    println!("    return 0;");
    println!("}}");
    println!("int mimi_unicode_alphabetic(uint64_t value) {{");
    println!("    return value <= UINT32_MAX && mimi_unicode_in_ranges((uint32_t)value, MIMI_UNICODE_ALPHABETIC, sizeof(MIMI_UNICODE_ALPHABETIC) / sizeof(MIMI_UNICODE_ALPHABETIC[0]));");
    println!("}}");
    println!("int mimi_unicode_alphanumeric(uint64_t value) {{");
    println!("    return value <= UINT32_MAX && mimi_unicode_in_ranges((uint32_t)value, MIMI_UNICODE_ALPHANUMERIC, sizeof(MIMI_UNICODE_ALPHANUMERIC) / sizeof(MIMI_UNICODE_ALPHANUMERIC[0]));");
    println!("}}");
}
