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
- [ ] Match Rust's complete Unicode Alphabetic/Alphanumeric identifier
  properties, including combining marks and all scripts.
- [x] Port the lexer, including UTF-8 identifiers, exact spans, escape
  validation, and useful lexical errors to the self-hosted compiler API.
- [ ] Implement the parser and AST for declarations, types, expressions,
  statements, imports, generics, and control flow.
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
- [x] Parse tuple destructuring names with atomic tuple literal initializers.
- [x] Build function definition nodes with balanced block source spans.
- [ ] Validate generic function type parameters and parse statement blocks.
- [ ] Extend tuple literal parsing to arbitrary expressions and parse tuple
  types/destructuring.
- [ ] Implement name resolution, type checking, generic specialization, and
  diagnostics.
- [x] Add a Python differential smoke suite for accepted programs and
  lexical, parse, and type errors against the Rust stage0 compiler.
- [x] Use Python tests to compare accepted/rejected programs and diagnostics
  against the Rust compiler.

## T3: Self-hosted code generation

- [ ] Implement a typed IR and lowering for the language feature matrix.
- [ ] Generate native code or a stable C representation and link programs.
- [ ] Match Rust compiler behavior for ABI, C imports, callbacks, and supported
  targets.
- [ ] Add regression coverage for runtime results, linked C programs, and
  generated artifact properties.

## T4: Bootstrap parity

- [ ] Compile the Mimi compiler with the Rust compiler (stage1).
- [ ] Compile the same compiler source with stage1 (stage2).
- [ ] Run the full Python suite under Rust, stage1, and stage2 compilers.
- [ ] Check reproducibility and document remaining differences from Rust.
- [ ] Make the self-hosted compiler the default only after parity gates pass.

## Current milestone

T1 containers, runtime, Result/Option, source file records, source-aware
diagnostic rendering, and iterative module import traversal are in place. The
T2 lexer returns token kinds and byte spans, decodes supported escapes, validates
UTF-8 sequences, classifies common errors, returns token/error spans together,
and handles member-number/decimal boundaries. Its Unicode identifier ranges
are still approximate. An arena-backed expression parser handles atom, prefix,
binary, grouped, member, indexed, sliced, and call forms with precedence.
Number/string/boolean atoms retain source spans. Named, pointer, slice,
fixed-array, nested tuple, nested function, and generic application type syntax
is parsed. Binding headers and declaration AST nodes preserve names, optional
types, initializers, and source spans. Function headers and named parameters
have source-aware nodes; callback types are supported in parameter lists.
Function definitions have balanced body spans, but body statements are not yet
composed into an AST. Atomic tuple literals and return/break/continue statement
nodes have dedicated parsers. Simple assignment and both expression/block
forms of `defer` are parsed as statement nodes; deferred block contents are
not yet recursively parsed.
Atomic tuple destructuring is represented by a dedicated name-list/value node.
Generic declarations and parameter validation, general tuple expressions and
destructuring type checks, statement blocks, and remaining control-flow
statements remain open. `if` statements and expressions retain conditions and
then/else-if/else branch spans. Match nodes preserve the scrutinee and variant
arm/binding/result spans. Block statement composition, enum exhaustiveness,
and semantic validation are not implemented.
While/loop/do-while and collection/range for loops retain their header and
body spans. Name
resolution and type checking remain open. The parser currently lives alongside
the lexer because stage0 loses imported aliases inside imported module function
bodies.

Next, parse declarations, types, and statements before name resolution and
type checking.
