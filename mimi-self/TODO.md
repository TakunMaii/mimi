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
- [ ] Establish stage0/stage1/stage2 build commands and artifact comparison
  after the self-hosted compiler can compile programs.

## T1: Language support needed by a compiler

- [x] Add or implement reusable dynamic arrays, byte strings, maps, and
  arena-style allocation for AST and IR storage.
- [x] Provide reliable file, path, directory, and process APIs through a
  minimal standard library or a small C runtime.
- [x] Add `Result`/`Option` conventions and error propagation suitable for
  parser and type-checker code.
- [ ] Complete source file/module loading for the self-hosted compiler,
  including import resolution and recursive traversal.
- [x] Track loading/loaded module states and reject cycles in a module graph.
- [x] Define source spans and render diagnostics with source context.

## T2: Self-hosted front end

- [x] Return token kinds and exact byte spans; classify keywords, punctuation,
  operators, and literal kinds.
- [x] Decode the supported string escapes and reject malformed UTF-8 in
  identifiers.
- [x] Classify common lexer failures with stable error codes and messages.
- [x] Reject Unicode symbol codepoints such as emoji at identifier start.
- [ ] Match Rust's complete Unicode Alphabetic/Alphanumeric identifier
  properties, including combining marks and all scripts.
- [ ] Port the lexer, including UTF-8 identifiers, exact spans, escape
  validation, and useful lexical errors to the self-hosted compiler API.
- [ ] Implement the parser and AST for declarations, types, expressions,
  statements, imports, generics, and control flow.
- [ ] Implement name resolution, type checking, generic specialization, and
  diagnostics.
- [ ] Use Python tests to compare accepted/rejected programs and diagnostics
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
diagnostic rendering, and module state/cycle tracking are in place. Source
import resolution and recursive traversal are still open. The T2 lexer
returns token kinds and byte spans, decodes
supported escapes, validates UTF-8 sequences, classifies common errors, and
handles member-number/decimal boundaries. Its Unicode identifier ranges are
still approximate, and its API reports an error code/message separately from
the token result. It still lacks a reusable parser and AST. The Rust stage0
compiler currently rejects generic enum construction from inside a function
body; tests cover generic enum values at typed local declarations.

Next, implement parser token consumption and an arena-backed AST, then extend
it through precedence, grouping, declarations, types, and statements.
