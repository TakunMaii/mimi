# C Interoperability

Mimi can import supported C declarations through Clang and link C libraries or
source files into a native executable.

## Header Imports

```mimi
c :: @cimport("stdlib.h")

main :: fn () i32 {
    c.abs(-42) - 42
}
```

`@cimport` invokes the Clang executable selected by `MIMI_CLANG` (default:
`clang`). The selected Mimi target is passed to Clang. Header lookup paths,
preprocessor definitions, and a sysroot can be configured on the build command:

```sh
mimi build main.mi -o main -I/path/to/include -DPLATFORM=1 --sysroot /sdk
```

Scalar C functions, scalar typedefs used in prototypes, pointers, function
pointers, integer enum constants, simple numeric macros, and simple named
struct fields are imported. Imported function calls use their C linker names.

## Linking Libraries And C Sources

```sh
mimi build main.mi -o main -L/path/to/lib -lgraphics --c-source support.c
```

Each `--c-source` file is compiled as C11 with optimization enabled and linked
into the output. Use `-I`, `-L`, and `-l` for header and library search paths.

## Current Limits

The importer currently skips declarations it cannot represent. It does not
provide a general C ABI for structs and unions, exact Clang record layouts,
opaque incomplete record use, C globals, variadic functions, callbacks,
arbitrary macros, or automatic conversion between Mimi slices and C strings.
Simple imported struct field access uses Mimi's current aggregate layout and
must not be used to pass such structs by value to C functions.
