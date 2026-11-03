# Self-hosted compiler parity

The Rust compiler remains the default. Run `python3 mimi-self/bootstrap.py
mimi-self/src/compiler.mi --output-dir /private/tmp/mimi-bootstrap` to build
three stages, run the behaviour suite, and compare stage1/stage2/stage3 output
written to the same path.

## Verified subset

- Stage1 and stage2 build the compiler and pass the Python behaviour suite.
- The self-hosted backend handles scalar and floating-point control flow,
  imports and public functions, plain structs and methods, tuples and
  destructuring, byte slices and byte-slice collection loops, scalar `extern`
  calls, and the tested pointer operations.
- Local and imported generic enums with scalar payloads support constructors,
  exhaustive matches, and `if` variant tests when the scrutinee is an identifier.
- Character literals, scoped `defer`, trailing `if`/`else` block values, and
  function bodies in which every branch returns are lowered to C.
- Stage1 and stage2 produce byte-identical artifacts when they build the same
  source to the same output path.
- Artifacts built for different output names differ only in the Mach-O
  `LC_UUID` load command and the ad-hoc code signature, which Apple's linker
  derives from the output path. `bootstrap.py` zeroes exactly those load-command
  regions and verifies that the remaining bytes are identical, so compiler
  output is reproducible even though the linker stamps per-output metadata.
  Disabling the metadata is not an option: arm64 executables must stay signed
  and dyld requires `LC_UUID`.

## Remaining differences

- The C backend maps integer widths and signedness to a single `int64_t`
  representation, so unsigned arithmetic beyond `i64::MAX`, exact integer
  ranges, overflow, and ABI parity remain unresolved. Stage0 now lowers
  unsigned division and comparison correctly.
- The self-hosted compiler has no module-level variables or constants, no
  `@cimport`, no generic structs, and no closures.
- Collection loops are implemented for byte slices; other collection element
  types are not yet lowered.
- Match and enum `if` lowering require identifier scrutinees. General
  expression scrutinees need single-evaluation temporary storage.
- Transfers from a deferred block are rejected. Struct defaults, generic
  structs, callbacks, and broad C ABI/C import coverage remain incomplete.
- A trailing `if` with one returning branch and one value branch is accepted by
  stage0 but not by the self-hosted backend.
- The self-hosted diagnostics use `E_*` codes and often point at the exact
  identifier, while Rust stage0 uses lowercase categories and sometimes points
  at the declaration. The behaviour suite checks each compiler's expected span
  and compares the error category.
- A generic enum return type from another module is accepted by the
  self-hosted compiler in the dedicated module test; Rust stage0 currently
  rejects that example with `unknown generic type 'Result'`. The test is
  consequently stage1/stage2-specific until the Rust behavior is reconciled.

The parity matrix in `FEATURES.md` and unchecked items in `TODO.md` remain the
source of truth for unfinished language coverage.
