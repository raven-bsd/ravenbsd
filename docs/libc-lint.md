<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc lint: one-symbol files and the frozen map

`cargo xtask libc-lint` is the executable form of the ravenlibc layout and ABI rules. It scans
every ravenlibc crate, the libc host tools and the `xtask/src/libc*` helpers. The current tree is
green over 3902 files.

## Checks relevant to C64n

| Check | Rule |
|---|---|
| `size` / `one-symbol` | The first `#[no_mangle]` export names the file, and a file has at most four aliases of one function. |
| `abi-freeze` / `frozen-missing` | Every public export in the tree is present in `abi/frozen/RAVEN_1.0.symbols`. |
| `abi-freeze` / `frozen-txt` | The public name set in `abi/frozen/RAVEN_1.0.txt` equals the set in `.symbols`. |

The frozen `RAVEN_1.0` node is the release contract. A frozen symbol is never deleted to silence
the lint; the generated files are brought up to the tree instead.

## One symbol per file

The file stem must match its first export. Module names may stay stable with `#[path]`, as in the
C64c layout cleanup:

```rust
#[path = "fgetwc.rs"]
pub(crate) mod wide;
```

When one file exports several distinct symbols, split it. The pty placeholders are now
`compat/{posix_openpt,grantpt,unlockpt,ptsname_r}.rs`; the stdio line and misc helpers are
`stdio/file/{fgetln,getline,remove,rewind}.rs`. A path inside a non-`mod.rs` module file is
relative to that file's parent directory, so `stdio/file.rs` uses `#[path = "file/fgetln.rs"]`.

## Regenerating the map

`libc-abi gen` derives `Versions.def`, `Symbols.list`, `Symbols.private` and
`frozen/RAVEN_1.0.symbols` from the shell `Symbol.map`, the donor census and the data-symbol
table:

```
cargo xtask libc-abi gen --root .
```

Public exports that the donor census does not list (the Rust-specific
`__getrandom_v03_custom`, and the BSD `getaudit`) still belong to `Symbols.list`. The generator
records them with default function metadata (`F - <arches> 3 -`) so the frozen node is not
narrower than the linked DSO.

The `RAVEN_1.0.txt` candidate is a `raven-libc-mkmap freeze` recording of the linked DSO, not a
map input. To refresh it after a source change, build the DSO and freeze the node:

```
cargo xtask libc-pack --arch x86_64
cargo xtask libc-link --arch x86_64
rm libc/ravenlibc/abi/frozen/RAVEN_1.0.txt
cargo run -q -p raven-libc-mkmap -- freeze \
    --so build/x86_64/libc.so.1 --node RAVEN_1.0 \
    --out libc/ravenlibc/abi/frozen --arch x86_64
```

`freeze` refuses to overwrite a different existing file, so the candidate is removed first; it is
always regenerated, never hand-edited. The resulting `.txt` and `.symbols` name sets must match.

## Gate

```
cargo nextest run -p xtask
cargo xtask libc-lint
make check-fast
```

`cargo nextest run -p xtask` includes `libc_lint::tests::real_tree_lint_is_clean`, so a stale
frozen map or a misnamed shell file fails the normal workspace suite as well.
