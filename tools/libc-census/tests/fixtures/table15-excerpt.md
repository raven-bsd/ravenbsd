<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Fixture excerpt for compare-table

#### 3.B.4 The census table

Columns are invented for the fixture.

| Function | glibc | musl | FBSD | NBSD | OBSD | bionic | illumos | mlibc | picolibc | n6 | Ring | Action |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `strlcpy` | 1/1 | 1/1 | 1/1 | 1/1 | 1/1 | 0/1 | 1/1 | 1/1 | 1/1 | 6 | 1 | fixture |
| `epoll_create` `epoll_create1` | 1/1 | 1/1 | 0/0 | 1/1 | 0/0 | 1/1 | 1/1 | 1/1 | 0/0 | 4 | 1* | fixture |
| `environ` (var) | 1/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 1 | 1 | fixture |
| `strlcat` | 1/1 | 1/1 | 1/1 | 1/1 | 1/1each | 0/1 | 1/1 | 1/1 | 1/1 | 6 | 1 | fixture |
| `<string.h>` | yes | yes | yes | yes | yes | yes | yes | yes | yes | 9 | 1 | fixture |
| `<sys/queue.h>` | no | no | yes | **yes** | yes | no | yes | no | no | 4 | 1 | fixture |

#### 3.B.5 Licence classes

Fixture text.
