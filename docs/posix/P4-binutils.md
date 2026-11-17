<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4v - POSIX `nm` and `strip`

First-party Rust implementations of `nm` and `strip` for ELF32/ELF64 little-endian objects,
backed by the bounded section/symbol inspector in `raven-elf`, plus host behaviour tests,
executable evidence rows and man pages.

## Method

* `crates/raven-elf/src/inspect.rs` adds a separate read-only inspection API next to the strict
  loader parser. `Object::parse` accepts ELF32/ELF64 and `ET_REL`/`ET_EXEC`/`ET_DYN`, checks every
  section-header and string range against the input and allocates nothing. `Elf::parse` and
  `plan_load` are unchanged.
* `nm` reads the `.symtab` sections, maps `st_info` binding/type and `st_shndx` to POSIX type
  letters, and supports `-a`, `-g`, `-u`, `-n`, `-p` and `-r`. Local symbols use lowercase
  letters; weak defined symbols are `W`/`V`, weak undefined symbols are `w`/`v`, absolute is
  `A`/`a`, common is `C`/`c`, undefined is `U`, bss is `B`/`b`, data is `D`/`d`, text is
  `T`/`t`, read-only is `R`/`r` and other sections are `N`/`n`.
* `strip` never moves section contents or program headers. It clears the section header of
  `.symtab`, its dedicated `.strtab`, and `.debug*`/`.zdebug*`/`.stab*`/`.line` sections in place,
  then reparses the result and verifies the neutralized headers. The section-header table stays at
  the same offset, so `e_shoff`, `e_shnum`, `e_shstrndx`, the entry point and every program header
  remain valid. `-g` removes debug sections only; `-s` (and the default) removes symbols as well;
  `-o outfile` copies the input, strips the copy and leaves the original untouched.
* The target binaries use `raven_rt::fs`; host tests build the same library and exercise the
  command-line wrappers against real host ELF executables.

## Tool matrix

| Tool | Coverage | Tests |
|---|---|---:|
| `nm` | ELF32/ELF64 `ET_REL`/`ET_EXEC`/`ET_DYN`; `.symtab`; `-a` `-g` `-u` `-n` `-p` `-r`; local/global/weak/absolute/common/undefined type letters | 12 lib + 5 host |
| `strip` | ELF32/ELF64; `.symtab` and linked `.strtab`; `.debug*`/`.zdebug*`/`.stab*`/`.line`; `-g` `-s` `-o`; reparse verification | 7 lib + 5 host |

The host integration tests include the ticket's proof sequence: `nm` lists symbols from the host
`nm` binary, `strip` removes them, the stripped binary is executed successfully, and `nm` then
reports no symbols. The tests also compare the entry point and the complete program-header bytes
before and after stripping.

## Known limits

| Limit | Detail |
|---|---|
| Dynamic symbols | `nm` lists `.symtab` only; `.dynsym` is not shown and is preserved by `strip` |
| Archives | `nm`/`strip` take ELF files; archive members belong to the `ar`/`pax` tickets |
| Physical removal | `strip` neutralizes section headers instead of compacting the file; bytes belonging to removed sections remain unreferenced |
| Big-endian ELF | rejected by the bounded inspector |
| Multiple `-o` inputs | `strip -o` accepts exactly one input file |

## Acceptance

```
sh tests/posix/utility/run-binutils.sh
cargo nextest run -p raven-elf -p nm -p strip
make check-fast
```

The runner builds the host `nm` and `strip` once and reports `2 pass, 0 fail`. The two `util.*`
rows are registered in `docs/posix/evidence/P4-binutils.toml`; the gate executes each `test:`
script directly. No board claim.
