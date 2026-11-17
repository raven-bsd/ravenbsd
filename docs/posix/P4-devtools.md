<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P41 - POSIX cflow, ctags and cxref

## Tool matrix

| Tool | Supported options | Output | Evidence |
|---|---|---|---|
| `ctags` | `-f file`, `-x` | tags file or xref listing | `test.sh`; `lib_ut` |
| `cflow` | `-o file`, `-r` | call graph text; reverse order with `-r` | `test.sh`; `lib_ut` |
| `cxref` | `-o file`, `-x` | cross-reference table | `test.sh`; `lib_ut` |

All three read one or more C source files and share the tokenizer/analyzer in
`userland/bin/ctags/cparse` (package `raven-cparse`). There is no external parser crate and no
preprocessor evaluation.

## Output formats

`ctags` without `-x` writes `<name>\t<file>\t<line>` for functions, top-level globals, macros,
`struct` tags and `enum` tags, sorted by name. With `-x` it writes
`<name>\t<line>\t<file>\t<source-line>` to standard output.

`cflow` prints `name: line` for a root function and indents direct callees by four spaces.
Recursion is expanded with a `(cycle)` marker when a function is already on the current path.
Only calls to functions defined in the input are shown. `-r` inverts the edges and prints the
reverse call graph. Functions called by another function are printed nested, not again as a
top-level root.

`cxref` prints `<name>\t<kind>\t<file>:<line>\t<refs>` where `refs` is a comma-separated list of
`file:line` occurrences, or `-` when there are none. The definition line is not repeated in
`refs`; references are lexical identifier matches, so a same-named local can appear.

## Analysis matrix

| Construct | Recognized |
|---|---|
| `#define NAME` | macro, including function-like names (the name before `(`) |
| top-level object | global, including comma-separated declarators |
| function definition | function, with direct calls to known functions |
| `struct tag { ... }` | struct tag |
| `enum tag { ... }` | enum tag |
| comments, strings, char literals | skipped |

## Exact unsupported cases

- Standard input; every tool needs at least one file operand.
- Preprocessor conditionals, includes and macro expansion.
- C++ and Objective-C.
- `typedef` aliases, struct/union members and enum constants as separate tags.
- Function-pointer declarators and calls through function pointers.
- Calls to functions declared but not defined in the input, and calls through macros.
- Cross-file duplicate names: cflow merges direct calls by name; ctags and cxref emit one row per
  definition.
- `ctags` options `-t`, `-u`, `-v`, `-w`; `cflow` options `-d`, `-D`, `-i`, `-I`, `-U`, `-f`;
  `cxref` options `-c`, `-s`, `-t`, `-u`; multiple `-f`/`-o` operands.
- `cflow -o -` and `ctags -f -` write to standard output; other output-file semantics are not
  implemented.

## Acceptance

```
sh tests/posix/utility/run-devtools.sh
cargo nextest run -p cflow -p ctags -p cxref
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

Evidence: `docs/posix/evidence/P4-devtools.toml`.
