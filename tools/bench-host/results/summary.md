<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# bench-host results

Host: 20 CPUs, 12th Gen Intel(R) Core(TM) i7-12700H (`x86_64`), kernel 5.19.0-50-generic, QEMU QEMU emulator version 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31), KVM true, governor powersave.

| Measurement | Unit | n | Median | p99 | Min | Max | Note |
|---|---|---:|---:|---:|---:|---:|---|
| fork-exec | ns/op | 1000 | 223373.0 | 519202.0 | 210946.0 | 801992.0 |  |
| microvm-boot | ns/op | 0 | - | - | - | - | not measurable: kernel /boot/vmlinuz-5.19.0-50-generic is not readable: Permission denied (os error 13) |
| ns-start | ns/op | 200 | 373106.5 | 993719.0 | 337087.0 | 1079108.0 | namespaces=user,pid,mnt,net |
| ns-start-rss | bytes | 200 | 1380352 | 1380352 | 1380352 | 1380352 | namespaces=user,pid,mnt,net |
| podman-rss | bytes | 1 | 299000 | 299000 | 299000 | 299000 | image=docker.io/library/alpine:3.22; podman stats on a sleep container |
| podman-start | ns/op | 20 | 257784638.5 | 367859496.0 | 241279811.0 | 367859496.0 | image=docker.io/library/alpine:3.22; local image, --pull=never |
| ptrace-syscall | ns/op | 30 | 3648.8 | 4009.0 | 3581.1 | 4009.0 | slowdown_vs_native=66.54x |
| syscall-rtt-clock_gettime | ns/op | 30 | 80.4 | 81.9 | 79.3 | 81.9 |  |
| syscall-rtt-getpid | ns/op | 30 | 54.8 | 68.7 | 53.9 | 68.7 |  |
| syscall-rtt-read-zero | ns/op | 30 | 132.2 | 152.6 | 130.5 | 152.6 |  |

All timings are wall-clock nanoseconds. `n=0` means the item was not measurable; see the note and the report.
