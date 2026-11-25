# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# FreeBSD application-profile compile probe (S721)

`libc/ravenlibc/tools/hdrgen/tests/abi_profile.rs` is the reusable compile-only check
for the four baseline facts of the FreeBSD application profile (DR-69, donor
`133344405f532bd54ff3629a77d4c4e062f01fdb`): `EAGAIN`, `SIGUSR1`,
`sizeof(sa_family_t)` and the `sockaddr`/`sockaddr_in` offsets. It is a Rust probe:
it emits the C source into a temporary directory at run time and compiles it with
clang. No `.c` or `.h` file is committed to this stage; the product structural policy
forbids C/C++ additions outside `addons/`.

## Expected donor values

| Fact | Donor header | Value |
|---|---|---|
| `EAGAIN` | `sys/sys/errno.h` | 35 |
| `SIGUSR1` | `sys/sys/signal.h` | 30 |
| `sizeof(sa_family_t)` | `sys/sys/_types.h` | 1 |
| `offsetof(struct sockaddr_in, sin_family)` | `sys/sys/socket.h` | 1 |
| `sizeof(struct sockaddr_in)` | `sys/sys/socket.h` | 16 |
| `offsetof(struct sockaddr, sa_len)` | `sys/sys/socket.h` | 0 |

The donor values are independently verified by `hdrgen verify --profile freebsd`.
The probe compiles the same source twice per architecture: once against a
donor-shaped include tree built in the temporary directory (positive control, must
pass) and once against Raven's staged public header root.

## Run

```sh
cargo nextest run -p raven-libc-hdrgen --offline -E 'binary(abi_profile)'
```

The staged public header roots used by the probe are:

```text
libc/ravenlibc/include
libc/ravenlibc/abi/generated/include
libc/ravenlibc/arch/<x86_64|aarch64>/include
```

Both architectures are expected to report the named baseline rows before the
FreeBSD header cutover; the failures are the remaining red profile gaps, not a test
defect.
