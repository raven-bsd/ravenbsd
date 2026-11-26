<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release notes (fixture, 1-CPU failure)

### libc thread 1-CPU crash — BLOCKED

`smp-libc-concurrency` fails at **one CPU**, so it is not an SMP/TLB failure:

```
$ cargo xtask qtest --case smp-libc-concurrency-kvm1 --arch x86_64
smp-libc-concurrency: start
process: child killed by page fault at 0xca3a10
process: child exited (-11)
svcd: libc-test crash loop, giving up
init exited (1)
exit status 35
```

The fault occurs before `malloc ok`. This is a libc thread exit/join/TLS/stack defect; owner
**S132/Tim**.

### TCG4 boot/login reliability — BLOCKED
