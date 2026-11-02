# Mimi Self-Hosting

This directory is the workspace for implementing the Mimi compiler in Mimi.
The Rust compiler at the repository root is currently the stage0 bootstrap
compiler.

Run the Python tests from the repository root:

```sh
python3 mimi-self/test.py
```

The same suite accepts an alternate compiler executable with
`python3 mimi-self/test.py --compiler /path/to/mimi`.
When `--compiler` is set, the suite also compares accepted/rejected smoke
cases and their first diagnostic against the Rust stage0 compiler. Pass
`--reference-compiler /path/to/stage0` to select a different reference binary.

The first stage1 compiler can be built with:

```sh
target/debug/mimi build mimi-self/src/compiler.mi -o /tmp/mimi-stage1 --c-source mimi-self/runtime.c --no-cache
/tmp/mimi-stage1 build scalar.mi -o scalar --no-cache
```

Stage1 currently accepts scalar helper functions, parameters, direct calls,
bindings, assignments, arithmetic, boolean expressions, numeric casts, returns,
`if`/`else`, `while`, `loop`, `do-while`, integer range `for`, `break`, and `continue`. `main` still
takes no parameters. It writes temporary C, invokes the host `cc`, and removes
the C file. Direct scalar `extern` calls can link against host C symbols.
Collection `for`, match, defer, aggregates, imports, pointer-based C ABI calls, and generics
remain unsupported in stage1.
Whole-expression scalar `if`/`else` values are also supported; embedded `if`
expressions remain outside the current stage1 subset.
The Python suite checks stage1 against stage0 on supported programs.

Once stage1 can compile its own source, test both bootstrap stages with:

```sh
python3 mimi-self/bootstrap.py mimi-self/src/compiler.mi --output-dir /tmp/mimi-bootstrap
```

The script builds stage1 with stage0, builds stage2 with stage1, runs the test
suite under each stage, and reports each compiler artifact's size and SHA-256
for reproducibility comparisons.

The current milestone includes a Unicode-aware lexer, arena-backed expression
and type parsing, declarations for functions/structs/enums, tuple destructuring,
casts, and source-aware statement ASTs. Function bodies recursively retain
bindings, assignments, expressions, control flow, match, and defer blocks.
An initial semantic pass resolves local names and checks scalar, tuple element,
and return types. `source.analyze_text` returns a diagnostic for lexical,
parse, name, or initial type errors, which `source.render` can display with
source context. Full type checking and code generation are still open; stage0 is
the Rust compiler. See
[TODO.md](TODO.md) for the parity roadmap and [FEATURES.md](FEATURES.md) for
the feature matrix.

`lexer.error_code` returns 1 for an unexpected character, 2 for an
unterminated block comment, 3 for an unterminated string, 4 for an invalid
escape, and 5 for malformed UTF-8 in an identifier. `lexer.error_span` returns
the corresponding half-open byte range in the source. `lexer.lex_next` combines
the token kind, span, and error code in one `LexResult`.
The C runtime's Unicode identifier tables are generated from Rust's Unicode
`Alphabetic` and `Alphanumeric` character properties by
`mimi-self/tools/generate_unicode_tables.rs`. From the repository root, emit a
fresh table block with:

```sh
rustc mimi-self/tools/generate_unicode_tables.rs -o /tmp/mimi-unicode-tables
/tmp/mimi-unicode-tables > /tmp/mimi-unicode-tables.c
```
Use that output to replace the generated table block in `mimi-self/runtime.c`;
do not append it, since the runtime already contains the generated definitions.
`lexer.expression_payload` returns the original source spelling for identifier,
number, and string literal nodes; string escapes remain encoded in that view.
`lexer.parse_module` builds a source-spanned declaration list for bindings,
functions, structs, enums, extern functions, `@import`, `@cimport`, and
`@cDefine`. Extern functions use a `FunctionNode` with a null body; ordinary
function and struct method bodies contain recursively composed statement lists.

`source.render` writes a diagnostic with its path, one-based line and column,
source line, and byte-span underline to standard error. `source.load_graph`
discovers string-literal `@import` paths, resolves them relative to each module,
and traverses the graph iteratively while detecting import cycles. Its module
records retain canonical paths, source bytes, and import alias edges until
`module_graph_release`; `module_file_lookup` resolves an edge's canonical path.
