<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Fixture excerpt for seed-assignments

| id | header | symbols | ring | pers | doc | crate | status | notes |
|---|---|---|---|---|---|---|---|---|
| X01 | string.h | `strlcpy strlcat` | 1 | - | 01 | str | EXISTS-donor | P24? |
| X02 | string.h | `memmem memrchr` | 1 | - | 01 | str | EXISTS-donor | |
| L01 | sys/epoll.h | `epoll_create epoll_create1` | 2 | linux | 11/15 | p-linux | TO-BE-DESIGNED | EVQ |
| Z03 | (data) | `environ __environ _environ` | 0/1 | - | 03 | stdlib | EXISTS-donor | size in frozen file |
| X99 | thing.h | `pthread_attr_* <stdbit.h>` | 1 | - | 01 | str | EXISTS-donor | patterns |
