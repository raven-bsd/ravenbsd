# Capabilities: rights, transfer, revoke and generation

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED (the kernel behaviour described here is exercised by the S73 `cap-revoke` qtest).
This documents the user-visible capability contract that distinguishes the native model — rights
on *handles* — from a scheme-string router.

## 1. Model

A capability is a handle in a process's handle table. The handle carries a rights mask and a
generation, and every live handle belongs to a *lineage* (a derivation tree). There are no
scheme strings here: a handle names an object directly, and its rights say what it may do.

| Right | Bit | Meaning |
|---|---|---|
| `READ` | 0 | read from a file/pipe, receive |
| `WRITE` | 1 | write, send/call |
| `MAP` | 2 | map the object into an address space |
| `EXEC` | 3 | execute |
| `DUP` | 4 | duplicate the handle |
| `TRANSFER` | 5 | hand the handle to another process |
| `SIGNAL` | 6 | signal/cancel |
| `ADMIN` | 7 | administrative |

`MEMOBJ_CREATE` returns a handle with `READ|WRITE|MAP|DUP|TRANSFER`; `OPEN` grants the rights the
flags imply. A handle's rights never grow: every derived handle carries a subset of its parent's
effective rights.

## 2. Transfer (`HANDLE_TRANSFER`, 0x52)

`transfer(dest_proc, source, mask)` installs a *derived* handle in `dest_proc` whose rights are
`source_effective ∩ mask`; it derives from the source's lineage node. The source handle stays
open. Transfer is the "hand off a reduced capability to a child/server" primitive; a scheme reply
attachment and `HANDLE_COPY` (0x54, `ADMIN`) do the same with their own policy.

## 3. Revoke (`HANDLE_REVOKE`, 0x53)

`revoke(handle, mask)` removes `mask` (or, when `mask == 0`, every usable right) from `handle` and
every handle derived from it, including handles already transferred to another process. The handle
stays open. A later use that needs a revoked right fails with **`EPERM`**; a right that was never
granted fails with **`EACCES`**. Revocation is one-shot and idempotent; revoking an unknown or
closed handle is **`EBADF`**.

## 4. Generation and lifetime

A handle packs a 24-bit table index and an 8-bit generation. Closing a slot bumps its generation,
so a stale handle fails with **`EBADF`** instead of touching a reused object; a slot retired after
255 closes is never reused. The underlying object (a `MemObject`, `SchemeFile`, …) is reference
counted and is freed only when the last live handle closes — a process that closes its *source*
handle does not free an object that a child still holds.

## 5. The S73 transcript

1. Controller creates a `MemObject` (rights `READ|WRITE|MAP|DUP|TRANSFER`).
2. Controller transfers it to a child with `mask = MAP|READ`; the child gets `MAP|READ` only.
3. Child maps the object (**MAP ok**) and confirms `DUP` is denied (**EACCES** — reduced rights).
4. Controller revokes `MAP` on the lineage.
5. Child's next map fails with **`EPERM`** (the right was revoked).
6. A second object is transferred with `MAP`; the controller closes its source handle; the child
   still maps it (**alive** — released only after the last live handle).
7. Child closes the handle (object freed), a new object reuses the slot with a fresh generation,
   and the stale handle fails with **`EBADF`** (no stale-handle reuse).

The qtest asserts these lines verbatim (`tests/qtest/cases/cap-revoke.toml`).
