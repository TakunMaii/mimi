# Self-hosted compiler parity

The Rust compiler remains the default. Run `python3 mimi-self/bootstrap.py
mimi-self/src/compiler.mi --output-dir /private/tmp/mimi-bootstrap` to build
three stages, run the behavior suite, and compare stage1/stage2 output written
to the same path. The same-path comparison avoids differences caused solely
by the output filename.

## Verified subset

- Stage1 and stage2 build the compiler and pass the Python behavior suite.
- The self-hosted backend handles scalar control flow, imports and public
  functions, plain structs and methods, tuples and destructuring, byte slices,
  byte-slice collection loops, scalar extern calls, and the tested pointer
  operations.
- Local and imported generic enums with scalar payloads support constructors,
  exhaustive matches, and `if` variant tests when the scrutinee is an identifier.
- Stage1 and stage2 produced identical SHA-256 hashes when building the same
  compiler source to the same output path in the bootstrap check. Stage2 and
  stage3 binaries built under different output names had equal sizes but
  different hashes, so filename-independent artifact comparison is not yet
  established.

## Remaining differences

- The C backend currently maps integer widths and signedness to a single
  `int64_t` representation. Exact integer range, overflow, and ABI parity are
  unresolved.
- Collection loops are implemented for byte slices; other collection element
  types are not yet lowered.
- Match and enum `if` lowering require identifier scrutinees. General
  expression scrutinees need single-evaluation temporary storage.
- `defer`, struct defaults, generic structs, callbacks, and broad C ABI/C
  import coverage remain incomplete.
- The self-hosted diagnostics use `E_*` codes and often point at the exact
  identifier, while Rust stage0 uses lowercase categories and sometimes points
  at the declaration. The behavior suite checks each compiler's expected span
  and compares the error category.
- A generic enum return type from another module is accepted by the
  self-hosted compiler in the dedicated module test; Rust stage0 currently
  rejects that example with `unknown generic type 'Result'`. The test is
  consequently stage1/stage2-specific until the Rust behavior is reconciled.

The parity matrix in `FEATURES.md` and unchecked items in `TODO.md` remain the
source of truth for unfinished language coverage.
