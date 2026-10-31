# Self-Hosted Compiler Feature Matrix

The Rust column describes the current stage0 compiler. The self-hosted column
tracks what the Mimi implementation supports. A feature is complete only when
the Python suite covers it through the self-hosted compiler.

| Area | Current Rust compiler | Mimi implementation | Verification |
| --- | --- | --- | --- |
| Dynamic arrays, byte buffers, maps, arena | Supported by Rust standard library | U64 vector, byte buffer, string-to-u64 map, and arena allocation through C runtime | `tests/vector.mi`, `tests/bytes.mi`, `tests/map.mi`, `tests/arena.mi` |
| File, path, directory, process APIs | Supported by Rust standard library | Binary read/write, existence/type/size checks, directory creation, and argv-based process execution | Python integration tests |
| Result and Option | Rust `Result`/`Option` | Generic enum conventions with explicit `match` propagation | `tests/result.mi` |
| Source files and diagnostics | Compiler source map and diagnostics | File loading, canonical relative `@import` discovery (including root-directory modules), iterative depth-first traversal, cycle detection, source spans, and rendered diagnostics | Python source and module graph integration tests |
| Identifiers, numbers, strings, punctuation | Supported | Slice and raw-buffer token APIs with token spans, escape decoding, decimal/member-number distinction; rejects Unicode symbols in identifier continuation and accepts common combining/digit ranges; full Unicode property parity pending | `tests/lexer.mi`, module graph integration |
| Comments and lexical errors | Supported | Line/block comments, categorized errors, and one result with exact token/error byte spans | `tests/lexer.mi` |
| Source spans and diagnostics | Supported | Span records and source line/caret rendering | Python source model integration test |
| Declarations, functions, modules | Partial | Variable/constant binding ASTs, struct field and enum variant declarations, generic parameter lists, atomic tuple destructuring, function definition nodes with balanced body spans, and named parameters; type nodes cover named, pointer, slice, fixed-array, nested tuple, nested function, and generic applications; struct methods, generic specialization, parameter semantics, and block statement parsing pending | `tests/parser.mi` |
| Scalar, pointer, array, slice, tuple, function types | Supported | Named, pointer, slice, fixed-array, nested tuple, nested function, and generic application syntax parsed into arena nodes with source spans; generic declarations and semantic validation pending | `tests/parser.mi` |
| Expressions, casts, calls, member/index/slice access | Supported | Number/string/boolean atoms retain exact source payload spans; plain if/else, simple-name struct literals, and tuple literals with arbitrary parsed expression items have dedicated parsers; grouped unary/binary expressions with precedence, negation, logical not, address/dereference, member access, indexing, slices, and nested calls; casts and generic struct literal names pending | `tests/parser.mi` |
| If/match, loops, ranges, break/continue, defer | Supported | Assignment and transfer statement nodes; `if`/`else if`/`else`, enum `if let`, `while`/`loop`/`do-while`, collection/range `for`, and both defer forms retain expression and block spans; match variants, optional bindings, arm expressions, and spans are parsed; block composition, loop-context/type validation, and enum exhaustiveness pending | `tests/parser.mi` |
| Structs, enums, generics, closures | Supported | Not implemented | Pending |
| Name resolution and type checking | Supported | Not implemented | Pending |
| MIR/LIR lowering and register allocation | Supported | Not implemented | Pending |
| x86_64 and AArch64 code generation | Supported | Not implemented | Pending |
| C imports, ABI shims, native linking | Supported | Not implemented | Pending |
| Build and C-import caches | Not in current remote version | Not implemented | Pending |
| Stage1/stage2 bootstrap | Supported by Rust only | Not implemented | Pending |
