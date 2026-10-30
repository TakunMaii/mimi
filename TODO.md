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
- [x] Pass top-level non-capturing Mimi functions as scalar C callbacks.
  - [ ] Support callback parameters with aggregate C ABI arguments or returns.
- [ ] Preserve C qualifiers and calling convention metadata.
- [x] Import forward declarations as opaque C types usable through pointers.
  - [x] Retain complete records with unsupported fields as opaque pointer types.
  - [x] Resolve pointers to complete imported C struct types.
- [x] Pass string literals as NUL-terminated C `char *` arguments.
- [x] Pass mutable Mimi byte-array buffers through explicit element pointers to C `char *` parameters.
  - [x] Expose general slice/array `.ptr` and `.len` for explicit C pointer/length arguments.

## C Layout And ABI

- [ ] Add an explicit C type representation that remains distinct from Mimi types.
- [x] Query target C record layouts from Clang, including size, alignment, and offsets for supported named structs.
- [x] Import simple named C struct and union fields with scalar, pointer, and nested record members.
- [x] Import signed and unsigned bitfields contained within their declared storage unit.
  - [x] Support fields contained within 8/16/32/64-bit integer storage units.
  - [ ] Support fields that cross or do not match their declared storage unit.
- [x] Pass complete C struct and union arguments and returns through Clang-generated ABI thunks.
  - [x] Compile and link aggregate calls for x86_64 and AArch64; execute on the host target.
- [x] Import and access scalar and pointer C global variables, including const write protection.
  - [x] Import mutable globals of complete structs and unions.
  - [x] Support thread-local declarations through generated C address accessors.
  - [x] Support const aggregate globals as read-only values.
- [x] Compile and link integer and floating-point scalar C calls for x86_64 and AArch64.

## Advanced C Declarations

- [x] Import simple numeric macro constants through Clang's preprocessor output.
  - [x] Provide source level `@cDefine` configuration.
- [x] Support variadic C functions and default argument promotions.
  - [x] Import declarations and call with integer variadic arguments and `f64`.
  - [x] Pass narrow integer values with C default-promotion semantics.
  - [x] Promote `f32` arguments to `f64` on x86_64 and AArch64.
- [x] Pass top-level non-capturing Mimi callbacks to C with static code lifetime.
  - [ ] Support capturing closures through explicit callback context/trampolines.
- [x] Build and link C source files alongside Mimi modules with `--c-source`.
- [x] Document Clang discovery, target/sysroot setup, linking, and unsupported constructs.
