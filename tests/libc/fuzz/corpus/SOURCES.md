<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# Corpus sources

All donor files are read for **inputs only**; no code or comments are copied. The revision is
the checkout in `ravenbsd-resources/donors.lock`.

| target | seeds | donor source | licence as read | notes |
|---|---|---|---|---|
| printf_format | seed-001..seed-016 | `netbsd/tests/lib/libc/stdio/t_printf.c`; `picolibc/test/test-stdio/test-printf-testcases.h` | BSD-2 | format strings only; seedgen adds the 384-byte hostile environment prefix |
| scanf_format | seed-001..seed-014 | `netbsd/tests/lib/libc/stdio/t_scanf.c`; `picolibc/test/libc-testsuite/sscanf.c` | BSD-2 | format/input pairs |
| strtod_input | seed-001..seed-014 | `netbsd/tests/lib/libc/stdlib/t_strtod.c`; `openbsd/regress/lib/libc/strtod/strtodtest.c`; `picolibc` strtod tests | BSD-2 / ISC | numeric strings |
| strtol_input | seed-001..seed-014 | `netbsd/tests/lib/libc/stdlib/t_strtol.c`; `openbsd/regress/lib/libc/getopt/getopt-test.c` | BSD-2 / ISC | base/sign/input triples |
| time_format | synthetic (seedgen) | derived from the C44 tests (`t_strptime.c`, `picolibc/test-strftime.c`) | BSD-2 | `Tm` fields, zone selector, format/subject |
| tz_parse | seed-001..seed-012 + synthetic | `netbsd/tests/lib/libc/time/t_strptime.c`; synthetic TZif v1/v2 | BSD-2 | POSIX strings and TZif images |
| regex_compile_exec | seed-001..seed-014 + `netbsd/tests/lib/libc/regex/data/*.in` | NetBSD Spencer data; `openbsd/regress/lib/libc/regex/tests` | Spencer/BSD | patterns and subjects |
| glob_pattern | seed-001..seed-012 | `netbsd/tests/lib/libc/gen/t_glob.c`; `t_fnmatch.c` | BSD-2 | patterns and entry lists |
| resolver_message | seed-001..seed-005 + synthetic | `netbsd/tests/lib/libc/net/h_dns_server.c`; RFC 1035 examples | BSD-2 | query/answer/compression packets |
| resolver_conf | seed-001..seed-013 | `netbsd/tests/lib/libc/net/hosts` and `resolv.conf`; C51 tests | BSD-2 / BSD-3 | file bodies |
| elf_dynamic | seed-001..seed-004 + synthetic | C52 tests; synthetic ET_DYN builder | BSD-3 | minimal PT_LOAD/PT_DYNAMIC/DT_HASH images and truncations |
| utf8_decode | seed-001..seed-012 | `netbsd/tests/lib/libc/locale/t_mbrtowc.c` | BSD-2 | byte strings |
| inet_text | seed-001..seed-012 | `netbsd/tests/lib/libc/inet/t_inet_addr.c`; `t_inet_network.c` | BSD-2 | address strings |
| getopt_args | seed-001..seed-010 | `netbsd/tests/lib/libc/stdlib/h_getopt.c`; `h_getopt_long.c`; OpenBSD `getopt-test.c` | BSD-2 / ISC | optstrings and argv |
