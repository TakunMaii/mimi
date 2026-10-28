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
  - [ ] Define an f8 encoding and support its lowering and ABI representation.
- [ ] Define target data layout and ABI rules for all supported value types.
- [x] Implement 64-bit stack slots, loads/stores, and mutable locals across loops and branches.
- [x] Lower scalar struct literals, 64-bit field slots, defaults, reads, writes, parameters, and inferred arguments.
- [ ] Implement nested fields, compound layouts, and value return ABI.
  - [x] Resolve reads and writes through nested struct fields.
  - [x] Store nested struct fields inline in local values and copy them by value.
  - [ ] Store nested aggregates inline and define aggregate parameter and return ABI.
- [x] Implement tag-plus-one-slot enum construction and payload access.
- [ ] Add target-aware enum layouts for payloads larger than one machine word and value returns.
- [x] Implement fixed-size scalar array literals, stack layout, indexing, and reads.
- [x] Store indexed elements in fixed-size scalar arrays.
- [ ] Support array element layouts beyond the current 64-bit scalar slots.
  - [x] Pack `u8` arrays and support byte reads and writes across slot and slice boundaries.
  - [x] Pack scalar elements at their 1-, 2-, 4-, or 8-byte target widths.
  - [x] Lay out fixed arrays of structs inline, including indexing, field reads and writes, and value copies.
  - [x] Lay out nested arrays inline, including arrays of structs and array fields.
  - [x] Lay out tag-plus-one-slot payload enums inline in arrays and struct fields.
  - [ ] Lay out enums whose payload exceeds one machine word inline.
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
- [ ] Complete package symbol visibility and cross-package linking behavior.
  - [x] Compile imported functions with package-qualified symbols and resolve package-local calls.
  - [ ] Import and link exported types, constants, and external declarations with full visibility checks.

## Targets and Tooling

- [x] Emit textual assembly for all current LIR blocks and supported operations.
- [ ] Implement x86_64 code generation.
- [ ] Define target interfaces and add another platform/object format.
- [ ] Add integration tests that compile, link, and run representative programs per feature.
