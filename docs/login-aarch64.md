<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 dynamic login exits before main (K154)

Status: **BLOCKED — the loader reaches `enter`; the first failing stage is the aarch64 Rust
std/`raven_rt` start path in the login program, not `ld-elf.so.1` or `crates/raven-spawn`.**

## Reproduction

`sh scripts/login-cold-start-aarch64.sh --samples 5` on the fresh image
`build/aarch64/login-cold-start-release.img` (SHA-256 `47108e64…` at the start of this ticket)
stops at sample 1:

```text
login-cold-start-aarch64: BLOCKED stage=login-exit-before-password
login: root
process: child exited (1)
```

Fresh binaries: `login` is AArch64 DYN/PIE with `PT_PHDR`, `INTERP=/libexec/ld-elf.so.1`,
`NEEDED libc.so.1`, entry `0x404cb8`, no `PT_TLS`. `getty` is also AArch64 DYN/PIE with the same
interpreter and `libc.so.1`; it runs and prints the console banner, so the loader/libc path works
for a dynamic aarch64 program.

## Loader stage trace (temporary markers)

A diagnostic image with marker writes in the aarch64 loader showed every loader stage:

```text
K154 selfrel
K154 main
K154 premap-ok
K154 needed
K154 tls
K154 reloc
K154 enter
login: root
process: child exited (1)
```

`libc pre-map` succeeded (no fallback), all dependency loads and relocations completed, and the
loader transferred to `AT_ENTRY`. No `libc start` marker (from `__raven_libc_start`) and no
`K154 login main` marker appeared.

## Entry analysis

`aarch64-linux-gnu-objdump -D --start-address=0x404cb8 build/aarch64/user/login`:

```text
404cb8: mov  x0, sp
404cbc: and  x0, x0, #0xfffffffffffffff0
404cc0: mov  sp, x0
404cc4: bl   0x40a760
404cc8: brk  #0
```

The login uses `raven_rt::entry!(main)`, and this `_start` calls the `raven_rt`/Rust std start at
`0x40a760`, not the project `Scrt1`/`__raven_libc_start` five-argument path. The loader's
`enter` is therefore correct; the failure is inside the login program's Rust std/`raven_rt` start
before `main`.

`getty` uses the same dynamic shape and runs, so a generic loader or libc mapping fault is ruled
out. The remaining difference is the login binary's start path (or its Rust std/libc startup
state), outside K154's Owns.

## Handoff

* **K135 / Phil** for the login program's aarch64 start path and `Console::open`/`main` trace.
* If the std/`raven_rt` start itself is wrong, a separate std/crt ticket owns it; K154's Owns are
  `libc/ravenlibc/ld/**` and `crates/raven-spawn/**`, and both are proven complete.
* Preserve `build/login-cold-start-aarch64/sample-1.log` for that trace. Do not use a static login,
  skipped prompt, raised timeout or fake sample.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | RavenBSD decision |
|---|---|---|---|---|
| aarch64 entry and `AT_PHDR` | Runtime linker entry; behaviour only, no code copied. | FreeBSD rtld/aarch64 start; behaviour only, no code copied. | Linux aarch64 `_start`/`AT_PHDR`; idea only. | Keep the first-party loader; the defect is in the login/start path, not donor code. |
