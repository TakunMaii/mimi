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
Imported function calls use their C linker names. Prototypes
using by-value records or function pointer parameters are skipped because the
current backends do not implement those C calling conventions.
Named C enum tags and typedefs are represented by their fixed integer type when
Clang reports one, or by the target's default `int` representation otherwise.
External scalar and pointer globals are available through the package namespace;
top-level `const` globals are read-only in Mimi.
String literals passed to imported `char *` parameters are emitted as
NUL-terminated static byte arrays. Mutable buffers and arbitrary Mimi slices
still require explicit conversion support.

## Linking Libraries And C Sources

```sh
mimi build main.mi -o main -L/path/to/lib -lgraphics --c-source support.c
```

Each `--c-source` file is compiled as C11 with optimization enabled and linked
into the output. Use `-I`, `-L`, and `-l` for header and library search paths.

## Current Limits

The importer currently skips declarations it cannot represent. It does not
provide a general C ABI for structs and unions passed by value, multi-byte
bitfields, opaque incomplete record field access, thread-local or aggregate C
globals, variadic functions, callbacks,
arbitrary macros, `long double`, or automatic conversion between Mimi slices
and C strings.
Simple imported struct field access uses Mimi's current aggregate layout and
uses the Clang layout metadata; such structs must not be passed by value to C
functions until target ABI classification is implemented.
