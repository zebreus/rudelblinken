# rudelblinken-bindgen

`rudelblinken-bindgen` is a custom, lightweight C binding wrapper and generator specifically built for the [Rudelblinken](https://github.com/zebreus/rudelblinken) project firmware and WebAssembly host/guest boundaries.

## Overview

It parses a limited subset of C headers (like `rudel.h`), primarily aiming to lower the declarations into a simplified Abstract Syntax Tree (AST), ignoring complex C compilation rules that aren't necessary for this task. From this AST, it outputs strongly-typed rust bindings for WebAssembly and modified C-guest headers that inject correct WebAssembly imports, making host-level SDK invocation seamless.

## Features

- **Pure Rust Parser**: Built using `chumsky` combinators – eliminating heavy C-compiler toolchain dependencies (e.g. `libclang`).
- **Targeted Subset**: Supports simple constants `#define`, C23 `static_assert`, `struct`, `enum`, primitive types, pointers, arrays, and standard functions.
- **Attributes Support**: Parses both GNU `__attribute__((...))` bindings (useful for `import_module` and `import_name`) and C23 attribute specifier strings (`[[...]]`).

## Missing Features / Unimplemented

- Floating points, complex variadic arguments, advanced preprocessor macros.
- Full resolution of static assertions (currently ignored/passed verbatim).

## source C format

The input C headers are a subset of what you would normally find in a C header file. While you can often express different things in C in multiple ways, we attempt to restrict the input so that for many things only one canonical representation is possible. When different ways of expressing similar concepts are allowed, they usually differ a bit in their semantics for rudelblinken-bindgen. They are in C23 and new language features are preferred over older ones.

### comments

Comments are allowed and associated with the next item that follows them. We only support line comments (Introduced by `//`).

### preprocessor directives

The only allowed pragma is `#pragma once`. It must be at the top of the file if present.

You can (and should) use static assertions with C23 `static_assert` (not C11 `_Static_assert`) to ensure that the C API is correctly defined. These are currently ignored by the parser, but they will be reemitted in the generated C header. In a future revision it might be mandatory to have assertions for all types you are using.

### defines

You are only allowed to use `#define` for defining simple constants. These must be of the form `#define NAME VALUE`, where `VALUE` is a simple literal (TODO: define this in proper C terms) (e.g. `42`, `0x2A`, `-1`) or a string literal (e.g. `"hello"`). You cannot use macros with parameters or complex expressions in the value.

### constexpr

Like defines, maybe different. We should probably prefer this and maybe not even allow defines at all. This is a C23 feature but we use that anyway.

### enums

Also kind of like defines.

### struct

Data containers. We don't support `typedef`ing structs, please just use the normal form (like `struct Name {int field;};`). The memory layout follows the ["Basic" C ABI](https://github.com/WebAssembly/tool-conventions/blob/main/BasicCABI.md).

### typedef

<!-- TODO: document typedef support (currently unsupported) -->

### function declarations

functions are either things that the guest either imports or exports.

If a function is not explicitly marked as an import or export, it is considered an import from the host by default.

syntax

```
/// Comments
int x();
```

#### function attributes

only a limited set of function attributes are supported. These are:

- `__attribute__((import_module("module_name")))` or `[[import_module("module_name")]]`: Specifies the WebAssembly module from which the function should be imported.
- `__attribute__((import_name("field_name")))` or `[[import_name("field_name")]]`: Overrides the imported symbol name (the default is the C function identifier).
- `__attribute__((export_name("field_name")))` or `[[export_name("field_name")]]`: Marks the function as a guest export and sets the exported symbol name.
- `__attribute__((import_module("module_name")))` or `[[import_module("module_name")]]`: Specifies the WebAssembly module for imports.
<!-- TODO: noreturn and friends -->

Notes:

- Attributes are only supported on function declarations.
- GNU-style (`__attribute__((...))`) and C23-style (`[[...]]`) forms are equivalent.
- Any attribute outside the list above is currently unsupported.
- If no import/export attribute is present, the function is treated as an import by default.

### function definitions

As we are only specifying interfaces, function definitions with bodies are not supported. Only declarations are allowed.

### globals

Globals are for now not supported.

<!-- TODO: section placeholder, remove or fill in -->
