# Mimi Self-Hosting TODO

The goal is a Mimi compiler implementation with feature coverage close to the
Rust compiler. The Rust compiler remains the bootstrap compiler until the
Mimi compiler can compile its own source and pass the same behavior suite in
two consecutive stages.

## T0: Build and test foundation

- [x] Create an isolated `mimi-self` project and a Python test entry point.
- [x] Implement the first executable compiler component in Mimi: lexical
  scanning for identifiers, numbers, strings, comments, and punctuation.
- [x] Test Mimi code by compiling it with the Rust compiler and running it.
- [x] Keep a feature parity matrix against `LANGUAGE.md` and the Rust tests.
- [x] Add Python helpers for compile success/failure, exit status, captured
  stdout/stderr, and temporary project setup.
- [x] Make the Python suite selectable by compiler executable so the same
  tests can run against stage0, stage1, and stage2.
- [ ] Run the stage0/stage1/stage2 bootstrap script against the self-hosted
  compiler source and review the generated artifact hashes.

## T1: Language support needed by a compiler

- [x] Add or implement reusable dynamic arrays, byte strings, maps, and
  arena-style allocation for AST and IR storage.
- [x] Provide reliable file, path, directory, and process APIs through a
  minimal standard library or a small C runtime.
- [x] Make container and process handle release idempotent and leave handles
  unusable until explicitly reinitialized.
- [x] Add `Result`/`Option` conventions and error propagation suitable for
  parser and type-checker code.
- [x] Complete source file/module loading for the self-hosted compiler,
  including relative import resolution and iterative depth-first traversal.
- [x] Track loading/loaded module states and reject cycles in a module graph.
- [x] Define source spans and render diagnostics with source context.

## T2: Self-hosted front end

- [x] Return token kinds and exact byte spans; classify keywords, punctuation,
  operators, and literal kinds.
- [x] Decode the supported string escapes and reject malformed UTF-8 in
  identifiers.
- [x] Classify common lexer failures with stable error codes and messages.
- [x] Return exact half-open byte spans for unexpected bytes, malformed
  escapes, and unterminated strings/comments.
- [x] Expose token kind, source span, and lexical error code in one result.
- [x] Reject Unicode symbol codepoints such as emoji at identifier start.
- [x] Match Rust's complete Unicode Alphabetic/Alphanumeric identifier
  properties, including combining marks and all scripts.
- [x] Port the lexer, including UTF-8 identifiers, exact spans, escape
  validation, and useful lexical errors to the self-hosted compiler API.
- [x] Parse the supported declaration, type, expression, statement, import,
  generic, and control-flow syntax into source-aware AST nodes.
- [x] Parse `extern` functions into module ASTs.
- [x] Parse C define declarations into module ASTs.
- [x] Parse grouped unary and binary expressions with operator precedence into
  arena-backed AST nodes.
- [x] Parse chained named and numeric member access in expressions.
- [x] Parse indexed expressions with full binary expression precedence.
- [x] Parse nested calls and comma-separated arguments.
- [x] Parse bounded and open-ended slices using `:` and `..` separators.
- [x] Build variable/constant declaration AST nodes from binding headers,
  parsed types, and initializer expressions.
- [x] Parse `return`, `break`, and `continue` into source-aware statement
  nodes.
- [x] Parse expression-form `defer` statements.
- [x] Parse block-form `defer` statements with body spans.
- [x] Parse simple assignment statements with expression targets and values.
- [x] Parse plain `if` statements with optional `else` block spans.
- [x] Parse `while`, `loop`, and `do-while` condition/body spans.
- [x] Parse collection and integer-range `for` headers with body spans.
- [x] Parse plain `if/else` expressions into expression nodes.
- [x] Parse match values, variant arms, optional bindings, and result spans.
- [x] Parse enum `if let` variant tests with optional bindings.
- [x] Parse generic parameter lists and reject duplicate names.
- [x] Parse enum variants and optional payload types into declaration nodes.
- [x] Parse struct fields, defaults, mutability, generic parameters, and methods;
  reject duplicate struct fields, methods, and enum variants.
- [x] Compose module declarations for bindings, functions, structs, enums,
  `@import`, and `@cimport`, including public declarations.
- [x] Parse tuple destructuring names with atomic tuple literal initializers.
- [x] Build function definition nodes with balanced block source spans.
- [x] Reject duplicate function parameter names.
- [x] Confirm generic function declarations are outside the current stage0
  syntax; generic parameters apply to structs and enums.
- [x] Compose binding, tuple destructuring, assignment, transfer, defer, and
  expression statements into block statement lists.
- [x] Recursively compose control-flow statements and nested block contents.
- [x] Extend tuple literal parsing to arbitrary expressions and parse tuple
  types and destructuring bindings.
- [x] Parse `value as Type` casts for scalar, pointer, and grouped values.
- [x] Match cast precedence inside unparenthesized binary expressions.
- [x] Reject trailing commas in function type parameter lists, matching stage0.
- [x] Parse simple-name struct literal fields and nested values.
- [x] Parse module-qualified type names such as `*bytes.ByteBuffer`.
- [x] Parse generic struct literal type applications.
- [x] Validate tuple destructuring initializer arity when it is a tuple literal.
- [x] Resolve module, parameter, and local names across nested statement blocks;
  report undefined and duplicate local names with source spans.
- [x] Validate destructuring tuple element types during semantic analysis.
- [x] Check explicit and trailing expression return types for scalar functions.
- [x] Expose a front-end entry point that reports lexical, parse, name, and
  initial type diagnostics through the source renderer.
- [x] Point module parse failures at the failing declaration rather than
  always reporting byte zero.
- [ ] Implement name resolution, type checking, generic specialization, and
  diagnostics.
- [x] Add a Python differential smoke suite for accepted programs and
  lexical, parse, and type errors against the Rust stage0 compiler.
- [x] Use Python tests to compare accepted/rejected programs and diagnostics
  against the Rust compiler.
- [x] Include casts, tuple destructuring, and struct methods in the differential
  accepted-program suite.

## T3: Self-hosted code generation

- [x] Lower scalar functions, bindings, expressions, and returns
  into arena-backed typed IR with source-spanned unsupported errors.
- [x] Lower scalar parameters and direct function calls, including forward
  references and argument arity/type checks.
- [x] Emit C for this scalar subset and compile it with the first self-hosted
  `build` command.
- [x] Compare stage0 and stage1 runtime results on arithmetic, casts, and
  booleans; check rejected-program diagnostics.
- [x] Lower scalar assignments, `if`/`else`, `while`, `loop`, `do-while`,
  `break`, and `continue` to C; compare runtime behavior with stage0.
- [x] Lower scalar integer range `for` loops with loop-local read-only bindings
  and compare `continue` behavior with stage0.
- [x] Lower whole-expression scalar `if`/`else` values to typed IR and C
  conditional expressions.
- [x] Lower direct scalar `extern` declarations and calls through the host C
  linker, with stage0/stage1 runtime comparison.
- [ ] Extend typed IR and C generation to control flow, aggregates, generics,
  imports, C ABI calls, and the remaining language features.
- [ ] Implement a typed IR and lowering for the language feature matrix.
- [ ] Generate native code or a stable C representation and link programs.
- [ ] Match Rust compiler behavior for ABI, C imports, callbacks, and supported
  targets.
- [ ] Add regression coverage for runtime results, linked C programs, and
  generated artifact properties.

## T4: Bootstrap parity

- [x] Compile the Mimi compiler with the Rust compiler (stage1).
- [ ] Compile the same compiler source with stage1 (stage2).
- [ ] Run the full Python suite under Rust, stage1, and stage2 compilers.
- [ ] Check reproducibility and document remaining differences from Rust.
- [ ] Make the self-hosted compiler the default only after parity gates pass.

## Current milestone

T1 containers, runtime, Result/Option, source file records, source-aware
diagnostic rendering, and iterative module import traversal are in place. The
T2 lexer returns token kinds and byte spans, decodes supported escapes, validates
UTF-8 sequences, classifies common errors, returns token/error spans together,
and handles member-number/decimal boundaries. Unicode identifier checks use
complete Rust Alphabetic/Alphanumeric property tables. An arena-backed
expression parser handles atom, prefix, binary, grouped, member, indexed,
sliced, and call forms with precedence.
Number/string/boolean atoms retain source spans. Named, pointer, slice,
fixed-array, nested tuple, nested function, and generic application type syntax
is parsed. Binding headers and declaration AST nodes preserve names, optional
types, initializers, and source spans. Function headers and named parameters
have source-aware nodes; callback types are supported in parameter lists.
Generic parameter lists, enum variant declarations with payload types, struct
field declarations with defaults/mutability, and struct methods are parsed.
Function definitions contain source-aware body statement lists.
`parse_statement_block` composes binding, tuple destructuring, assignment,
transfer, defer, expression, if/if-let, loop, for, and match statements into
source-aware linked lists; nested control-flow and defer blocks are recursively
composed.
Tuple literals accept arbitrary expressions and nested calls; simple-name
struct literal fields and nested values are also parsed.
Return/break/continue statement nodes have dedicated parsers. Simple
assignment and both expression/block forms of `defer` are parsed as statement
nodes; deferred block contents are recursively parsed. Atomic tuple
destructuring is represented by a dedicated name-list/value node; tuple literal
initializer arity is checked, while element types require semantic analysis.
Scalar, pointer, generic, and tuple casts bind to the preceding operand in
unparenthesized binary expressions.
Generic declarations reject duplicate names, and function parameter lists
reject duplicate parameter names; checking type-parameter use remains open.
`if` statements and expressions retain conditions and then/else-if/else branch
spans. Match nodes preserve the scrutinee and variant arm/binding/result spans
and reject duplicate arms. Enum exhaustiveness and semantic validation are not
implemented.
While/loop/do-while and collection/range for loops retain their header and
recursively composed bodies. The first semantic pass resolves module, parameter,
and local names in parsed statements and reports source-spanned name errors.
An initial type pass checks scalar bindings, tuple element uses, arithmetic,
logical operations, assignments, conditions, and explicit/trailing returns in
functions and methods. `source.analyze_text` connects lexing, parsing, and
semantic checking to source-aware diagnostics. Module-level parse errors now
identify the failing declaration, but exact token-level failures inside that
declaration still need work. Full type checking and generic specialization
remain open.
The parser currently lives alongside
the lexer because stage0 loses imported aliases inside imported module function
bodies.

The first stage1 executable compiles scalar functions, direct calls, assignments,
and basic control flow through a typed IR and C backend, then invokes `cc`.
`main` still takes no parameters. The subset excludes collection `for`, match, defer,
aggregates, imports, pointer-based C ABI calls, and generic specialization; stage2 cannot yet
compile `compiler.mi`. Stage1 currently collapses integer widths and signedness,
so exact range-bound type parity remains open. Scalar `if` expressions are
supported when they form a complete expression; embedded forms remain open.
Next, extend typing and IR lowering across those features.
The stage1 front end now parses `compiler.mi` itself; its first stage2 blocker is
the `bytes :: @import("bytes.mi")` declaration, which the C backend does not
lower yet.
