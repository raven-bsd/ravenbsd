# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# FreeBSD application-profile compile fixture (S721)

`fixture.c` is the reusable compile-only check for the four baseline facts of the
FreeBSD application profile (DR-69, donor `133344405f532bd54ff3629a77d4c4e062f01fdb`):
`EAGAIN`, `SIGUSR1`, `sizeof(sa_family_t)` and the `sockaddr`/`sockaddr_in`
offsets. It is a compile-time fixture: it certifies no runtime behaviour.

## Expected donor values

| Fact | Donor header | Value |
|---|---|---|
| `EAGAIN` | `sys/sys/errno.h` | 35 |
| `SIGUSR1` | `sys/sys/signal.h` | 30 |
| `sizeof(sa_family_t)` | `sys/sys/_types.h` | 1 |
| `offsetof(struct sockaddr_in, sin_family)` | `sys/sys/socket.h` | 1 |
| `sizeof(struct sockaddr_in)` | `sys/sys/socket.h` | 16 |
| `offsetof(struct sockaddr, sa_len)` | `sys/sys/socket.h` | 0 |

The donor values are verified by `hdrgen verify --profile freebsd` and by compiling
this file against a constructed donor include tree.

## Run against Raven

```sh
clang --target=<x86_64|aarch64>-unknown-ravenbsd -nostdinc \
  -isystem "$(clang -print-resource-dir)/include" \
  -isystem libc/ravenlibc/include \
  -isystem libc/ravenlibc/abi/generated/include \
  -isystem libc/ravenlibc/arch/<x86_64|aarch64>/include \
  -fsyntax-only tests/conformance/abi-profile/fixture.c
```

Both architectures are expected to fail before the FreeBSD header cutover; the
failures are the remaining red profile gaps, not a test defect.
