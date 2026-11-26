<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Dynamic loader cold start (K139)

Status: **PARTIAL — the spawn→main loader cost is fixed; the password→shell step is a separate
K135/getty+login path.** The fix maps libc from the initfs zero-copy instead of copying it through
the file server. `login-latency` step 3 (`root` → `Password:`) now passes its 1 s cap 5/5; step 5
(`Password:` → shell prompt) still fails 5/5 and is handed to K135.

## Symptom (K135 markers)

The static login image reaches `login main` 76 ms after getty returns from `spawn`. The release
dynamic image took 1.48 s (first login) and 1.44 s (second login) between `getty spawn returned
/bin/login` and `K135 login main`. Relocation was not the cost: a loader phase trace with temporary
markers showed:

| Phase | Time |
|---|---:|
| self-relocation | ~10 ms |
| register main | ~12 ms |
| `load_needed_objects` (libc) | **1 574–2 222 ms** |
| TLS offsets | <1 ms |
| `relocate_all` | 12–17 ms |
| enter main | 3–6 ms |

Inside `load_needed_objects`, the loader opened `/lib/libc.so.1` through `file:` and copied each
segment with `PREAD`:

| Operation | Time |
|---|---:|
| open `file:/lib/libc.so.1` | 58 ms |
| 8 ELF-header/program-header reads (56–64 B) | 12–17 ms each |
| map R segment (0x2724c bytes) | 261 ms |
| map RX segment (0x109054 bytes) | **1 517 ms** |
| map RW segments | 63 ms / 15 ms |

The file server copies about 1.3 MiB at roughly 0.8 MiB/s on TCG. The per-header reads are secondary;
the large segment copies dominate. This is why the first dynamic child after boot pays it and why a
later `raven-date` (small, and after the boot surge) looks faster; it is not a relocation or TLS
layout cost.

## Fix

1. The spawner (`crates/raven-spawn`) looks up `/lib/libc.so.1` in the initfs and passes its
   **archive offset** in `AT_RAVEN_LIBC` (`0x7007`). It does not parse libc with `raven-elf`: a
   shared object has `e_entry = 0`, which the executable-only parser correctly rejects.
2. The loader (`libc/ravenlibc/ld/src/raw/libc.rs`) maps the header page from `Handle::INITFS` at a
   scratch address, parses `Ehdr`/`Phdr` with `raven-libc-elf`, plans the segments, maps every
   file-backed segment directly from the initfs at `LIBC_BASE` (`0x3000_0000_0000`), zeroes the
   file-tail page, and registers the object as `libc.so.1` before the `DT_NEEDED` closure.
3. `load_needed_objects` finds `libc.so.1` by name and never opens `file:`. The old copy path
   remains as the fallback for other libraries and for image-only spawns.

The header page must be parsed at a scratch address, not at `LIBC_BASE`: the first `File` step maps
at `LIBC_BASE`, and a fixed mapping over the header page fails.

No kernel, loader-base (`--image-base=0x1000`), getty/login/consd or uutils change is part of this
fix. A failed pre-map prints `ld-elf.so.1: libc pre-map failed; using the file server` and falls
back instead of failing the process.

## Result

`login-latency` (Phil's bounded case, copied from `origin/phil/K135`, 1 s step caps):

| Run | Host load1 before | step 3 `Password:` | step 5 shell prompt |
|---|---:|---|---|
| 1 | 15.65 | pass | fail |
| 2 | 6.81 | pass | fail |
| 3 | 12.25 | pass | fail |
| 4 | 21.08 | pass | fail |
| 5 | 8.66 | pass | fail |

The dynamic spawn→main path is under the 1 s cap; the password→shell path is not. The remaining
gap is the same K135 marker sequence: `Password:` at 76.122 → `getty spawn returned /bin/brush` at
77.084 (0.962 s) → prompt 77.648 (1.526 s). Brush is static; the cost is login verification/exit
plus getty's wait and the static brush startup, outside the K139 loader boundary. **Next owner:
K135 (Phil) for login/getty and the static shell startup.** The target 300 ms for both steps is not
met.

Validation on the same tree:

```
release-login: pass
login-retry: pass
tty-control: pass
tty-job-control: pass
make check-fast: PASSED
provenance: 0 violations
```

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | RavenBSD decision |
|---|---|---|---|---|
| Dynamic library mapping | The runtime linker maps shared objects from the file object; behaviour only, no code copied. | FreeBSD rtld maps the object and applies relocations; this loader is a transpiled FreeBSD rtld line, no new donor code. | Linux maps the shared object and demand-pages it; idea only. | Keep the first-party loader and map libc from the initfs zero-copy. |
| Program-header discovery | Runtime linker reads the mapped headers. | rtld reads the mapped headers; behaviour only. | AT_PHDR/PT_TLS contract. | The spawner passes the initfs offset; the loader parses with `raven-libc-elf`. |
