# Self-Hosted Compiler Feature Matrix

The Rust column describes the current stage0 compiler. The self-hosted column
tracks what the Mimi implementation supports. A feature is complete only when
the Python suite covers it through the self-hosted compiler.

| Area | Current Rust compiler | Mimi implementation | Verification |
| --- | --- | --- | --- |
| Identifiers, numbers, strings, punctuation | Supported | Partial token stream; broad token categories | `tests/lexer.mi` |
| Comments and lexical errors | Supported | Partial scanner; invalid tokens use an error kind | `tests/lexer.mi` |
| Source spans and diagnostics | Supported | Not implemented | Pending |
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
