# C Interoperability Roadmap

The goal is Zig-like direct use of C libraries through Clang-backed header
imports while preserving C types, layout, and target ABI rules.

## Compiler-Driven Header Imports

- [x] Parse `alias :: @cimport("header.h")` and expand declarations through Clang.
  - [x] Pass the selected target triple and include paths to Clang.
  - [x] Import C scalar function prototypes and retain their original link names.
  - [x] Report Clang failures and reject headers without supported prototypes.
  - [ ] Pass defines and sysroot configuration through the CLI.
  - [ ] Cache parsed headers using content, Clang version, target, and flags.
- [ ] Import typedefs, target-sized integer aliases, enums, and integer constants.
- [ ] Preserve C pointer and function-pointer types, qualifiers, and calling convention.
- [ ] Import forward declarations as opaque C types usable through pointers.
- [ ] Add explicit conversions for C strings, buffers, and Mimi slices.

## C Layout And ABI

- [ ] Add an explicit C type representation that remains distinct from Mimi types.
- [ ] Query target C record layouts from Clang, including size, alignment, and offsets.
- [ ] Support C struct/union fields and nested C records.
- [ ] Classify C aggregate arguments and returns for each supported target ABI.
- [ ] Support C global variables and thread-local declarations.
- [ ] Add cross-target ABI tests for scalar and aggregate C calls.

## Advanced C Declarations

- [ ] Import supported macro constants and provide `@cDefine` configuration.
- [ ] Support variadic C functions and default argument promotions.
- [ ] Support Mimi callbacks passed to C, with explicit lifetime/trampoline rules.
- [ ] Build and link C source files alongside Mimi modules.
- [ ] Document Clang discovery, target/sysroot setup, linking, and unsupported constructs.
