# S744 sysctl conformance

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

This topic is the target case for the declared FreeBSD sysctl MIB subset. `build.sh` builds a
`no_std` Rust probe against the freshly packed ravenlibc for the selected architecture, refreshes
the minimal `base` initfs plus the `sysd`/`consd` server overlays, and adds the probe and a one-shot
service. `sysctl.toml` is the qtest case and is also discovered by
`tests/conformance/aarch64/run.py`, which rewrites only the architecture and console details.

## Run

```sh
cargo xtask qtest --case tests/conformance/sysctl/sysctl.toml
python3 tests/conformance/aarch64/run.py --table tests/conformance/sysctl/sysctl.toml
```

The probe exits zero only when all twelve normal rows pass and the three mutations are caught. The
qtest case asserts the normal summary, the three mutation end lines and the absence of a panic.
`build.sh` applies the interim 4 KiB `shape-check.sh` gate on x86_64 only; the aarch64 linker script
uses the 64 KiB segment alignment its loader expects.

## Matrix (pinned FreeBSD `133344405f`)

| Name | MIB | Encoding | Read/write | Provider and result |
|---|---|---|---|---|
| `kern.ostype` | 1,1 | string incl NUL | read | `uname().sysname` |
| `kern.osrelease` | 1,2 | string incl NUL | read | `uname().release` |
| `kern.version` | 1,4 | string incl NUL | read | `uname().version` |
| `kern.hostname` | 1,10 | string incl NUL, < 256 bytes | read/write | `sys:hostname`; provider `HOST_NAME_MAX=64` (G-27) |
| `kern.hostid` | 1,11 | `CTLTYPE_ULONG`, 8 bytes | read | constant `0` (identity); write unsupported (`EPERM`) |
| `kern.domainname` | 1,22 | string incl NUL | read | empty NIS domain; write unsupported (`EPERM`) |
| `kern.argmax` | 1,8 | `CTLTYPE_INT`, 4 bytes | read | `limits::ARG_MAX` |
| `hw.machine` | 6,1 | string incl NUL | read | `uname().machine` |
| `hw.model` | 6,2 | string | read | no authoritative CPU-model provider: `ENOENT` |
| `hw.ncpu` | 6,3 | `CTLTYPE_INT`, 4 bytes | read | `SYSINFO.cpus_online`; `ENOENT` until PG-21 wires `SYSINFO` |
| `hw.physmem` | 6,5 | `CTLTYPE_ULONG`, 8 bytes | read | `SYSINFO.mem_total_pages * page size`; `ENOENT` until PG-21 |
| `hw.pagesize` | 6,7 | `CTLTYPE_INT`, 4 bytes | read | the OS page size (4096) |

A too-small buffer copies as many bytes as fit, returns `ENOMEM`, and reports the copied byte count
in `*oldlenp` (FreeBSD `sysctl_old_user` and `sysctl(3)`). `oldp = NULL` reports the needed length.
`namelen` outside `2..=24` is `EINVAL`; an unknown name or MIB is `ENOENT`; a NULL name, MIB or
length pointer is `EFAULT` where FreeBSD reads it. `newp = NULL` ignores `newlen`. A write copies
the old value out first and is skipped when that copy is short. `sysctlnametomib` keeps the
caller's element count on `ENOMEM`, matching FreeBSD's wrapper.

`hw.model` is a declared name with no provider; it is not synthesised from `uname().machine`.

## Rows

`mib-numbers`, `sizing`, `short-buffer`, `unknown-mib`, `malformed-mib`, `invalid-pointers`,
`dynamic-provider`, `unsupported-model`, `write-readonly`, `hostname-roundtrip`, `canary`,
`guard-page`.

The dynamic row compares `sysctl` with independent providers in the same boot: `uname`,
`gethostname`, and `sysconf` (`_SC_ARG_MAX`, `_SC_PAGESIZE`). `hw.ncpu` and `hw.physmem` are
declared but have no `SYSINFO` provider until PG-21, so the row requires `ENOENT` for both rather
than comparing a fabricated fallback. The canary row checks the bytes outside the reported length;
the guard-page row places a value exactly at a `PROT_NONE` boundary so an overrun faults the probe.

A non-NULL invalid address is a caller contract violation on Raven: `sysctl` is a libc function in
the caller's address space and cannot copy the buffer through a kernel boundary, so only the NULL
pointer cases return `EFAULT`/`ENOMEM`. The guard-page row proves the implementation does not write
past `*oldlenp`; it does not claim `EFAULT` for a `PROT_NONE` buffer.

## Mutations

`required-length` asserts that a short integer buffer reports the needed width; `unknown-mib`
asserts that an unknown MIB succeeds; `privilege` asserts that a read-only write succeeds. Against
the correct product each targeted row is red, so each mutation section ends with `failures=1`.

## Donor map

- FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2/3-Clause):
  `sys/sys/sysctl.h` (numbers), `sys/kern/kern_sysctl.c` (`sys___sysctl`,
  `userland_sysctl`, `sysctl_old_user`, `sysctl_root`, `sysctl_handle_int`,
  `sysctl_handle_string`, `sysctl_sysctl_name2oid`), `sys/kern/kern_mib.c`
  (`KERN_ARGMAX`, `HW_NCPU`, `HW_PAGESIZE`, `HW_PHYSMEM`, `sysctl_hostname`,
  `sysctl_hostid`), `lib/libc/gen/{sysctl,sysctlbyname,sysctlnametomib}.c`, and the
  `sysctl(3)` manual. Read for behaviour; no C code is copied or translated.
- NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`: `lib/libc/gen/sysctlnametomib.c` and
  `sysctlgetmibinfo.c` for the name-to-MIB shape. Not the native ABI.
- Illumos and Linux have no equivalent public BSD MIB contract; no donor expression is used.
