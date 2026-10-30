# C Interoperability Roadmap

The goal is Zig-like direct use of C libraries through Clang-backed header
imports while preserving C types, layout, and target ABI rules.

## Compiler-Driven Header Imports

- [x] Parse `alias :: @cimport("header.h")` and expand declarations through Clang.
  - [x] Pass the selected target triple and include paths to Clang.
  - [x] Import C scalar function prototypes and retain their original link names.
  - [x] Report Clang failures and reject headers without supported prototypes.
  - [x] Pass defines and sysroot configuration through the CLI.
  - [x] Cache parsed headers within a compiler process by Clang version, header content/path, target, and flags.
- [x] Expose scalar C typedef names as reusable Mimi types.
  - [x] Resolve scalar typedefs while importing function prototypes.
  - [x] Expose typedef aliases and target-sized integer widths to Mimi code.
  - [x] Expose pointer and record typedef aliases as first class types where their pointee types are supported.
  - [x] Import named C enum tags as their Clang fixed integer type or target-default `int` representation.
  - [x] Import integer-valued C enum constants.
  - [ ] Preserve C enum types and their underlying representation.
- [x] Parse ordinary C pointer and function-pointer spellings.
  - [ ] Pass C function pointers through calls using a C ABI representation.
  - [ ] Preserve C qualifiers and calling convention metadata.
- [x] Import forward declarations as opaque C types usable through pointers.
  - [x] Resolve pointers to complete imported C struct types.
- [x] Pass string literals as NUL-terminated C `char *` arguments.
  - [ ] Add explicit conversions for mutable buffers and general Mimi slices.

## C Layout And ABI

- [ ] Add an explicit C type representation that remains distinct from Mimi types.
- [x] Query target C record layouts from Clang, including size, alignment, and offsets for supported named structs.
- [x] Import simple named C struct and union fields with scalar, pointer, and nested record members.
  - [ ] Support bitfields.
- [ ] Classify C aggregate arguments and returns for each supported target ABI.
- [ ] Support C global variables and thread-local declarations.
- [ ] Add cross-target ABI tests for scalar and aggregate C calls.

## Advanced C Declarations

- [x] Import simple numeric macro constants through Clang's preprocessor output.
  - [x] Provide source level `@cDefine` configuration.
- [ ] Support variadic C functions and default argument promotions.
- [ ] Support Mimi callbacks passed to C, with explicit lifetime/trampoline rules.
- [x] Build and link C source files alongside Mimi modules with `--c-source`.
- [x] Document Clang discovery, target/sysroot setup, linking, and unsupported constructs.
