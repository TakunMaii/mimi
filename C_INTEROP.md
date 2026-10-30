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
through small Clang-compiled ABI thunks. Function pointer parameters support
scalar signatures and complete struct or union arguments and returns. The
compiler generates a C adapter for aggregate callback signatures so C's native
by-value convention is translated to Mimi's internal aggregate convention.
Top-level non-capturing Mimi functions can be passed as callbacks. Captured
closures can be passed when the C callback's final parameter is `void *` and
the C API takes an adjacent `void *` context parameter after the callback;
Mimi passes the closure environment as that context automatically. Other
capturing callback signatures are rejected. Parameter and return qualifiers
are retained in imported metadata. Declarations using non-default calling
conventions are skipped because the native backends currently implement the
platform C convention.
Records with unsupported field types remain opaque, so APIs that only pass
pointers to those records can still be imported and called.
Named C enum tags are retained as imported enum types, with Clang's underlying
integer type recorded separately for record layout. C function parameters,
returns, and globals use the corresponding integer ABI representation.
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
unit, opaque incomplete record field access, callbacks without a compatible
context parameter, arbitrary
macros, `long double`, or automatic conversion between Mimi slices and C
strings. C strings can be passed from literals or explicit byte pointers.
Variadic imports apply
the default promotions for integer and floating-point arguments on x86_64 and
AArch64.
Aggregate thunks currently cover complete struct and union parameters and
returns. Cross-target ABI runtime verification remains outstanding.
