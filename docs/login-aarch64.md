<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 release login (K153/K410)

Status: **aarch64 login 5/5 PASS (K173, 2026-11-19); styled-banner expectation fixed (K410,
2026-11-21).** The K166 PL011/DEBUG_READ path now reaches `Password:` and `raven#`; K169 fixed the
brush link layout. The K153/K139 history below is retained as history. No Pi 4 board result is
claimed.

## K410 styled-banner update (2026-11-21)

`tests/qtest/cases/login-aarch64.toml` still expected the pre-K374 single-line banner
`RavenBSD 0.1 (aarch64) (tty:console)` and timed out even though the guest booted, logged in and
exited cleanly. The case now asserts the K374 styled screen and keeps the authentication checks:

```diff
-[[expect]]
-regex = '^RavenBSD 0\.1 \(aarch64\) \(tty:console\)$'
+[[expect]]
+regex = '^R A V E N   B S D$'
+
+[[expect]]
+regex = '^-{10,}$'
+
+[[expect]]
+regex = '^Welcome to RavenBSD 0\.1 aarch64$'
```

The login/password/`raven#`/`init exited (0)` expectations are unchanged. This is not a weaker
"any output" check; it requires the styled screen (three markers), login, password, root prompt and
clean exit.

```sh
timeout 600 cargo xtask qtest --case login-aarch64 --arch aarch64 --no-build
# login-aarch64: pass, 34.6 s, all expectations matched, no forbidden lines
```

Serial: `R A V E N   B S D`, `Welcome to RavenBSD 0.1 aarch64`, `login: root`, `Password:`,
`raven#`, `init exited (0)`. The fixed/stale serial, JSON, case diff and `SHA256SUMS` are under
`tests/evidence/pi4/0b3bb7d1…/`. The guest image is the K096b release image
(`sha256 c6c2527f…83b2a2`); no guest code changed.

## K173 update (2026-11-19)

The K166 run reached `raven#` 5/5 but the harness exited 2 at its summary: dash/bash parsed
`printf '--- summary ---'` as an option. The fix is `printf '%s\n' '--- summary ---'`; the summary
awk also now reads fields 6/7 instead of the column names. No product code changed.

Acceptance at image `build/qtest/login-aarch64.img` (SHA-256
`15f38bcd1ffeda27348f566f60b584e8c3454a9ed4f34cb1d5b5f88af8710fa0`, commit `ffe61c55`):

```
sample  load1  accel  smp  commit    login_ms  password_ms
1       27.99  tcg    1    ffe61c55  335       2353
2       27.99  tcg    1    ffe61c55  369       3088
3       27.99  tcg    1    ffe61c55  257       1042
4       27.99  tcg    1    ffe61c55  105       1114
5       27.99  tcg    1    ffe61c55  199       1191

login_ms    n=5 min=105  median=257 p90=369  max=369
password_ms n=5 min=1042 median=1191 p90=3088 max=3088
```

`sh scripts/login-cold-start-aarch64.sh --samples 5 --image build/qtest/login-aarch64.img` exits
0; `cargo xtask qtest --case login-aarch64 --arch aarch64 --no-build` passes; `make check-fast`
passes. The host was shared (load1 27.99), so the password-leg times are not an idle-host
latency claim.

## 1. Arch staging (fixed)

`profiles/shell.d/72-uutils.toml` used `build/x86_64/uutils/*`. It now uses
`build/{arch}/uutils/*`; the profile/initfs loader resolves that placeholder through
`source_path`, and the uutils existence/name checks understand it. A clean aarch64 release build
therefore stages `build/aarch64/uutils/ls` (AArch64, static) instead of x86_64, with no
placeholder files.

* `timeout 360 cargo xtask image --arch aarch64 --platform generic --profile release --output build/aarch64/login-cold-start-release.img` → exit 0.
* Image SHA-256 `75a75b1e359585394493f4525bd2ae1516bbe9ca9332fd78c3baf896e26667ab`.
* `build/aarch64/uutils/ls`: AArch64, statically linked, `9e8cf5d3…`.
* `timeout 60 cargo nextest run -p xtask` → 547/547 passed.

## 2. Clean aarch64 rebuild (fresh hashes)

Removed `build/aarch64/user`, `build/aarch64/uutils`, `build/aarch64/ld-elf.so.1` and
`build/aarch64/libc.so.1` before the image build. The rebuilt artifacts are:

| File | Timestamp (local) | SHA-256 |
|---|---|---|
| `build/aarch64/user/login` | 2026-11-19 13:04:36 | `0276249510521affc0b37efac18fb6e9ad56b239f1f25a142762c15da89a8d7c` |
| `build/aarch64/ld-elf.so.1` | 2026-11-19 13:02:11 | `fd566b72855a2579dc2155c70733a52677c495f516c41171892f9192f27d7c25` |
| `build/aarch64/libc.so.1` | 2026-11-19 13:02:11 | `83a81c3a41031c4496f1f8377bf987314237c1583a054df955813f1128fd01f7` |
| `build/x86_64/user/login` | 2026-11-18 21:51:51 | `4b02eb76eb1447e581cfd1a3a553eaba9fa8592cb8ab3051aa1c81baad358185` |

The aarch64 hashes are identical to the pre-cleanup artifacts because the build is
deterministic; the timestamps and the clean build prove no stale x86/Nov-17 file remained.

## 3. readelf comparison

`readelf -lW build/aarch64/user/login`:

```
PHDR     0x000040 0x400040 ... R 0x8
INTERP   0x000270 ... /libexec/ld-elf.so.1
LOAD     0x000000 0x400000 0x400000 0x003c00 0x003c00 R   0x1000
LOAD     0x003c00 0x404c00 0x404c00 0x00ad84 0x00ad84 R E 0x1000
LOAD     0x00e988 0x410988 ... 0x000430 0x000678 RW  0x1000
LOAD     0x00edb8 0x411db8 ... 0x000000 0x000050 RW  0x1000
DYNAMIC / GNU_RELRO / GNU_EH_FRAME / GNU_STACK
```

`readelf -lW build/x86_64/user/login`:

```
PHDR     0x000040 0x400040 ... R 0x8
INTERP   0x000270 ... /libexec/ld-elf.so.1
LOAD     0x000000 0x400000 0x400000 0x0039cc 0x0039cc R   0x1000
LOAD     0x0039d0 0x4049d0 0x4049d0 0x01123f 0x01123f R E 0x1000
LOAD     0x014c10 0x416c10 ... 0x000e40 0x0013f0 RW  0x1000
LOAD     0x015a50 0x418a50 ... 0x000000 0x000050 RW  0x1000
DYNAMIC / GNU_RELRO / GNU_EH_FRAME / GNU_STACK
```

Both are AArch64/x86-64 PIE (`Type: DYN`), have `PT_PHDR`, an `INTERP`, and the same segment
order; neither login has a `PT_TLS` segment (static TLS is zero). The aarch64 `ld-elf.so.1` is
also AArch64 with the expected R/R E/RW layout. The old "first LOAD RW at offset 0" shape is
gone.

## 4. Fresh login still blocks before `main` (K153 history; fixed by K139/K169)

`timeout 300 sh scripts/login-cold-start-aarch64.sh --samples 5` → **exit 3**:

```
login-cold-start-aarch64: commit=5b3630f6 image=build/aarch64/login-cold-start-release.img sha256=75a75b1e… accel=tcg smp=1 samples=5 load1=12.12
login-cold-start-aarch64: BLOCKED stage=login-exit-before-password
login-cold-start-aarch64: BLOCKED at sample 1; preserved log: build/login-cold-start-aarch64/sample-1.log
```

The preserved log ends:

```
svcd: getty up
RavenBSD 0.1 (aarch64) (tty:console)
login: root
process: child exited (1)
```

No `Password:` is reached. The banner is now `(aarch64)` (Phil's K151 fix). K151's marker run
on a throwaway branch printed `K151 login main` **never**; the child exits before the first
`login` instruction, so `Console::open`, the password write and `set_echo` are never reached.
The fresh, arch-correct, x86-shaped binary reproduces that exact boundary.

## 5. Handoff (K153 history; closed by K139/K169)

Owner: **Kevin/K139** (aarch64 dynamic interpreter/libc startup), not this ticket:

* `crates/raven-spawn/src/lib.rs` (`spawn_image_with_fdmap`, interpreter/initial thread)
* `libc/ravenlibc/ld/src/**` (aarch64 self-relocation/libc startup)
* `libc/ravenlibc/crates/**` aarch64 dynamic startup

Evidence to use: this doc; the fresh hashes/readelf; image SHA
`75a75b1e359585394493f4525bd2ae1516bbe9ca9332fd78c3baf896e26667ab`; the preserved
`build/login-cold-start-aarch64/sample-1.log`; and K151's marker trace (`K151 login main` never
runs). The aarch64 `login-aarch64.toml` from `origin/phil/K151` is **not landed**: it is red
until the loader starts `main`, and a failing qtest must not enter the default suite.

## 6. Other gates

* x86 `timeout 120 cargo xtask qtest --case login-retry --arch x86_64` → exit **124** (cap
  expiry; the log `build/qtest/login-retry.log` is empty and the image was not produced). This
  is recorded as a TIMEOUT, not a pass. A subsequent `--no-build` reports the image is missing.
* `timeout 240 make check-fast` → exit 0, **`check-fast: PASSED`** (measured 60.56s host tests
  under load ~7; the 240 cap was not approached).
* `cargo run -q -p raven-provenance -- check --root .` → 0 violations (after this doc is added).

## 7. DR-52

Build/profile staging only: the placeholder and uutils slice are first-party RavenBSD build code;
no donor code was read or copied, no C, no CDDL/GPL. The login/loader handoff uses the existing
first-party aarch64 dynamic path.
