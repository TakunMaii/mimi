# Compiler TODO

Items are ordered by dependency: the IR and target foundations come before the
language features that depend on them. A checked item has an implementation
and focused regression coverage; it does not imply support on every target.

## IR and Control Flow

- [x] Correctly lower early `return` from nested blocks and conditional branches.
- [x] Lower unconditional `loop` bodies into CFG backedges; `break` remains pending.
- [x] Lower `while` and `do-while` into CFG blocks for basic loop bodies.
- [x] Lower `for` over fixed-size scalar arrays.
- [ ] Lower `for` over slices.
- [ ] Lower `break` and `continue` when the parser exposes them.
- [x] Lower `defer` at each scope exit and return path in LIFO order.
- [ ] Lower `match` and `if-let`, including enum payload bindings.

## Values and Memory

- [ ] Implement floating-point literals, arithmetic, comparisons, and ABI handling.
- [ ] Define target data layout and ABI rules for all supported value types.
- [x] Implement 64-bit stack slots, loads/stores, and mutable locals across loops and branches.
- [ ] Implement struct layout, literals, field access, and default initialization.
- [ ] Implement enum layout, construction, and payload access.
- [x] Implement fixed-size scalar array literals, stack layout, indexing, and reads.
- [ ] Implement array element writes and layouts beyond 64-bit scalar slots.
- [ ] Implement slice representation, indexing, and slicing.
- [ ] Implement pointer values, address-of, dereference, general pointer arithmetic, and pointer member access.

## Functions and Modules

- [ ] Implement function values, indirect calls, and lambda bodies.
- [ ] Implement instance and static methods in HIR/MIR, including automatic receivers.
- [x] Allocate virtual registers with liveness and preserve values live across calls.
- [ ] Support stack-passed arguments and callee-saved register allocation.
- [ ] Complete package symbol visibility and cross-package linking behavior.

## Targets and Tooling

- [ ] Make textual assembly emission cover all MIR blocks and supported operations.
- [ ] Implement x86_64 code generation.
- [ ] Define target interfaces and add another platform/object format.
- [ ] Add integration tests that compile, link, and run representative programs per feature.
