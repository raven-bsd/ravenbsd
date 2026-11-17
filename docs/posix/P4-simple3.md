<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX localedef (P4h)

Status: **DONE for `localedef`.** The bounded parser accepts the documented locale/charmap
subset and emits the runtime catalog image read by `raven_libc_locale::nls::Catalog`. The
evidence overlay registers `util.localedef`; unsupported syntax is an explicit non-zero error and
never a partial image.

## Method

`localedef` is a first-party Rust crate under `userland/bin/localedef` (`src/lib.rs`,
`src/main.rs`, `src/lib_ut.rs`, `tests/host.rs`). The library owns the locale/charmap parser and
reuses the existing `gencat::write_catalog` image writer. The host binary reads the two sources
and writes the image; the target binary is a documented host-only stub, like `gencat`.

```sh
sh tests/posix/utility/run-simple3.sh
sh tests/posix/utility/run-simple3.sh --list
cargo nextest run -p localedef
```

## Options

| Option | Meaning |
|---|---|
| `-i input` | POSIX locale definition source (required) |
| `-f charmap` | POSIX charmap source (required) |
| `-o output` | catalog image path; standard output when absent |
| `-c` | create the output (accepted; the tool always creates) |
| `name` | optional locale name recorded in the `LOCALE` set |
| `--` | end of options |

Unknown options, a missing `-i`/`-f` value, or a second positional name are usage errors (exit 2).

## Source subset

The locale definition supports the six categories `LC_CTYPE`, `LC_NUMERIC`, `LC_TIME`,
`LC_COLLATE`, `LC_MONETARY` and `LC_MESSAGES`. Each category is a line with its name, zero or
more `keyword "value"` lines, and an `END <category>` line.

| Category | Keywords |
|---|---|
| `LC_CTYPE` | `codeset`, `mb_cur_max` |
| `LC_NUMERIC` | `decimal_point`, `thousands_sep`, `grouping` |
| `LC_TIME` | `abday`, `day`, `abmon`, `mon`, `d_t_fmt`, `d_fmt`, `t_fmt`, `am_pm` |
| `LC_COLLATE` | `order` |
| `LC_MONETARY` | `int_curr_symbol`, `currency_symbol`, `mon_decimal_point`, `mon_thousands_sep`, `mon_grouping`, `positive_sign`, `negative_sign`, `int_frac_digits`, `frac_digits`, `p_cs_precedes`, `p_sep_by_space`, `n_cs_precedes`, `n_sep_by_space`, `p_sign_posn`, `n_sign_posn` |
| `LC_MESSAGES` | `yesexpr`, `noexpr`, `yesstr`, `nostr` |

Every category also accepts `copy "C"` / `copy "C.UTF-8"` (recorded with message id 100). Values
are quoted strings with the `\\`, `\"`, `\n`, `\t` and `\r` escapes. A `;`-separated list of quoted
values is accepted and stored joined by `;` (for example `abday "Sun";"Mon"` becomes `Sun;Mon`).
Comments start with `#`.

The charmap supports `<symbol> /xHH U+HHHH [name...]` lines (`/x` or `\x` for the byte code),
comments starting with `#` or `%`, and requires at least one entry. The charmap name is recorded
from the `-f` argument.

## Output

The output is the runtime catalog image (magic `ff88ff89`) consumed by
`raven_libc_locale::nls::Catalog`:

| Set | Content |
|---|---|
| 1 | `LC_CTYPE` keywords |
| 2 | `LC_NUMERIC` keywords |
| 3 | `LC_TIME` keywords |
| 4 | `LC_COLLATE` keywords |
| 5 | `LC_MONETARY` keywords |
| 6 | `LC_MESSAGES` keywords |
| 7 | `name` (1), `charmap` (2), `charmap_entries` (3), `categories` (4) |

The `tests/host.rs` opens the emitted image through `Catalog::open`/`Catalog::gets` and checks the
numeric values, the charmap name and the entry count, so the "runtime readable" claim is measured.

## Exact unsupported cases

* Categories other than the six listed, and keywords outside the tables above.
* `copy` values other than a recorded name (the copied data is not expanded).
* Multi-line values, adjacent string concatenation without `;`, hexadecimal/octal escapes other
  than the five listed, symbolic set names, `LC_*` `END` mismatches.
* ChARmap ranges/multi-byte encodings, symbolic `<...>` lines without `/x` and `U+`, and empty
  charmaps.
* The target binary is host-only (like `gencat`); the target image has no locale database
  consumer and no general stdin.

## Evidence overlay

`docs/posix/evidence/P4-simple3.toml` contains the `util.localedef` `test:` row. The row runs
`tests/posix/utility/simple3/localedef/test.sh`, which runs the nextest suite, emits a real image
and checks the magic, and asserts that an unsupported source writes nothing.

## Verification record

* `cargo nextest run -p localedef`: **10 passed**.
* `sh tests/posix/utility/run-simple3.sh`: **1 pass, 0 fail** (`localedef: 3 case(s)`).
* `cargo xtask posix-check --profile posix --arch x86_64 --host-only`: exit 1 for the global
  profile (**1489 pass, 217 fail, 105 skip**); the `util.localedef` row is **pass**. The global
  red is the other required utilities, not this tool.
* `make check-fast`: **PASSED**.
