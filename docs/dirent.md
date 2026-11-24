<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Directory-entry conversion (K172)

Status: **OBSERVED at `48e3a98c`.** libc `getdents` read a 4096-byte K60 page into a 2048-byte
readdir buffer and advanced the server cursor past records that did not fit. A 100-entry directory
returned only 85 names. The conversion now reads 3/4 of the caller buffer, so every converted
record fits; the fixed run returns all names in order.

## 1. Reproducer

`tests/qtest/cases/ls-dirent.toml` creates `/tmp/dirent` with:

- 62 one-byte names (`a`..`z`, `A`..`Z`, `0`..`9`);
- 38 two-byte names (`aa`..`bl`);
- one 200-byte name.

The raw K60 page is larger than 4096 bytes and the Linux `dirent` conversion is larger than the
2048-byte `DirStream` buffer. Pre-fix:

```text
raven# ls -a /tmp/dirent > /tmp/list
raven# wc -l /tmp/list
87 /tmp/list
```

Expected `103` (101 names plus `.` and `..`); 15 names were silently lost. The server cursor had
advanced over the whole 4096-byte page.

## 2. Fix

`libc/ravenlibc/crates/raven-libc-platform/src/compat.rs::getdents` now reads at most
`buffered_len * 3 / 4` raw bytes (clamped to `272..=4096`) instead of a fixed 4096. A K60 record
is `16 + name` bytes (8-aligned) and a Linux record is `20 + name` (8-aligned); the worst
expansion is 4/3 (7-8 byte names), so a 3/4 read cannot overflow the caller buffer. A buffer
smaller than the 280-byte maximum Linux record returns `EINVAL` rather than losing a record.

Ordering, the server cursor, short reads, EOF, malformed-record `EPROTO` handling and concurrent
mutation semantics are unchanged. No dirent ABI or vfsd/initfsd protocol change; no cache.

## 3. Regression

```text
$ timeout 120 cargo xtask qtest --case ls-dirent --arch x86_64
ls-dirent: pass                 # duration_ms 23822
exit 0
103 /tmp/list
101 /tmp/list
```

The pre-fix run of the same case returned `87 /tmp/list` and failed the count. Boundary names
include one-byte and 200-byte entries; the case uses KVM to avoid TCG host-load noise.

## 4. Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| `readdir` refill | `getdents` outcome only. | musl `readdir` refills a 2048-byte buffer and returns one record at a time; no code copied. | POSIX `getdents` returns records that fit the caller buffer. | First-party read-size bound; no ABI change. |
| Lossless conversion | Not applicable. | BSD dirent conversion outcome only. | `getdents` buffer semantics only. | 3/4 caller-buffer read with 4/3 worst-case expansion. |
