# Self-Hosted Compiler Feature Matrix

The Rust column describes the current stage0 compiler. The self-hosted column
tracks what the Mimi implementation supports. A feature is complete only when
the Python suite covers it through the self-hosted compiler.

| Area | Current Rust compiler | Mimi implementation | Verification |
| --- | --- | --- | --- |
| Dynamic arrays, byte buffers, maps, arena | Supported by Rust standard library | U64 vector, byte buffer, string-to-u64 map, and arena allocation through C runtime | `tests/vector.mi`, `tests/bytes.mi`, `tests/map.mi`, `tests/arena.mi` |
| File, path, directory, process APIs | Supported by Rust standard library | Binary read/write, existence/type/size checks, directory creation, and argv-based process execution | Python integration tests |
| Result and Option | Rust `Result`/`Option` | Generic enum conventions with explicit `match` propagation | `tests/result.mi` |
| Source files and diagnostics | Compiler source map and diagnostics | File loading plus source span and diagnostic records; rendering is pending | Python source model integration test |
| Identifiers, numbers, strings, punctuation | Supported | Token kinds, keyword classification, and byte spans; string values remain raw | `tests/lexer.mi` |
| Comments and lexical errors | Supported | Line/block comments and invalid-token kind; no diagnostic text yet | `tests/lexer.mi` |
| Source spans and diagnostics | Supported | Span and diagnostic records exist; source-aware rendering is pending | Python source model integration test |
| Declarations, functions, modules | Supported | Not implemented | Pending |
| Scalar, pointer, array, slice, tuple, function types | Supported | Not implemented | Pending |
| Expressions, casts, calls, member/index/slice access | Supported | Not implemented | Pending |
| If/match, loops, ranges, break/continue, defer | Supported | Not implemented | Pending |
| Structs, enums, generics, closures | Supported | Not implemented | Pending |
| Name resolution and type checking | Supported | Not implemented | Pending |
| MIR/LIR lowering and register allocation | Supported | Not implemented | Pending |
| x86_64 and AArch64 code generation | Supported | Not implemented | Pending |
| C imports, ABI shims, native linking | Supported | Not implemented | Pending |
| Build and C-import caches | Not in current remote version | Not implemented | Pending |
| Stage1/stage2 bootstrap | Supported by Rust only | Not implemented | Pending |
