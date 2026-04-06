# rudelblinken-bindgen

`rudelblinken-bindgen` is a custom, lightweight C binding wrapper and generator specifically built for the [Rudelblinken](https://github.com/zebreus/rudelblinken) project firmware and WebAssembly host/guest boundaries.

## Overview

It parses a limited subset of C headers (like `rudel.h`), primarily aiming to lower the declarations into a simplified Abstract Syntax Tree (AST), ignoring complex C compilation rules that aren't necessary for this task. From this AST, it outputs strongly-typed rust bindings for WebAssembly and modified C-guest headers that inject correct WebAssembly imports, making host-level SDK invocation seamless.

## Features

- **Pure Rust Parser**: Built using `chumsky` combinators – eliminating heavy C-compiler toolchain dependencies (e.g. `libclang`).
- **Targeted Subset**: Supports simple constants `#define`, `_Static_assert`, `struct`, `enum`, primitive types, pointers, arrays, and standard functions.
- **Attributes Support**: Parses both GNU `__attribute__((...))` bindings (useful for `import_module` and `import_name`) and C23 attribute specifier strings (`[[...]]`).

## Missing Features / Unimplemented

- Floating points, complex variadic arguments, advanced preprocessor macros.
- Full resolution of static assertions (currently ignored/passed verbatim).

## source C format

The input C headers are a subset of what you would normally find in a C header file. While you can often express different things in C in multiple ways, we attempt to restrict the input so that for many things only one canonical representation is possible. When different ways of expressing similar concepts are allowed, they usually differ a bit in their semantics for rudelblinken-bindgen.

### preprocessor directives

The only allowed pragma is `#pragma once`. It must be at the top of the file if present.
