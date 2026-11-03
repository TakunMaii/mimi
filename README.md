# Mimi

Mimi is a small statically typed systems language and its compiler. The Rust
crate at the repository root is the bootstrap compiler (**stage0**). The
[`mimi-self/`](mimi-self/) directory contains a compiler written in Mimi that
generates C and drives the host `cc`; it builds itself and passes the
behaviour suite in three consecutive stages.

## Layout

| Path | Contents |
| --- | --- |
| [`src/`](src/) | Stage0 compiler: lexer, parser, type checker, HIR, MIR, LIR, register allocation, AArch64/x86_64 code generation, Clang-based C import |
| [`tests/`](tests/) | Rust unit tests for the parser, type checker, and IR/codegen layers |
| [`mimi-self/src/`](mimi-self/src/) | Self-hosted compiler: lexer, parser, semantic pass, typed IR lowering, C emitter, module graph, CLI |
| [`mimi-self/runtime.c`](mimi-self/runtime.c) | C runtime backing the self-hosted compiler (arena, buffers, map, I/O, processes, Unicode tables) |
| [`mimi-self/tests/`](mimi-self/tests/) | Mimi programs compiled and run by the behaviour suite |
| [`runtime/`](runtime/) | Optional C helpers (f8 soft float, `print`) |
| [`LANGUAGE.md`](LANGUAGE.md) | Accepted language syntax |
| [`C_INTEROP.md`](C_INTEROP.md) | `@cimport`, ABI thunks, callbacks, linking |
| [`mimi-self/TODO.md`](mimi-self/TODO.md) | Parity roadmap and current milestone |
| [`mimi-self/PARITY.md`](mimi-self/PARITY.md) | Verified subset and remaining differences |
| [`mimi-self/FEATURES.md`](mimi-self/FEATURES.md) | Feature matrix |

## Build and test

```sh
cargo build                      # stage0 compiler: target/debug/mimi
cargo test                       # Rust unit tests
python3 mimi-self/test.py        # behaviour suite with stage0
python3 mimi-self/bootstrap.py mimi-self/src/compiler.mi --output-dir /tmp/mimi-bootstrap
```

`bootstrap.py` builds stage1 with stage0, stage2 with stage1, and stage3 with
stage2, runs the behaviour suite under every stage, and reports artifact sizes
and SHA-256 digests.

## Status

The stage0 compiler implements the documented language, including structs with
methods, enums with payloads, generics on structs and enums, pointers, arrays,
slices, tuples, closures/callbacks, `defer`, and C interop through Clang. Native
code is produced for macOS AArch64 by a built-in Mach-O encoder and for Linux
AArch64/x86_64 through the system assembler and linker.

The self-hosted compiler covers the compiler-shaped subset of the language,
bootstraps itself, and reproduces byte-identical artifacts. It still collapses
integer widths and pointer target types to `i64`/`void *`, lacks `@cimport`,
generic structs, and module-level globals; see
[`mimi-self/PARITY.md`](mimi-self/PARITY.md) for the precise list.

## Build cache

Successful builds are cached under `.mimi/` in the working directory (ignored by
git). C import results are cached under `.mimi/cimport/`. The directory is safe
to delete at any time:

```sh
rm -rf .mimi
```

Pass `--no-cache` to `mimi build` to bypass both caches.
