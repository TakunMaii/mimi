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
  - [ ] Support f8/f16 lowering and ABI representation.
- [ ] Define target data layout and ABI rules for all supported value types.
- [x] Implement 64-bit stack slots, loads/stores, and mutable locals across loops and branches.
- [x] Lower scalar struct literals, 64-bit field slots, defaults, reads, writes, parameters, and inferred arguments.
- [ ] Implement nested fields, compound layouts, and value return ABI.
  - [x] Resolve reads and writes through nested struct fields.
  - [ ] Store nested aggregates inline and define aggregate parameter and return ABI.
- [x] Implement tag-plus-one-slot enum construction and payload access.
- [ ] Add target-aware enum layouts for payloads larger than one machine word and value returns.
- [x] Implement fixed-size scalar array literals, stack layout, indexing, and reads.
- [x] Store indexed elements in fixed-size scalar arrays.
- [ ] Support array element layouts beyond the current 64-bit scalar slots.
  - [x] Pack `u8` arrays and support byte reads and writes across slot and slice boundaries.
  - [x] Pack scalar elements at their 1-, 2-, 4-, or 8-byte target widths.
  - [ ] Lay out array, struct, and enum elements inline.
- [x] Represent local slices with pointer/length slots; support indexing, slicing, and parameter passing.
- [x] Lower local slice reassignment with updated pointer, length, and element scale.
- [x] Return slices from Mimi functions as pointer in x0 and length in x1; reject C extern slice returns.
- [x] Implement local addresses, pointer loads/stores, and pointer type checks.
- [x] Lower pointer member access and typed pointer indexing in MIR.
- [ ] Implement pointer-to-array layouts and remaining general pointer arithmetic.
  - [x] Scale pointer plus/minus integer by scalar element width; allow integer plus pointer.
  - [x] Compute element distances when subtracting pointers to the same scalar type.
  - [x] Scale and index pointers to fixed arrays of scalar and pointer elements.
  - [ ] Lay out and index pointers to structs and enums.
  - [ ] Define compound-pointer arithmetic.

## Functions and Modules

- [x] Implement named function values, function-value parameters, and indirect calls.
- [ ] Lower lambda bodies and captures.
- [x] Lower direct instance and static method calls with automatic or explicit receivers.
- [x] Support static methods as function values and indirect calls.
- [ ] Support bound instance methods as function values and indirect calls.
- [x] Allocate virtual registers with liveness and preserve values live across calls.
- [x] Pass ARM64 integer arguments beyond x0-x7 on the stack (up to 16 total).
- [x] Allocate and preserve ARM64 callee-saved registers.
- [ ] Complete package symbol visibility and cross-package linking behavior.

## Targets and Tooling

- [x] Emit textual assembly for all current LIR blocks and supported operations.
- [ ] Implement x86_64 code generation.
- [ ] Define target interfaces and add another platform/object format.
- [ ] Add integration tests that compile, link, and run representative programs per feature.
