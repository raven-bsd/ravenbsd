<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc size report

Metric: `S = Berkeley text+data of the stripped executable; B = bss; F = file bytes`.
Budgets: measured + 3 percent (`R-24`). `status = skipped: ...` is never a pass.

## x86_64

| probe | linkage | set | lang | S | B | F | members | needed | budget | status |
|---|---|---|---|---|---|---|---|---|---|---|
| P0 | static | tiny | rust | 27869 | 5768 | 29264 | 45 | 0 | 0 | ok |
| P1 | static | tiny | rust | 28409 | 9328 | 29800 | 47 | 0 | 0 | ok |
| P2 | static | tiny | rust | 63861 | 9496 | 65320 | 100 | 0 | 0 | ok |
| P3 | static | tiny | rust | 64557 | 8800 | 66016 | 100 | 0 | 0 | ok |
| P4 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | tiny | rust | 126475 | 9568 | 127864 | 88 | 0 | 0 | ok |
| P9 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: fgets |
| P0 | static | tiny | c | 27928 | 5776 | 29184 | 71 | 0 | 0 | ok |
| P1 | static | tiny | c | 28468 | 9336 | 29720 | 73 | 0 | 0 | ok |
| P2 | static | tiny | c | 63912 | 9496 | 65240 | 126 | 0 | 0 | ok |
| P3 | static | tiny | c | 63920 | 9480 | 65256 | 126 | 0 | 0 | ok |
| P4 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | tiny | c | 126498 | 9600 | 127760 | 114 | 0 | 0 | ok |
| P9 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: P9 is the Rust full link (generated p9_all.rs) |
| P0 | static | server | rust | 27248 | 6384 | 28648 | 37 | 0 | 0 | ok |
| P1 | static | server | rust | 27761 | 5872 | 29160 | 38 | 0 | 0 | ok |
| P2 | static | server | rust | 66417 | 6936 | 67880 | 80 | 0 | 0 | ok |
| P3 | static | server | rust | 67113 | 6240 | 68576 | 80 | 0 | 0 | ok |
| P4 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | server | rust | 132736 | 7400 | 134128 | 71 | 0 | 0 | ok |
| P9 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: fgets |
| P0 | static | server | c | 27319 | 6376 | 28584 | 63 | 0 | 0 | ok |
| P1 | static | server | c | 27848 | 5848 | 29112 | 64 | 0 | 0 | ok |
| P2 | static | server | c | 66477 | 6928 | 67808 | 106 | 0 | 0 | ok |
| P3 | static | server | c | 66485 | 6912 | 67824 | 106 | 0 | 0 | ok |
| P4 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | server | c | 132764 | 7424 | 134032 | 97 | 0 | 0 | ok |
| P9 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: P9 is the Rust full link (generated p9_all.rs) |
| P0 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P1 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P2 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P3 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P4 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P5 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P6 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P7 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P8 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P9 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P0 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P1 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P2 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P3 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P4 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P5 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P6 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P7 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P8 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P9 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P0 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P1 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P2 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P3 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P4 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P5 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P6 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P7 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P8 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P9 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P0 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P1 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P2 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P3 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P4 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P5 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P6 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P7 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P8 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P9 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |

Boot: syscalls before `main` 0, start instructions 816, `.init_array` 0, `.preinit_array` 0, `.ctors 0`.

* `[boot-malloc]` early allocator member: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.01.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.04.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.06.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.07.rcgu.o
* `[boot-malloc]` getenv -> raven_libc-ad3bdc61da7c9401.raven_libc.6dc199050756d7d8-cgu.241.rcgu.o -> raven_libc-ad3bdc61da7c9401.raven_libc.6dc199050756d7d8-cgu.032.rcgu.o -> free

## aarch64

| probe | linkage | set | lang | S | B | F | members | needed | budget | status |
|---|---|---|---|---|---|---|---|---|---|---|
| P0 | static | tiny | rust | 37771 | 8160 | 39160 | 44 | 0 | 0 | ok |
| P1 | static | tiny | rust | 38843 | 7088 | 40232 | 46 | 0 | 0 | ok |
| P2 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | tiny | rust | 138822 | 9512 | 140208 | 88 | 0 | 0 | ok |
| P9 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P0 | static | tiny | c | 35747 | 6152 | 37000 | 71 | 0 | 0 | ok |
| P1 | static | tiny | c | 36699 | 9296 | 37952 | 73 | 0 | 0 | ok |
| P2 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | tiny | c | 133786 | 6416 | 135040 | 115 | 0 | 0 | ok |
| P9 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: P9 is the Rust full link (generated p9_all.rs) |
| P0 | static | server | rust | 35154 | 6680 | 36544 | 37 | 0 | 0 | ok |
| P1 | static | server | rust | 35950 | 5888 | 37336 | 38 | 0 | 0 | ok |
| P2 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | server | rust | 138843 | 9488 | 140232 | 74 | 0 | 0 | ok |
| P9 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P0 | static | server | c | 34098 | 7800 | 35352 | 64 | 0 | 0 | ok |
| P1 | static | server | c | 34854 | 7040 | 36112 | 65 | 0 | 0 | ok |
| P2 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | server | c | 136559 | 7736 | 137816 | 101 | 0 | 0 | ok |
| P9 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: P9 is the Rust full link (generated p9_all.rs) |
| P0 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P1 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P2 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P3 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P4 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P5 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P6 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P7 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P8 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P9 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P0 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P1 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P2 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P3 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P4 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P5 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P6 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P7 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P8 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P9 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P0 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P1 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P2 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P3 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P4 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P5 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P6 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P7 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P8 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P9 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P0 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P1 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P2 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P3 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P4 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P5 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P6 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P7 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P8 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P9 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |

Boot: syscalls before `main` 2, start instructions 1439, `.init_array` 0, `.preinit_array` 0, `.ctors 0`.

* `[boot-malloc]` early allocator member: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.01.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.04.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.06.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.07.rcgu.o
* `[boot-malloc]` getenv -> raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.161.rcgu.o -> raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.023.rcgu.o -> free

## riscv64

| probe | linkage | set | lang | S | B | F | members | needed | budget | status |
|---|---|---|---|---|---|---|---|---|---|---|
| P0 | static | tiny | rust | 28488 | 9256 | 29992 | 46 | 0 | 0 | ok |
| P1 | static | tiny | rust | 29004 | 8744 | 30504 | 48 | 0 | 0 | ok |
| P2 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | tiny | rust | 113238 | 6424 | 114744 | 88 | 0 | 0 | ok |
| P9 | static | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P0 | static | tiny | c | 28566 | 9176 | 30072 | 59 | 0 | 0 | ok |
| P1 | static | tiny | c | 29086 | 8656 | 30592 | 61 | 0 | 0 | ok |
| P2 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | tiny | c | 113324 | 6344 | 114824 | 101 | 0 | 0 | ok |
| P9 | static | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: P9 is the Rust full link (generated p9_all.rs) |
| P0 | static | server | rust | 25884 | 7768 | 27384 | 38 | 0 | 0 | ok |
| P1 | static | server | rust | 26328 | 7320 | 27832 | 39 | 0 | 0 | ok |
| P2 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | server | rust | 114636 | 9128 | 116136 | 75 | 0 | 0 | ok |
| P9 | static | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: = note: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P0 | static | server | c | 25962 | 7688 | 27464 | 51 | 0 | 0 | ok |
| P1 | static | server | c | 26410 | 7240 | 27912 | 52 | 0 | 0 | ok |
| P2 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P3 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: link: rust-lld: error: undefined symbol: __raven_variadic_unsupported_printf |
| P4 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: pthread_create |
| P5 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: getaddrinfo |
| P6 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: dlopen |
| P7 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: undefined symbol: strftime |
| P8 | static | server | c | 114722 | 9040 | 116224 | 88 | 0 | 0 | ok |
| P9 | static | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: P9 is the Rust full link (generated p9_all.rs) |
| P0 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P1 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P2 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P3 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P4 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P5 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P6 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P7 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P8 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P9 | dynamic | tiny | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P0 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P1 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P2 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P3 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P4 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P5 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P6 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P7 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P8 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P9 | dynamic | tiny | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: dynamic tiny does not exist (tiny is static) |
| P0 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P1 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P2 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P3 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P4 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P5 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P6 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P7 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P8 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P9 | dynamic | server | rust | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P0 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P1 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P2 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P3 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P4 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P5 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P6 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P7 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P8 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |
| P9 | dynamic | server | c | 0 | 0 | 0 | 0 | 0 | 0 | skipped: libc.so.1 absent (C62 not landed) |

Boot: syscalls before `main` 0, start instructions 39, `.init_array` 0, `.preinit_array` 0, `.ctors 0`.

* `[boot-malloc]` early allocator member: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.01.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.04.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.06.rcgu.o
* `[boot-malloc]` early allocator member: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.07.rcgu.o
* `[boot-malloc]` getenv -> raven_libc-ab9dd22f2f6ab9d4.raven_libc.2ec755fbaf62545f-cgu.159.rcgu.o -> raven_libc-ab9dd22f2f6ab9d4.raven_libc.2ec755fbaf62545f-cgu.023.rcgu.o -> free

## Failing probe rules

* P0 x86_64 static rust: `P0-R-9` member `malloc*` pulled: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.01.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.04.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.06.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.07.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter12pad_integral
* P1 x86_64 static rust: `P1-R-15` member `malloc*` pulled: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.01.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.04.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.06.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.07.rcgu.o
* P2 x86_64 static rust: `P2-R-13` member `regex*` pulled: raven_libc_regex-9613249de89ae1ef.raven_libc_regex.ace6f165a1da806b-cgu.02.rcgu.o, raven_libc_regex-9613249de89ae1ef.raven_libc_regex.ace6f165a1da806b-cgu.05.rcgu.o
* P3 x86_64 static rust: `P3-R-13` no member matches fmt_float*
* P0 x86_64 static c: `P0-R-9` member `malloc*` pulled: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.01.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.04.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.06.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.07.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter12pad_integral
* P1 x86_64 static c: `P1-R-15` member `malloc*` pulled: raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.01.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.04.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.06.rcgu.o, raven_libc_malloc-7aadee5db6ef10c3.raven_libc_malloc.c43405e7e46846e0-cgu.07.rcgu.o
* P2 x86_64 static c: `P2-R-13` member `regex*` pulled: raven_libc_regex-9613249de89ae1ef.raven_libc_regex.ace6f165a1da806b-cgu.02.rcgu.o, raven_libc_regex-9613249de89ae1ef.raven_libc_regex.ace6f165a1da806b-cgu.05.rcgu.o
* P3 x86_64 static c: `P3-R-13` no member matches fmt_float*
* P0 x86_64 static rust: `P0-R-9` symbol `core3fmt*` present: _RNvNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter12pad_integral
* P2 x86_64 static rust: `P2-R-13` member `regex*` pulled: raven_libc_regex-a86411995354beb5.raven_libc_regex.ffd3fc91d4b2a7c4-cgu.02.rcgu.o
* P3 x86_64 static rust: `P3-R-13` no member matches fmt_float*
* P0 x86_64 static c: `P0-R-9` symbol `core3fmt*` present: _RNvNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCs8ROlWsJZPDz_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCs8ROlWsJZPDz_4core3fmtNtB5_9Formatter12pad_integral
* P2 x86_64 static c: `P2-R-13` member `regex*` pulled: raven_libc_regex-a86411995354beb5.raven_libc_regex.ffd3fc91d4b2a7c4-cgu.02.rcgu.o
* P3 x86_64 static c: `P3-R-13` no member matches fmt_float*
* P0 aarch64 static rust: `P0-R-9` member `malloc*` pulled: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.01.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.04.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.06.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.07.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCsa54BgosZjhp_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsa54BgosZjhp_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsa54BgosZjhp_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter12pad_integral
* P0 aarch64 static rust: `R-30` 26 member(s) above 4096 bytes are not in size-allowlist.toml: core-62d2c2da6b64b23f.core.756df07a42d278dd-cgu.0.rcgu.o=184911, raven_libc_platform-935daf8a59ceb444.raven_libc_platform.96f1a08e4cf95614-cgu.00.rcgu.o=40735, alloc-178a59339fc63ec5.alloc.2a577af655833e63-cgu.0.rcgu.o=33479, raven_libc_ctype-9c40b3c63d953ace.raven_libc_ctype.f9d4ebad5b14bdb2-cgu.44.rcgu.o=33304, raven_libc_regex-be1981b13fcb4b83.raven_libc_regex.a6e7d68735d67664-cgu.00.rcgu.o=28279, raven_libc_regex-be1981b13fcb4b83.raven_libc_regex.a6e7d68735d67664-cgu.01.rcgu.o=16870, raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.000.rcgu.o=14841, raven_libc_platform-935daf8a59ceb444.raven_libc_platform.96f1a08e4cf95614-cgu.01.rcgu.o=11709, raven_libc_dtoa-ddce65dd59699571.raven_libc_dtoa.21fbf1abef819db8-cgu.00.rcgu.o=11674, raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.002.rcgu.o=9484, raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.001.rcgu.o=8871, raven_libc_dtoa-ddce65dd59699571.raven_libc_dtoa.21fbf1abef819db8-cgu.03.rcgu.o=8020, raven_libc_dtoa-ddce65dd59699571.raven_libc_dtoa.21fbf1abef819db8-cgu.01.rcgu.o=7726, raven_libc_dtoa-ddce65dd59699571.raven_libc_dtoa.21fbf1abef819db8-cgu.02.rcgu.o=6985, raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.004.rcgu.o=6493, raven_libc_glob-865e988ad7748d39.raven_libc_glob.2ec10fdcec5a2efb-cgu.00.rcgu.o=6408, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.006.rcgu.o=6332, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.013.rcgu.o=5340, raven_libc_platform-935daf8a59ceb444.raven_libc_platform.96f1a08e4cf95614-cgu.04.rcgu.o=5024, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.075.rcgu.o=4948, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.019.rcgu.o=4680, raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.003.rcgu.o=4471, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.00.rcgu.o=4457, raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.005.rcgu.o=4326, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.016.rcgu.o=4280, raven_libc-912e24dc93ebdaef.raven_libc.801f850f463ff766-cgu.007.rcgu.o=4176
* P1 aarch64 static rust: `P1-R-15` member `malloc*` pulled: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.01.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.04.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.06.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.07.rcgu.o
* P0 aarch64 static c: `P0-R-9` member `malloc*` pulled: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.01.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.04.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.06.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.07.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCsa54BgosZjhp_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsa54BgosZjhp_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsa54BgosZjhp_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter12pad_integral
* P1 aarch64 static c: `P1-R-15` member `malloc*` pulled: raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.01.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.04.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.06.rcgu.o, raven_libc_malloc-0f85f76a46dc2838.raven_libc_malloc.f11b7cc444e956ea-cgu.07.rcgu.o
* P0 aarch64 static rust: `P0-R-9` member `stdio*` pulled: raven_libc_stdio-10e7df5389dbcf25.raven_libc_stdio.57bd77f630c2f050-cgu.0.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCsa54BgosZjhp_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsa54BgosZjhp_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsa54BgosZjhp_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter12pad_integral
* P0 aarch64 static rust: `R-30` 28 member(s) above 4096 bytes are not in size-allowlist.toml: core-62d2c2da6b64b23f.core.756df07a42d278dd-cgu.0.rcgu.o=184911, raven_libc_platform-a7171b327b33845d.raven_libc_platform.4bc69655bcb38caf-cgu.00.rcgu.o=46711, alloc-178a59339fc63ec5.alloc.2a577af655833e63-cgu.0.rcgu.o=33479, raven_libc_ctype-656b516d27b2bd40.raven_libc_ctype.b42a263b23215127-cgu.44.rcgu.o=33304, raven_libc_regex-ea5568bbffdc7483.raven_libc_regex.d3e37f2bb6c85e97-cgu.00.rcgu.o=30115, raven_libc-ba232b601075ec5c.raven_libc.72baab98db11f276-cgu.000.rcgu.o=21269, raven_libc_regex-ea5568bbffdc7483.raven_libc_regex.d3e37f2bb6c85e97-cgu.01.rcgu.o=19201, raven_libc_dtoa-45c2d948b6da0a2c.raven_libc_dtoa.60e0ab97be07768c-cgu.00.rcgu.o=16069, raven_libc_platform-a7171b327b33845d.raven_libc_platform.4bc69655bcb38caf-cgu.01.rcgu.o=12929, raven_libc_dtoa-45c2d948b6da0a2c.raven_libc_dtoa.60e0ab97be07768c-cgu.01.rcgu.o=10496, raven_libc-ba232b601075ec5c.raven_libc.72baab98db11f276-cgu.001.rcgu.o=9970, raven_libc-ba232b601075ec5c.raven_libc.72baab98db11f276-cgu.002.rcgu.o=9908, raven_libc_dtoa-45c2d948b6da0a2c.raven_libc_dtoa.60e0ab97be07768c-cgu.02.rcgu.o=9187, raven_libc_dtoa-45c2d948b6da0a2c.raven_libc_dtoa.60e0ab97be07768c-cgu.03.rcgu.o=9042, raven_libc_glob-d9910aa5f2d6b571.raven_libc_glob.ff501857952b2d36-cgu.00.rcgu.o=7328, raven_libc-ba232b601075ec5c.raven_libc.72baab98db11f276-cgu.004.rcgu.o=6773, raven_libc_dtoa-45c2d948b6da0a2c.raven_libc_dtoa.60e0ab97be07768c-cgu.04.rcgu.o=6676, raven_libc_str-7f74cce893583411.raven_libc_str.48d10c9d0e699033-cgu.05.rcgu.o=6499, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.006.rcgu.o=6332, raven_libc_platform-a7171b327b33845d.raven_libc_platform.4bc69655bcb38caf-cgu.04.rcgu.o=5400, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.013.rcgu.o=5340, raven_libc-ba232b601075ec5c.raven_libc.72baab98db11f276-cgu.003.rcgu.o=4955, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.075.rcgu.o=4948, raven_libc_dtoa-45c2d948b6da0a2c.raven_libc_dtoa.60e0ab97be07768c-cgu.05.rcgu.o=4684, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.019.rcgu.o=4680, compiler_builtins-6fa23885a00a91d5.compiler_builtins.bff0b50a038f5711-cgu.016.rcgu.o=4280, raven_libc-ba232b601075ec5c.raven_libc.72baab98db11f276-cgu.008.rcgu.o=4244, raven_libc-ba232b601075ec5c.raven_libc.72baab98db11f276-cgu.005.rcgu.o=4223
* P1 aarch64 static rust: `P1-R-15` member `stdio*` pulled: raven_libc_stdio-10e7df5389dbcf25.raven_libc_stdio.57bd77f630c2f050-cgu.0.rcgu.o
* P0 aarch64 static c: `P0-R-9` member `stdio*` pulled: raven_libc_stdio-10e7df5389dbcf25.raven_libc_stdio.57bd77f630c2f050-cgu.0.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCsa54BgosZjhp_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsa54BgosZjhp_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsa54BgosZjhp_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsa54BgosZjhp_4core3fmtNtB5_9Formatter12pad_integral
* P1 aarch64 static c: `P1-R-15` member `stdio*` pulled: raven_libc_stdio-10e7df5389dbcf25.raven_libc_stdio.57bd77f630c2f050-cgu.0.rcgu.o
* P0 riscv64 static rust: `P0-R-9` member `malloc*` pulled: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.01.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.04.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.06.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.07.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter12pad_integral
* P0 riscv64 static rust: `R-30` 22 member(s) above 4096 bytes are not in size-allowlist.toml: core-091f8e594b9d17e0.core.a1411766580e0007-cgu.0.rcgu.o=175881, raven_libc_ctype-d617825bb5677d79.raven_libc_ctype.2cb1d4b84ff5ac27-cgu.44.rcgu.o=33304, alloc-474d3a6110973dd4.alloc.b5694f824a9868cc-cgu.0.rcgu.o=30783, raven_libc_platform-99204e105a701c2a.raven_libc_platform.45f1e3e094b8e08d-cgu.00.rcgu.o=29161, raven_libc_regex-506ffaedd1503561.raven_libc_regex.f13d1d0c05e0d5a4-cgu.00.rcgu.o=22739, raven_libc_regex-506ffaedd1503561.raven_libc_regex.f13d1d0c05e0d5a4-cgu.01.rcgu.o=14009, raven_libc-ab9dd22f2f6ab9d4.raven_libc.2ec755fbaf62545f-cgu.000.rcgu.o=13932, raven_libc_dtoa-e841d5d14807ac7a.raven_libc_dtoa.a3e306b1ab3c8c6a-cgu.00.rcgu.o=8720, raven_libc_platform-99204e105a701c2a.raven_libc_platform.45f1e3e094b8e08d-cgu.01.rcgu.o=8473, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.020.rcgu.o=7290, raven_libc-ab9dd22f2f6ab9d4.raven_libc.2ec755fbaf62545f-cgu.002.rcgu.o=7265, raven_libc-ab9dd22f2f6ab9d4.raven_libc.2ec755fbaf62545f-cgu.001.rcgu.o=6717, raven_libc_dtoa-e841d5d14807ac7a.raven_libc_dtoa.a3e306b1ab3c8c6a-cgu.03.rcgu.o=5812, raven_libc_dtoa-e841d5d14807ac7a.raven_libc_dtoa.a3e306b1ab3c8c6a-cgu.01.rcgu.o=5799, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.006.rcgu.o=5610, raven_libc_dtoa-e841d5d14807ac7a.raven_libc_dtoa.a3e306b1ab3c8c6a-cgu.02.rcgu.o=5539, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.002.rcgu.o=5478, raven_libc-ab9dd22f2f6ab9d4.raven_libc.2ec755fbaf62545f-cgu.004.rcgu.o=5095, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.075.rcgu.o=4720, raven_libc_glob-f629cc9bcc1213ed.raven_libc_glob.5600be480b2dfffc-cgu.00.rcgu.o=4714, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.001.rcgu.o=4306, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.022.rcgu.o=4242
* P1 riscv64 static rust: `P1-R-15` member `malloc*` pulled: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.01.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.04.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.06.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.07.rcgu.o
* P0 riscv64 static c: `P0-R-9` member `malloc*` pulled: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.01.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.04.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.06.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.07.rcgu.o; symbol `core3fmt*` present: _RNvNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter12pad_integral
* P1 riscv64 static c: `P1-R-15` member `malloc*` pulled: raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.01.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.04.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.06.rcgu.o, raven_libc_malloc-00f535f551d0733d.raven_libc_malloc.72327d32faa6d945-cgu.07.rcgu.o
* P0 riscv64 static rust: `P0-R-9` symbol `core3fmt*` present: _RNvNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter12pad_integral
* P0 riscv64 static rust: `R-30` 23 member(s) above 4096 bytes are not in size-allowlist.toml: core-091f8e594b9d17e0.core.a1411766580e0007-cgu.0.rcgu.o=175881, raven_libc_platform-9e2f31ea7b377ffa.raven_libc_platform.79c3142ea740dee5-cgu.00.rcgu.o=35423, raven_libc_ctype-cd3bdd8ab76c13e1.raven_libc_ctype.61570466d61a6749-cgu.44.rcgu.o=33304, alloc-474d3a6110973dd4.alloc.b5694f824a9868cc-cgu.0.rcgu.o=30783, raven_libc_regex-1eade174ca15ff76.raven_libc_regex.ee5255419c7daffe-cgu.00.rcgu.o=24172, raven_libc-b9caf4b279a6b0c5.raven_libc.ae3e2f299c71b36a-cgu.000.rcgu.o=18391, raven_libc_regex-1eade174ca15ff76.raven_libc_regex.ee5255419c7daffe-cgu.01.rcgu.o=16585, raven_libc_dtoa-46c0a57f0895d537.raven_libc_dtoa.6629f8e921bf5653-cgu.00.rcgu.o=11675, raven_libc_platform-9e2f31ea7b377ffa.raven_libc_platform.79c3142ea740dee5-cgu.01.rcgu.o=9265, raven_libc_dtoa-46c0a57f0895d537.raven_libc_dtoa.6629f8e921bf5653-cgu.01.rcgu.o=8634, raven_libc-b9caf4b279a6b0c5.raven_libc.ae3e2f299c71b36a-cgu.001.rcgu.o=7774, raven_libc-b9caf4b279a6b0c5.raven_libc.ae3e2f299c71b36a-cgu.002.rcgu.o=7531, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.020.rcgu.o=7290, raven_libc_dtoa-46c0a57f0895d537.raven_libc_dtoa.6629f8e921bf5653-cgu.03.rcgu.o=7028, raven_libc_dtoa-46c0a57f0895d537.raven_libc_dtoa.6629f8e921bf5653-cgu.02.rcgu.o=6877, raven_libc_glob-ab2387711ef7cfb7.raven_libc_glob.cdf7d2d35da589b-cgu.00.rcgu.o=5678, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.006.rcgu.o=5610, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.002.rcgu.o=5478, raven_libc-b9caf4b279a6b0c5.raven_libc.ae3e2f299c71b36a-cgu.004.rcgu.o=5413, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.075.rcgu.o=4720, raven_libc_dtoa-46c0a57f0895d537.raven_libc_dtoa.6629f8e921bf5653-cgu.04.rcgu.o=4578, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.001.rcgu.o=4306, compiler_builtins-cf0f135b0cd53105.compiler_builtins.9b573503339ea1e8-cgu.022.rcgu.o=4242
* P0 riscv64 static c: `P0-R-9` symbol `core3fmt*` present: _RNvNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB7_9Formatter12pad_integral12write_prefix, _RNvXsd_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impyNtB9_7Display3fmt, _RNvXsi_NtNtNtCsdQlRCGBlRAh_4core3fmt3num3impjNtB9_7Display3fmt, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter3pad, _RNvMsa_NtCsdQlRCGBlRAh_4core3fmtNtB5_9Formatter12pad_integral

## Hard-invariant failures

* P0 x86_64 static rust: `INV-B-P0` measured 5768 > 512
* P2 x86_64 static rust: `INV-B-P2` measured 9496 > 4096
* P2 x86_64 static rust: `INV-S-P2-P0` measured 35992 > 5120
* P2 x86_64 static rust: `R-13` measured 10 > 0
* P0 x86_64 static c: `INV-B-P0` measured 5776 > 512
* P2 x86_64 static c: `INV-B-P2` measured 9496 > 4096
* P2 x86_64 static c: `INV-S-P2-P0` measured 35984 > 5120
* P2 x86_64 static c: `R-13` measured 10 > 0
* P0 x86_64 static rust: `INV-B-P0` measured 6384 > 512
* P2 x86_64 static rust: `INV-B-P2` measured 6936 > 4096
* P2 x86_64 static rust: `INV-S-P2-P0` measured 39169 > 5120
* P2 x86_64 static rust: `R-13` measured 9 > 0
* P0 x86_64 static c: `INV-B-P0` measured 6376 > 512
* P2 x86_64 static c: `INV-B-P2` measured 6928 > 4096
* P2 x86_64 static c: `INV-S-P2-P0` measured 39158 > 5120
* P2 x86_64 static c: `R-13` measured 9 > 0
* P0 aarch64 static rust: `INV-B-P0` measured 8160 > 512
* P0 aarch64 static c: `INV-B-P0` measured 6152 > 512
* P0 aarch64 static rust: `INV-B-P0` measured 6680 > 512
* P0 aarch64 static c: `INV-B-P0` measured 7800 > 512
* P0 riscv64 static rust: `INV-B-P0` measured 9256 > 512
* P0 riscv64 static c: `INV-B-P0` measured 9176 > 512
* P0 riscv64 static rust: `INV-B-P0` measured 7768 > 512
* P0 riscv64 static c: `INV-B-P0` measured 7688 > 512

## Top symbols: P2 x86_64 tiny rust

* `raven_libc_fmt::printf::vformat::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>, raven_libc::stdio::vprintf::FloatHook>=4652`
* `raven_libc_sys::pages::REGISTRY=3080`
* `anon.0943ef241966b6e839b8deb1dc755758.1.llvm.17620530845122684329=2903`
* `raven_libc::stdio::streams::STREAMS=2808`
* `raven_libc_fmt::printf::float_layout::emit_inner::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::stdio::vprintf::FloatHook>=2385`
* `raven_libc::ffi::platform::PLATFORM (.llvm.1623428538001991915)=2336`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::openat=2134`
* `raven_libc_fmt::printf::pos::analyse::<u8>=1297`
* `raven_libc_platform::path::resolve::resolve=1111`
* `core::str::count::do_count_chars=948`
* `raven_libc_fmt::printf::emit_common::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=942`
* `<raven_libc_stdio::mem::MemCookie>::write_cookie::<raven_libc::ffi::env::ShellEnv> (.llvm.650956290432286061)=848`
* `<core::fmt::Formatter>::pad_integral=791`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::fcntl=678`
* `<core::fmt::Formatter>::pad=649`
* `raven_libc_platform::path::special::special_fd=643`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::root_call (.llvm.15524546224668414644)=606`
* `raven_libc_fmt::printf::str::emit_mb_wstr::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=549`
* `<raven_libc_malloc::tiny::Tiny<raven_libc::malloc::heap::Pages, raven_libc::malloc::heap::Spin, raven_libc::malloc::heap::Hooks>>::malloc_chunk (.llvm.11824565903476581360)=545`
* `__raven_libc_start=521`

## Top symbols: P2 x86_64 server rust

* `raven_libc_fmt::printf::vformat::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>, raven_libc::stdio::vprintf::FloatHook>=6485`
* `raven_libc_sys::pages::REGISTRY=3080`
* `anon.1207d017910d04a7fed8e48a21ac277a.1.llvm.16926217209641037306=2903`
* `raven_libc_fmt::printf::float_layout::emit_inner::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::stdio::vprintf::FloatHook>=2832`
* `raven_libc::stdio::streams::STREAMS=2808`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::openat=2366`
* `raven_libc::ffi::platform::PLATFORM (.llvm.13736572854360460066)=2336`
* `raven_libc_platform::path::resolve::resolve=1583`
* `raven_libc_fmt::printf::pos::analyse::<u8>=1507`
* `raven_libc_fmt::printf::emit_common::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=1303`
* `core::str::count::do_count_chars=948`
* `<raven_libc_stdio::mem::MemCookie>::write_cookie::<raven_libc::ffi::env::ShellEnv> (.llvm.7647518359455344107)=883`
* `<core::fmt::Formatter>::pad_integral=791`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::fcntl=702`
* `raven_libc_platform::path::special::special_fd=669`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::root_call (.llvm.16368304533426694555)=666`
* `<core::fmt::Formatter>::pad=649`
* `raven_libc_fmt::printf::int::ujtoa=587`
* `raven_libc_fmt::printf::str::emit_mb_wstr::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=576`
* `__raven_libc_start=534`

## Top symbols: P2 x86_64 tiny c

* `raven_libc_fmt::printf::vformat::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>, raven_libc::stdio::vprintf::FloatHook>=4652`
* `raven_libc_sys::pages::REGISTRY=3080`
* `anon.0943ef241966b6e839b8deb1dc755758.1.llvm.17620530845122684329=2903`
* `raven_libc::stdio::streams::STREAMS=2808`
* `raven_libc_fmt::printf::float_layout::emit_inner::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::stdio::vprintf::FloatHook>=2385`
* `raven_libc::ffi::platform::PLATFORM (.llvm.1623428538001991915)=2336`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::openat=2134`
* `raven_libc_fmt::printf::pos::analyse::<u8>=1297`
* `raven_libc_platform::path::resolve::resolve=1111`
* `core::str::count::do_count_chars=948`
* `raven_libc_fmt::printf::emit_common::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=942`
* `<raven_libc_stdio::mem::MemCookie>::write_cookie::<raven_libc::ffi::env::ShellEnv> (.llvm.650956290432286061)=848`
* `<core::fmt::Formatter>::pad_integral=791`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::fcntl=678`
* `<core::fmt::Formatter>::pad=649`
* `raven_libc_platform::path::special::special_fd=643`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::root_call (.llvm.15524546224668414644)=606`
* `raven_libc_fmt::printf::str::emit_mb_wstr::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=549`
* `<raven_libc_malloc::tiny::Tiny<raven_libc::malloc::heap::Pages, raven_libc::malloc::heap::Spin, raven_libc::malloc::heap::Hooks>>::malloc_chunk (.llvm.11824565903476581360)=545`
* `__raven_libc_start=521`

## Top symbols: P2 x86_64 server c

* `raven_libc_fmt::printf::vformat::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>, raven_libc::stdio::vprintf::FloatHook>=6485`
* `raven_libc_sys::pages::REGISTRY=3080`
* `anon.1207d017910d04a7fed8e48a21ac277a.1.llvm.16926217209641037306=2903`
* `raven_libc_fmt::printf::float_layout::emit_inner::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::stdio::vprintf::FloatHook>=2832`
* `raven_libc::stdio::streams::STREAMS=2808`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::openat=2366`
* `raven_libc::ffi::platform::PLATFORM (.llvm.13736572854360460066)=2336`
* `raven_libc_platform::path::resolve::resolve=1583`
* `raven_libc_fmt::printf::pos::analyse::<u8>=1507`
* `raven_libc_fmt::printf::emit_common::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=1303`
* `core::str::count::do_count_chars=948`
* `<raven_libc_stdio::mem::MemCookie>::write_cookie::<raven_libc::ffi::env::ShellEnv> (.llvm.7647518359455344107)=883`
* `<core::fmt::Formatter>::pad_integral=791`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::fcntl=702`
* `raven_libc_platform::path::special::special_fd=669`
* `<raven_libc_platform::platform::Platform<raven_libc_platform::backend::RealSys, raven_libc::ffi::platform::ShellLock>>::root_call (.llvm.16368304533426694555)=666`
* `<core::fmt::Formatter>::pad=649`
* `raven_libc_fmt::printf::int::ujtoa=587`
* `raven_libc_fmt::printf::str::emit_mb_wstr::<u8, raven_libc_stdio::glue_io::FileSink<raven_libc::ffi::env::ShellEnv>, raven_libc::va::env::CEnv<raven_libc::va::sysv_x86_64::SysvReader>>=576`
* `__raven_libc_start=534`

## Skipped artifacts

* P0 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P0 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P0 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P0 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P0 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P0 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P1 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P1 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P1 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P1 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P1 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P1 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P2 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P2 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P2 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P2 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P2 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P2 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P3 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P3 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P3 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P3 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P3 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P3 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P4 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P4 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P4 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P4 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P4 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P4 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P5 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P5 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P5 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P5 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P5 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P5 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P6 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P6 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P6 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P6 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P6 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P6 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P7 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P7 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P7 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P7 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P7 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P7 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P8 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P8 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P8 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P8 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P8 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P8 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P9 aarch64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P9 aarch64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/aarch64/libc.so.1`)
* P9 riscv64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P9 riscv64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/riscv64/libc.so.1`)
* P9 x86_64 dynamic c: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)
* P9 x86_64 dynamic rust: `skipped: libc.so.1 absent (C62 not landed)` (`build/x86_64/libc.so.1`)

