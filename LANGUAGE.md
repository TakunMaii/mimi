# Mimi Language Syntax

This document describes the syntax currently accepted by the compiler. The
language is whitespace-separated: newlines are not significant, and semicolons
are not statement terminators. Braces delimit blocks and declarations.

## Declarations

```mimi
answer := 42
limit : i32 = 100
origin :: 0

bump :: fn () void {
    answer = answer + 1
}

add :: fn (left : i32, right : i32) i32 {
    left + right
}

Point :: struct {
    x : f32,
    y : f32,
}

Option :: enum {
    some(i32),
    none,
}
```

`:=` declares an inferred local, `: Type =` declares a typed local, `=` assigns
to an existing mutable local, and `::` defines a compile-time constant or
top-level declaration. Public declarations use the `pub` prefix. Imports use
`alias :: @import("path")` and C headers use `alias :: @cimport("header.h")`.

At module scope the same spellings declare globals: `name := value` and
`name : Type = value` define a mutable global with a constant initializer, and
`name :: value` defines a compile-time constant that is inlined at each use.
Globals currently support numeric and boolean scalars, may be read and assigned
from function bodies, and are initialized before `main` runs. Executable
statements are not allowed at module scope; wrap them in a function.

## Types

The built-in scalar types are `bool`, `void`, signed and unsigned integers
`i8`–`i64` and `u8`–`u64`, and floats `f8`, `f16`, `f32`, and `f64`. Composite
type forms include pointers (`*T`), fixed arrays (`[N]T`), slices (`[]T`),
tuples (`(T, U)`), and function types (`fn(T, U)R`). A one-element tuple type
requires a trailing comma, for example `(i32,)`. Generic struct and enum
declarations use `<T>` parameters and are instantiated as `Pair<i32>`.

## Expressions and precedence

From lowest to highest precedence, binary operators are:

| Operators | Associativity |
| --- | --- |
| `||` | left |
| `&&` | left |
| `==`, `!=` | left |
| `<`, `<=`, `>`, `>=` | left |
| `\|` | left |
| `^` | left |
| `&` | left |
| `<<`, `>>` | left |
| `+`, `-` | left |
| `*`, `/`, `%` | left |

Prefix `-`, `!`, `~`, `&`, and `*` bind more tightly than binary operators.
Casts use `value as Type`. Calls, indexing, slicing, member access, and casts
can be chained. Comparisons are not chainable; parenthesize when expressing a
comparison of comparison results.

Bitwise operators require integer operands. `~` is the bitwise complement and
`!` the boolean negation. Shifts use the low six bits of the right operand
(counts 0–63); `>>` is arithmetic for signed left operands and logical for
unsigned ones. `&` is both the address-of prefix and the bitwise-and operator:
a `&` that follows an expression continues it as bitwise-and, so a bare
address-of expression that follows another statement should be parenthesized.

## Control flow and ranges

`if`, `while`, `do { ... } while condition`, `loop`, `break`, `continue`, and
`defer` are supported. `for item in collection { ... }` iterates arrays and
slices. Integer range loops use a half-open range and require signed integer
bounds of the same type:

```mimi
for index in 0..10 {
    work(index)
}
```

The end value is excluded. Range expressions are currently supported in `for`
headers, not as general first-class values. Array and slice ranges are also
half-open and may omit either bound:

```mimi
middle := values[2..5]
prefix := values[..5]
suffix := values[2..]
whole := values[..]
```

The older colon slice spelling (`values[2:5]`, including open bounds) remains
supported for compatibility.

Enum `match` expressions use variant patterns (`some(value) => value`) and
must cover every variant. Conditional expressions use `if condition { ... }
else { ... }`.

## Lexical forms

Identifiers may contain Unicode alphabetic/numeric characters and underscores.
Numbers begin with a decimal digit; decimal fractions use a dot. Strings use
double quotes and currently support `\"`, `\\`, `\n`, `\r`, and `\t` escapes.
Character literals use single quotes and denote exactly one byte, either a
literal ASCII character (`'a'`) or an escape (`'\n'`, `'\r'`, `'\t'`, `'\\'`,
`'\''`, `'\"'`, `'\0'`). A character literal is an integer literal with the
byte's value, so it can be compared with `u8` values or assigned to any integer
type.
Line comments begin with `//`; block comments use `/* ... */`.
