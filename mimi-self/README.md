# Mimi Self-Hosting

This directory is the workspace for implementing the Mimi compiler in Mimi.
The Rust compiler at the repository root is currently the stage0 bootstrap
compiler.

Run the Python tests from the repository root:

```sh
python3 mimi-self/test.py
```

The same suite accepts an alternate compiler executable with
`python3 mimi-self/test.py --compiler /path/to/mimi`.
When `--compiler` is set, the suite also compares accepted/rejected smoke
cases and their first diagnostic against the Rust stage0 compiler. Pass
`--reference-compiler /path/to/stage0` to select a different reference binary.

The current milestone is the self-hosted lexer and an arena-backed expression
parser for grouped unary/binary expressions, member/index access, and nested
calls. The parser handles operator precedence and left associativity. Stage0
is the Rust compiler. See
[TODO.md](TODO.md) for the parity roadmap and [FEATURES.md](FEATURES.md) for
the feature matrix.

`lexer.error_code` returns 1 for an unexpected character, 2 for an
unterminated block comment, 3 for an unterminated string, 4 for an invalid
escape, and 5 for malformed UTF-8 in an identifier. `lexer.error_span` returns
the corresponding half-open byte range in the source. `lexer.lex_next` combines
the token kind, span, and error code in one `LexResult`.

`source.render` writes a diagnostic with its path, one-based line and column,
source line, and byte-span underline to standard error. `ModuleGraph` tracks
loading and loaded paths for cycle detection; automatic import discovery is
still pending.
