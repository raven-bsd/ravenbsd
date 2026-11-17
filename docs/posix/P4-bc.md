<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX bc calculator (P4s)

Status: **DONE for the bounded POSIX core.** `raven-bc` implements arbitrary-precision decimal
arithmetic, `scale`, `sqrt`, `length`, variables, arrays, comparisons, `if`/`while`/`for`, user
functions, `print`, `-l`, file operands and stdin. `util.bc` is registered as present only through
the host behaviour script.

## Method

`userland/bin/bc` is a first-party Rust crate (`raven-bc`), `no_std` on the target and
`#![forbid(unsafe_code)]`. `src/number.rs` is a clean-room base-10^9 decimal implementation with
explicit scale; `src/lib.rs` contains the lexer, parser and interpreter; `src/math.rs` is the
bounded `-l` library.

`tests/posix/utility/bc/test.sh` is self-contained, builds the host binary and runs 23 cases. It
checks exact digit output against recorded values, including `2^100`, 50-digit division and square
root, scale rules, recursion, arrays, printing, comments, `-l` math values, file/stdin input and
error statuses. `src/lib_ut.rs`, `src/number_ut.rs` and `src/math_ut.rs` add the Rust unit cases.

```sh
sh tests/posix/utility/run-bc.sh
sh tests/posix/utility/bc/test.sh
```

## Precision/control matrix

| Area | Implemented | Cases |
|---|---|---|
| Arbitrary precision | base-10^9 signed decimal, explicit `scale`, `+ - * / % ^`, `sqrt` | 3 |
| Scale and length | `scale` variable and `scale(expr)`, zero-offset truncation, `length(expr)` | 1 |
| Variables/arrays | scalar variables, associative integer-indexed arrays, default zero | 1 |
| Control flow | `if`/`else`, `while`, C-style `for`, `break`, `continue`, `quit` | 2 |
| Functions | parameters, `return`, `auto` locals, recursion, scalar return values | 1 |
| Output | expression values, `print` lists, strings, comments | 2 |
| `-l` | sets scale to 20; bounded `e`, `l`, `s`, `c`, `a` | 2 |
| Input | stdin, multiple file operands, `-`, `quit` stop | 4 |
| Errors | division by zero, negative `sqrt`, undefined function, unsupported base, parse/I/O/usage | 7 |

The arithmetic follows bc's scale rules: addition/subtraction use the larger scale; multiplication
uses the minimum of the sum of scales and the maximum of the global or operand scales; division
uses the global scale; `sqrt` uses the maximum of the global or operand scale. Division and other
truncating operations discard digits rather than round.

## Known limits and defects

* `dc` is a separate language and is not implemented.
* GNU extensions are not implemented: `read()`, `last`, `history`, `limits`, `warranty`, `print`
  extensions, and the `-i`/`-q`/`-s`/`-w` options.
* `ibase` and `obase` are fixed at 10. Assigning any other base is an explicit unsupported error.
* Array parameters and `auto` arrays are unsupported; scalar parameters are passed by value and
  arrays are global.
* The Bessel function `j` is unsupported. The bounded `-l` series are limited to the documented
  convergence/iteration bounds; large transcendental arguments may be rejected or truncated.
* Integer exponents are bounded to 10,000; `scale` is bounded to 1,000,000; array indices are
  bounded to 0..1,000,000; recursion is bounded to 64 frames.
* The target binary reads `tty:console` when no file is supplied; the image has no general
  process-stdin handle. The target binary is not staged in the image, so the host suite is the
  executable evidence.
* `#` line comments are accepted as a common extension; the POSIX `/* ... */` form is the primary
  syntax.

## Evidence overlay

`docs/posix/evidence/P4-bc.toml` contains the `util.bc` row with
`test:tests/posix/utility/bc/test.sh`. The P0b loader accepts the `P4-bc` owner batch file and the
gate executes the script; the row is present only while every case passes.
