# Compiler TODO

Items are ordered by dependency: the IR and target foundations come before the
language features that depend on them. A checked item has an implementation
and focused regression coverage; it does not imply support on every target.

## IR and Control Flow

- [x] Correctly lower early `return` from nested blocks and conditional branches.
- [x] Lower unconditional `loop` bodies into CFG backedges.
- [x] Lower `while` and `do-while` into CFG blocks for basic loop bodies.
- [x] Lower `for` over fixed-size scalar arrays.
- [x] Lower `for` over slices taken directly from fixed-size scalar arrays.
- [x] Lower `for` over local slice values, slice parameters, and string slices.
- [x] Lower `break` and `continue`, including loop-body `defer` exits.
- [x] Lower `defer` at each scope exit and return path in LIFO order.
- [x] Lower `match` and `if-let` for payloadless enums.
- [x] Lower one-slot enum payload construction and bind payloads in `match` and `if-let`.

## Values and Memory

- [ ] Implement floating-point literals, arithmetic, comparisons, and ABI handling.
  - [x] Support f32/f64 scalar literals, arithmetic, comparisons, function arguments, and returns.
  - [x] Support f16 literals, arithmetic, comparisons, function arguments, and returns.
  - [x] Reserve `f8` as a keyword and retain it as a type placeholder.
  - [ ] Define an f8 encoding and support its lowering and ABI representation.
- [x] Define target data layout and ABI rules for all supported value types.
  - [x] Carry pointer and stack-slot widths from target selection through MIR and LIR.
  - [x] Centralize recursive aggregate layouts and per-type ABI classification.
- [x] Implement 64-bit stack slots, loads/stores, and mutable locals across loops and branches.
- [x] Lower scalar struct literals, 64-bit field slots, defaults, reads, writes, parameters, and inferred arguments.
- [x] Implement nested fields and compound target layouts.
  - [x] Resolve reads and writes through nested struct fields.
  - [x] Store nested struct fields inline in local values and copy them by value.
  - [x] Store nested aggregates inline and define aggregate parameter and return ABI.
- [x] Implement tag-plus-one-slot enum construction and payload access.
- [x] Add target-aware enum layouts for payloads larger than one machine word and value returns.
- [x] Implement fixed-size scalar array literals, stack layout, indexing, and reads.
- [x] Store indexed elements in fixed-size scalar arrays.
- [x] Support array element layouts beyond the current 64-bit scalar slots.
  - [x] Pack `u8` arrays and support byte reads and writes across slot and slice boundaries.
  - [x] Pack scalar elements at their 1-, 2-, 4-, or 8-byte target widths.
  - [x] Lay out fixed arrays of structs inline, including indexing, field reads and writes, and value copies.
  - [x] Lay out nested arrays inline, including arrays of structs and array fields.
  - [x] Lay out tag-plus-one-slot payload enums inline in arrays and struct fields.
  - [x] Lay out enums whose payload exceeds one machine word inline.
  - [x] Store slice pointer/length descriptors inline in arrays, struct fields, and enum payloads.
- [x] Represent local slices with pointer/length slots; support indexing, slicing, and parameter passing.
- [x] Lower local slice reassignment with updated pointer, length, and element scale.
- [x] Return slices from Mimi functions as pointer in x0 and length in x1; reject C extern slice returns.
- [x] Implement local addresses, pointer loads/stores, and pointer type checks.
- [x] Lower pointer member access and typed pointer indexing in MIR.
- [x] Implement pointer-to-array layouts and remaining general pointer arithmetic.
  - [x] Scale pointer plus/minus integer by scalar element width; allow integer plus pointer.
  - [x] Compute element distances when subtracting pointers to the same scalar type.
  - [x] Scale and index pointers to fixed arrays of scalar and pointer elements.
  - [x] Scale and index pointers to pointers by pointer width.
  - [x] Lay out and index pointers to structs and enums.
  - [x] Scale pointer arithmetic and differences for struct and enum pointees.

## Functions and Modules

- [x] Implement named function values, function-value parameters, and indirect calls.
- [x] Lift capture-free lambda bodies into callable functions.
- [ ] Capture outer values in lambda closure environments.
- [x] Lower direct instance and static method calls with automatic or explicit receivers.
- [x] Support static methods as function values and indirect calls.
- [ ] Support bound instance methods as function values and indirect calls.
- [x] Allocate virtual registers with liveness and preserve values live across calls.
- [x] Pass ARM64 integer arguments beyond x0-x7 on the stack (up to 16 total).
- [x] Allocate and preserve ARM64 callee-saved registers.
- [x] Complete package symbol visibility and cross-package linking behavior.
  - [x] Compile imported functions with package-qualified symbols and resolve package-local calls.
  - [x] Re-export nested package symbols and types under qualified package paths.
  - [x] Report cyclic package imports instead of recursing indefinitely.
  - [x] Import and link exported types, constants, and external declarations with full visibility checks.
    - [x] Import public function-valued constant aliases as callable functions.
    - [x] Inline imported public integer, float, and boolean literal constants.
    - [x] Import public external declarations for direct calls, with package-qualified visibility and original link symbols.
    - [x] Load imported external function addresses through target-specific GOT relocations.
    - [x] Import exported types and enforce type visibility.
    - [x] Lower and link exported constants with full visibility checks.
      - [x] Inline imported public scalar expressions and same-package scalar constant references.
      - [x] Lower imported structs, fixed arrays, enums, and string constants.

## Targets and Tooling

- [x] Emit textual assembly for all current LIR blocks and supported operations.
- [x] Implement x86_64 code generation.
  - [x] Emit and assemble System V x86_64 text for the current LIR operations.
  - [x] Emit native x86_64 objects and executables through the target interface.
- [x] Define target interfaces and add another platform/object format.
- [ ] Add integration tests that compile, link, and run representative programs per feature.
  - [x] Compile and link an x86_64 float/control-flow program; run it on x86_64 hosts and verify its architecture on cross-target hosts.
  - [x] Run x86_64 fixed arrays, pointer writes, slices, loops, aggregate values, and payload enums on x86_64 hosts.
  - [x] Exercise mixed integer/float indirect-call arguments, stack-passed integer arguments, and f16 ABI values on x86_64.
  - [x] Link an x86_64 Mimi executable with the C print library and verify its output.
  - [x] Run do-while, loop defer/break/continue, function constant aliases, and static method values on x86_64.
