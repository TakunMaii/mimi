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

Source files can set definitions for header import before importing the header:

```mimi
config :: @cDefine("PLATFORM", 1)
c :: @cimport("platform.h")
```

Scalar C functions, scalar and supported pointer/record typedef names (for example
`c.size_t`), ordinary pointers, integer enum constants, simple numeric macros,
and simple named struct and union fields are imported. Named records use Clang's
target size, alignment, and field offsets. Union literals initialize one member.
Complete struct and union values can be passed to and returned from C functions
through small Clang-compiled ABI thunks. Function pointer parameters are still
imported for scalar signatures. Top-level non-capturing Mimi functions can be
passed as callbacks; captured closures and callback signatures with aggregate
arguments or returns are not supported.
Records with unsupported field types remain opaque, so APIs that only pass
pointers to those records can still be imported and called.
Named C enum tags and typedefs are represented by their fixed integer type when
Clang reports one, or by the target's default `int` representation otherwise.
External scalar and pointer globals are available through the package namespace;
top-level `const` globals are read-only in Mimi.
Mutable globals of complete imported structs and unions support field reads,
field writes, and taking the address of the global aggregate.
Integer and floating-point scalar calls are compiled and linked against C on
both supported Apple targets.
String literals passed to imported `char *` parameters are emitted as
NUL-terminated static byte arrays. Slices and arrays expose `.ptr` and `.len`,
so APIs that take a pointer and length can be called as
`c.write(buffer.ptr, buffer.len)`.

## Linking Libraries And C Sources

```sh
mimi build main.mi -o main -L/path/to/lib -lgraphics --c-source support.c
```

Each `--c-source` file is compiled as C11 with optimization enabled and linked
into the output. Use `-I`, `-L`, and `-l` for header and library search paths.

## Current Limits

The importer currently skips declarations it cannot represent. It does not
provide a general C ABI for bitfields that cross their declared integer storage
unit, opaque incomplete record field access, captured callbacks, arbitrary
macros, `long double`, or
automatic conversion between Mimi slices and C strings. Variadic imports apply
the default promotions for integer and floating-point arguments on x86_64 and
AArch64.
Aggregate thunks currently cover complete struct and union parameters and
returns. Cross-target ABI runtime verification remains outstanding.
