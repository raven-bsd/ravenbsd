<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Doors: no `door_*` ABI in ravenlibc

illumos doors are a kernel object around a server function: the client's call runs on a server
thread in the same scheduling entity, and the kernel supplies the caller's authenticated
identity (`06-solaris-design.md` A-sol-46). Raven's analogue is the **scheme `CALL`** protocol
with `raven-scheme` server loops (ticket K30), not a libc facility.

What this ticket does **not** provide, deliberately:

* no `door_create`, `door_call`, `door_return`, `door_server_create`, `door_ucred`,
  `door_cred`, `door_unbind` export;
* no `include/door.h` (`donor-map/14` 3.B.3.e marks it SKIP; `02-api-surface` S04 is SKIP);
* no server-thread pool in libc. The pool policy (target idle threads, grow on demand, daemon
  flag) belongs to `raven-scheme::pool`, and a daemon-marked pool thread must not keep the
  process alive (A-sol-11).

Mapping for a port that asks for doors:

| illumos | Raven |
|---|---|
| `door_create(fn, cookie, attrs)` | create a scheme server and register it (`SCHEME_REGISTER`) |
| `door_call(door, args)` | `CALL` on the scheme handle with the request bytes |
| `door_return` | the server loop answers the `CALL` |
| `door_ucred`/`door_cred` | caller identity attached by the kernel to every request (`PG-27`) |
| `door_server_create` | `raven-scheme::pool` policy, daemon-marked threads |

A `libdoor-compat.so` shim would be a separate add-on ticket, only if a real port needs it
(C67 12.8). Nothing is exported or declared today.
