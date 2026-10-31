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

The current milestone is a Mimi implementation of a lexical scanner. It is
compiled and executed by the Rust compiler. See [TODO.md](TODO.md) for the
parity roadmap and [FEATURES.md](FEATURES.md) for the feature matrix.
