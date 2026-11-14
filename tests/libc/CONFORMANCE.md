<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc donor-suite conformance

Nothing is deleted from this table: a failing body stays listed until it passes (rule of C20).

| Suite | bodies | programs | target-builds | oracle-pass | target-pass | fail | skip | excluded | oracle-fail | not-run | rate |
|---|---|---|---|---|---|---|---|---|---|---|---|
| freebsd-libc | 139 | 61 | 0 | 51 | 0 | 0 | 0 | 22 | 66 | 51 | 0.0 % |
| freebsd-msun | 86 | 47 | 0 | 41 | 0 | 0 | 0 | 26 | 19 | 41 | 0.0 % |
| netbsd-libc | 498 | 280 | 0 | 247 | 0 | 0 | 39 | 39 | 173 | 247 | 0.0 % |
| netbsd-libcrypt | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0.0 % |
| netbsd-libm | 303 | 32 | 0 | 290 | 0 | 0 | 1 | 0 | 12 | 290 | 0.0 % |
| netbsd-libposix | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0 % |
| netbsd-libpthread | 48 | 39 | 0 | 13 | 0 | 0 | 5 | 3 | 27 | 13 | 0.0 % |
| openbsd-libc | 47 | 47 | 0 | 15 | 0 | 0 | 0 | 1 | 31 | 15 | 0.0 % |
| picolibc | 33 | 33 | 0 | 18 | 0 | 0 | 0 | 3 | 12 | 18 | 0.0 % |

#### freebsd-libc: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| stdio/clearerr_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/clearerr_test.c | - |
| stdio/eintr_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/eintr_test.c | - |
| stdio/fdopen_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/fdopen_test.c:170:28: error: ‘O_EXEC’ undeclared (first use in this function); did you mean ‘O_EXCL’? | - |
| stdio/fflush_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/fflush_test.c | - |
| stdio/flushlbuf_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:fwopen | - |
| stdio/fmemopen2_test | test_append_binary_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/fmemopen2_test | test_autoalloc | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/fmemopen2_test.c:126: nofw == 0 not met | - |
| stdio/fmemopen2_test | test_binary | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/fmemopen2_test.c:233: buf[i] != '\0' not met | - |
| stdio/fmemopen2_test | test_data_length | oracle-fail | oracle-behaviour:no result file (signal 6) | - |
| stdio/fmemopen2_test | test_preexisting | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/fmemopen2_test.c:91: nofw == 4 not met | - |
| stdio/fmemopen2_test | test_rdonly_wronly | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/fmemopen2_test | test_size_0 | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/fmemopen2_test.c:272: fp == NULL not met | - |
| stdio/fmemopen_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/fmemopen_test.c | - |
| stdio/fopen2_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/fopen_test.c:38:40: error: ‘O_EXEC’ undeclared (first use in this function); did you mean ‘O_EXCL’? | - |
| stdio/fopen_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/fopen_test.c:38:40: error: ‘O_EXEC’ undeclared (first use in this function); did you mean ‘O_EXCL’? | - |
| stdio/fputc_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/fputc_test.c | - |
| stdio/freopen_test | null__r__a__test | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/freopen_test.c:53: freopen("<NULL>", "a", fopen("/dev/null", "r")) failed: 0 | - |
| stdio/freopen_test | null__r__r__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__r__r_append__test | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/freopen_test.c:53: freopen("<NULL>", "r+", fopen("/dev/null", "r")) failed: 0 | - |
| stdio/freopen_test | null__r__w__test | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/freopen_test.c:53: freopen("<NULL>", "w", fopen("/dev/null", "r")) failed: 0 | - |
| stdio/freopen_test | null__r__w_append__test | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/freopen_test.c:53: freopen("<NULL>", "w+", fopen("/dev/null", "r")) failed: 0 | - |
| stdio/freopen_test | null__r_append__a__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__r_append__r__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__r_append__r_append__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__r_append__w__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__r_append__w_append__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__w__a__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__w__r__test | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/freopen_test.c:53: freopen("<NULL>", "r", fopen("/dev/null", "w")) failed: 0 | - |
| stdio/freopen_test | null__w__r_append__test | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/freopen_test.c:53: freopen("<NULL>", "r+", fopen("/dev/null", "w")) failed: 0 | - |
| stdio/freopen_test | null__w__w__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | null__w__w_append__test | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/freopen_test.c:53: freopen("<NULL>", "w+", fopen("/dev/null", "w")) failed: 0 | - |
| stdio/freopen_test | sh__null__r__r__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | sh__null__r__w__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | sh__r__r__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/freopen_test | sh__sh__r__r__test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/getdelim_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:fropen | - |
| stdio/gets_s_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/gets_s_test.c:36:8: error: unknown type name ‘errno_t’ | - |
| stdio/mkostemp_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/mkostemp_test.c:46:42: error: ‘O_SHLOCK’ undeclared (first use in this function); did you mean ‘F_SHLCK’? | - |
| stdio/mktemp_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/mktemp_test.c | - |
| stdio/open_memstream2_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/open_memstream2_test.c:185:17: error: ‘OFF_MAX’ undeclared (first use in this function) | - |
| stdio/open_memstream_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/open_memstream_test.c | - |
| stdio/open_wmemstream_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/open_wmemstream_test.c:185:17: error: ‘OFF_MAX’ undeclared (first use in this function) | - |
| stdio/perror_test | perror_test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/popen_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/popen_test.c | - |
| stdio/print_positional_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| stdio/printbasic_test | int_limits | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printbasic_test | int_within_limits | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printf_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/printf_test.c | - |
| stdio/printfloat_test | alternate_form | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | decimal_rounding | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | float_within_limits | oracle-fail | oracle-behaviour:2 checks failed; see output for more details | - |
| stdio/printfloat_test | hexadecimal_floating_point | oracle-fail | oracle-behaviour:10 checks failed; see output for more details | - |
| stdio/printfloat_test | hexadecimal_rounding | oracle-fail | oracle-behaviour:4 checks failed; see output for more details | - |
| stdio/printfloat_test | hexadecimal_rounding_fullprec | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | infinities_and_nans | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | padding | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | padding_and_decimal_point_placement | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | precision_specifiers | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | signed_conversions | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | subnormal_double | oracle-fail | oracle-behaviour:8 checks failed; see output for more details | - |
| stdio/printfloat_test | subnormal_float | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/printfloat_test | thousands_separator_and_other_locale_tests | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/printfloat_test.c:176: setlocale(LC_NUMERIC, "hi_IN.ISCII-DEV") not met | - |
| stdio/scanf_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdio/scanf_test.c | - |
| stdio/scanfloat_test | infinities_and_nans | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/scanfloat_test.c:175: strcmp(buf, "plugh") == 0 not met | - |
| stdio/scanfloat_test | normalized_numbers | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdio/scanfloat_test.c:90: 2 != sscanf("-1.23e", "%e%s", &f, buf) | - |
| stdio/scanfloat_test | rounding_tests | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/scanfloat_test | strtod | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/snprintf_test | snprintf_B | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/snprintf_test | snprintf_X | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/snprintf_test | snprintf_b | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/snprintf_test | snprintf_d | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/snprintf_test | snprintf_wN | oracle-fail | oracle-behaviour:43 checks failed; see output for more details | - |
| stdio/snprintf_test | snprintf_wfN | oracle-fail | oracle-behaviour:46 checks failed; see output for more details | - |
| stdio/snprintf_test | snprintf_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/sscanf_test | sscanf_b | oracle-fail | oracle-behaviour:366 checks failed; see output for more details | - |
| stdio/sscanf_test | sscanf_d | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/sscanf_test | sscanf_i | oracle-fail | oracle-behaviour:21 checks failed; see output for more details | - |
| stdio/sscanf_test | sscanf_o | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/sscanf_test | sscanf_termination | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/sscanf_test | sscanf_wN | oracle-fail | oracle-behaviour:32 checks failed; see output for more details | - |
| stdio/sscanf_test | sscanf_wfN | oracle-fail | oracle-behaviour:32 checks failed; see output for more details | - |
| stdio/sscanf_test | sscanf_x | oracle-fail | oracle-behaviour:12 checks failed; see output for more details | - |
| stdio/swprintf_test | swprintf_B | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swprintf_test | swprintf_X | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swprintf_test | swprintf_b | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swprintf_test | swprintf_d | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swprintf_test | swprintf_wN | oracle-fail | oracle-behaviour:43 checks failed; see output for more details | - |
| stdio/swprintf_test | swprintf_wfN | oracle-fail | oracle-behaviour:46 checks failed; see output for more details | - |
| stdio/swprintf_test | swprintf_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swscanf_test | swscanf_b | oracle-fail | oracle-behaviour:366 checks failed; see output for more details | - |
| stdio/swscanf_test | swscanf_d | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swscanf_test | swscanf_i | oracle-fail | oracle-behaviour:21 checks failed; see output for more details | - |
| stdio/swscanf_test | swscanf_o | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swscanf_test | swscanf_termination | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/swscanf_test | swscanf_wN | oracle-fail | oracle-behaviour:24 checks failed; see output for more details | - |
| stdio/swscanf_test | swscanf_wfN | oracle-fail | oracle-behaviour:24 checks failed; see output for more details | - |
| stdio/swscanf_test | swscanf_x | oracle-fail | oracle-behaviour:12 checks failed; see output for more details | - |
| stdlib/abs_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/abs_test.c | - |
| stdlib/atoi_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/atoi_test.c | - |
| stdlib/bsearch_b_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): ATF_TESTS_C inside .if/.for | - |
| stdlib/bsearch_s_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/bsearch_s_test.c:18:8: error: unknown type name ‘errno_t’ | - |
| stdlib/bsearch_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| stdlib/clearenv_test | clearenv__double_clear_vars | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/clearenv_test | clearenv__environ_null | oracle-fail | oracle-behaviour:1 checks failed; see output for more details | - |
| stdlib/clearenv_test | clearenv__multiple_vars_test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/clearenv_test | clearenv__putenv_vars | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/clearenv_test | clearenv__recreated_system_var_test | oracle-fail | oracle-behaviour:1 checks failed; see output for more details | - |
| stdlib/clearenv_test | clearenv__recreated_vars_test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/clearenv_test | clearenv__single_var_test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/clearenv_test | clearenv__system_var_test | oracle-fail | oracle-behaviour:1 checks failed; see output for more details | - |
| stdlib/cxa_atexit_test | late_cxa_atexit | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/cxa_atexit_test.c:41: dlopen: /home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/libc-conformance/work/freebsd-libc/stdlib/cxa_atexit_test/late_cxa_atexit/srcdir/libatexit.so: cannot open shared object file: No such file or directory | - |
| stdlib/cxa_atexit_test | late_cxa_atexit_ran | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/cxa_atexit_test.c:41: dlopen: /home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/libc-conformance/work/freebsd-libc/stdlib/cxa_atexit_test/late_cxa_atexit_ran/srcdir/libatexit.so: cannot open shared object file: No such file or directory | - |
| stdlib/cxa_atexit_test | simple_cxa_atexit | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/cxa_atexit_test.c:41: dlopen: /home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/libc-conformance/work/freebsd-libc/stdlib/cxa_atexit_test/simple_cxa_atexit/srcdir/libatexit.so: cannot open shared object file: No such file or directory | - |
| stdlib/div_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/div_test.c | - |
| stdlib/dynthr_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/dynthr_test.c:68:9: error: unknown type name ‘dlfunc_t’; did you mean ‘modfunc_t’? | - |
| stdlib/exit_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/exit_test.c | - |
| stdlib/getenv_r_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:getenv_r | - |
| stdlib/getenv_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/getenv_test.c | - |
| stdlib/heapsort_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:heapsort | - |
| stdlib/hsearch_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/hsearch_test.c | - |
| stdlib/libc_exit_test | quick_exit | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/libc_exit_test | recursive_exit1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/libc_exit_test | recursive_exit2 | oracle-fail | oracle-behaviour:1 checks failed; see output for more details | - |
| stdlib/mergesort_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:mergesort | - |
| stdlib/posix_memalign_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/posix_memalign_test.c | - |
| stdlib/qsort_b_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): ATF_TESTS_C inside .if/.for | - |
| stdlib/qsort_bench | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/qsort_bench.c:40:8: error: unknown type name ‘uintmax_t’ | - |
| stdlib/qsort_r_compat_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| stdlib/qsort_r_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| stdlib/qsort_s_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/qsort_s_test.c:41:8: error: unknown type name ‘errno_t’ | - |
| stdlib/qsort_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| stdlib/random_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/random_test.c | - |
| stdlib/set_constraint_handler_s_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/set_constraint_handler_s_test.c:35:41: error: ‘abort_handler_s’ undeclared (first use in this function) | - |
| stdlib/strfmon_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| stdlib/strfrom_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/strfrom_test.c:479:32: error: ‘EDOOFUS’ undeclared (first use in this function); did you mean ‘EROFS’? | - |
| stdlib/strtod_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/strtod_test.c | - |
| stdlib/strtol_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/strtol_test.c | - |
| stdlib/system_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/system_test.c:119:49: error: ‘O_EXLOCK’ undeclared (first use in this function); did you mean ‘F_EXLCK’? | - |
| stdlib/tsearch_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/libc/stdlib/tsearch_test.c:38:20: error: unknown type name ‘posix_tnode’ | - |

Programs of freebsd-libc:

* stdio/clearerr_test: 0 pass
* stdio/eintr_test: 0 pass
* stdio/fdopen_test: 0 pass
* stdio/fflush_test: 0 pass
* stdio/flushlbuf_test: 0 pass
* stdio/fmemopen2_test: 0 pass
* stdio/fmemopen_test: 0 pass
* stdio/fopen2_test: 0 pass
* stdio/fopen_test: 0 pass
* stdio/fputc_test: 0 pass
* stdio/freopen_test: 0 pass
* stdio/getdelim_test: 0 pass
* stdio/gets_s_test: 0 pass
* stdio/mkostemp_test: 0 pass
* stdio/mktemp_test: 0 pass
* stdio/open_memstream2_test: 0 pass
* stdio/open_memstream_test: 0 pass
* stdio/open_wmemstream_test: 0 pass
* stdio/perror_test: 0 pass
* stdio/popen_test: 0 pass
* stdio/print_positional_test: 0 pass
* stdio/printbasic_test: 0 pass
* stdio/printf_test: 0 pass
* stdio/printfloat_test: 0 pass
* stdio/scanf_test: 0 pass
* stdio/scanfloat_test: 0 pass
* stdio/snprintf_test: 0 pass
* stdio/sscanf_test: 0 pass
* stdio/swprintf_test: 0 pass
* stdio/swscanf_test: 0 pass
* stdlib/abs_test: 0 pass
* stdlib/atoi_test: 0 pass
* stdlib/bsearch_b_test: 0 pass
* stdlib/bsearch_s_test: 0 pass
* stdlib/bsearch_test: 0 pass
* stdlib/clearenv_test: 0 pass
* stdlib/cxa_atexit_test: 0 pass
* stdlib/div_test: 0 pass
* stdlib/dynthr_test: 0 pass
* stdlib/exit_test: 0 pass
* stdlib/getenv_r_test: 0 pass
* stdlib/getenv_test: 0 pass
* stdlib/heapsort_test: 0 pass
* stdlib/hsearch_test: 0 pass
* stdlib/libc_exit_test: 0 pass
* stdlib/mergesort_test: 0 pass
* stdlib/posix_memalign_test: 0 pass
* stdlib/qsort_b_test: 0 pass
* stdlib/qsort_bench: 0 pass
* stdlib/qsort_r_compat_test: 0 pass
* stdlib/qsort_r_test: 0 pass
* stdlib/qsort_s_test: 0 pass
* stdlib/qsort_test: 0 pass
* stdlib/random_test: 0 pass
* stdlib/set_constraint_handler_s_test: 0 pass
* stdlib/strfmon_test: 0 pass
* stdlib/strfrom_test: 0 pass
* stdlib/strtod_test: 0 pass
* stdlib/strtol_test: 0 pass
* stdlib/system_test: 0 pass
* stdlib/tsearch_test: 0 pass

#### freebsd-msun: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| acos_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: acos_test.c | - |
| asin_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: asin_test.c | - |
| atan_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: atan_test.c | - |
| bit_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: bit_test.c | - |
| casinh_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: casinh_test.c | - |
| cbrt_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: cbrt_test.c | - |
| ceil_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: ceil_test.c | - |
| cexp_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| conj_test | main | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| cos_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: cos_test.c | - |
| cosh_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: cosh_test.c | - |
| csqrt_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/msun/csqrt_test.c:258:17: error: unknown type name ‘uint64_t’ | - |
| ctrig_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:__XSTRING | - |
| erf_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: erf_test.c | - |
| exp_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: exp_test.c | - |
| exponential_test | unknown-bodies | oracle-fail | oracle-build:build-other:<command-line>: error: token "{" is not valid in preprocessor expressions | - |
| fe_round_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: fe_round_test.c | - |
| fenv_test | unknown-bodies | oracle-fail | oracle-build:missing-header:libutil.h | - |
| fma_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| fmaximum_fminimum_test | test1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test10 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test11 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test12 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test13 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test14 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test15 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test3 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test4 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test5 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test6 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test7 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test8 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaximum_fminimum_test | test9 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test10 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test11 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test12 | oracle-fail | oracle-behaviour:24 checks failed; see output for more details | - |
| fmaxmin_test | test2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test3 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test4 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test5 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test6 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test7 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test8 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmaxmin_test | test9 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fmod_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: fmod_test.c | - |
| hypot_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: hypot_test.c | - |
| ilogb2_test | ilogb | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| ilogb2_test | ilogbf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| ilogb2_test | ilogbl | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| ilogb_test | ilogb | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| ilogb_test | ilogbf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| ilogb_test | ilogbl | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| infinity_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: infinity_test.c | - |
| invctrig_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| invtrig_test | accuracy | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/msun/invtrig_test.c:247: (((0x20))) != fetestexcept((0x04 \| 0x20 \| 0x01 \| 0x08 \| 0x10)): unexpected exception flags: got 0 not 0x20 for asin((double)((1.0))) | - |
| invtrig_test | atan_huge | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/msun/invtrig_test.c:348: (((0x20))) != fetestexcept((0x04 \| 0x20 \| 0x01 \| 0x08 \| 0x10)): unexpected exception flags: got 0 not 0x20 for atan((double)((huge))) | - |
| invtrig_test | inverse | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| invtrig_test | p2x_atan2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| invtrig_test | special | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/msun/invtrig_test.c:132: (((0x20))) != fetestexcept((0x04 \| 0x20 \| 0x01 \| 0x08 \| 0x10)): unexpected exception flags: got 0 not 0x20 for acos((double)((0.0))) | - |
| invtrig_test | special_atan2 | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/msun/invtrig_test.c:160: (((0x20))) != fetestexcept((0x04 \| 0x20 \| 0x01 \| 0x08 \| 0x10)): unexpected exception flags: got 0 not 0x20 for atan2((double)((-0.0))) | - |
| invtrig_test | tiny | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/freebsd/msun/invtrig_test.c:313: (((0x20))) != fetestexcept((0x04 \| 0x20 \| 0x01 \| 0x08 \| 0x10)): unexpected exception flags: got 0 not 0x20 for asin((double)((tiny))) | - |
| ldexp_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: ldexp_test.c | - |
| log_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: log_test.c | - |
| logarithm_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| lrint_test | lrint | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| lround_test | main | oracle-fail | oracle-behaviour:48 checks failed; see output for more details | - |
| nan_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| nearbyint_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |
| next_test | boundaries | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| next_test | eq_and_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| next_test | ordinary | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| next_test | overflow | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| next_test | zeros | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| pow_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: pow_test.c | - |
| precision_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: precision_test.c | - |
| rem_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:fls | - |
| round_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: round_test.c | - |
| scalbn_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: scalbn_test.c | - |
| sin_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: sin_test.c | - |
| sinh_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: sinh_test.c | - |
| sqrt_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: sqrt_test.c | - |
| tan_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: tan_test.c | - |
| tanh_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: tanh_test.c | - |
| trig_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:nitems | - |

Programs of freebsd-msun:

* acos_test: 0 pass
* asin_test: 0 pass
* atan_test: 0 pass
* bit_test: 0 pass
* casinh_test: 0 pass
* cbrt_test: 0 pass
* ceil_test: 0 pass
* cexp_test: 0 pass
* conj_test: 0 pass
* cos_test: 0 pass
* cosh_test: 0 pass
* csqrt_test: 0 pass
* ctrig_test: 0 pass
* erf_test: 0 pass
* exp_test: 0 pass
* exponential_test: 0 pass
* fe_round_test: 0 pass
* fenv_test: 0 pass
* fma_test: 0 pass
* fmaximum_fminimum_test: 0 pass
* fmaxmin_test: 0 pass
* fmod_test: 0 pass
* hypot_test: 0 pass
* ilogb2_test: 0 pass
* ilogb_test: 0 pass
* infinity_test: 0 pass
* invctrig_test: 0 pass
* invtrig_test: 0 pass
* ldexp_test: 0 pass
* log_test: 0 pass
* logarithm_test: 0 pass
* lrint_test: 0 pass
* lround_test: 0 pass
* nan_test: 0 pass
* nearbyint_test: 0 pass
* next_test: 0 pass
* pow_test: 0 pass
* precision_test: 0 pass
* rem_test: 0 pass
* round_test: 0 pass
* scalbn_test: 0 pass
* sin_test: 0 pass
* sinh_test: 0 pass
* sqrt_test: 0 pass
* tan_test: 0 pass
* tanh_test: 0 pass
* trig_test: 0 pass

#### netbsd-libc: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| atomic/t___sync_add | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t___sync_and | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t___sync_compare_and_swap | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t___sync_lock | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t___sync_nand | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t___sync_or | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t___sync_sub | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t___sync_xor | unknown-bodies | oracle-fail | oracle-build:missing-header:machine/types.h | - |
| atomic/t_atomic_add | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| atomic/t_atomic_and | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| atomic/t_atomic_cas | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| atomic/t_atomic_dec | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| atomic/t_atomic_inc | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| atomic/t_atomic_or | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| atomic/t_atomic_swap | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| c063/t_faccessat | faccessat_abs | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_abs_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_abs_fddir | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_fderr1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_fderr2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_fderr3 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_faccessat | faccessat_fdlink | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fchmodat | fchmodat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fchmodat | fchmodat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fchmodat | fchmodat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fchmodat | fchmodat_fderr1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fchmodat | fchmodat_fderr2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fchmodat | fchmodat_fderr3 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fchmodat | fchmodat_fdlink | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/c063/t_fchmodat.c:183: fchmodat(dfdlink, BASELINK, 0600, AT_SYMLINK_NOFOLLOW) == 0 not met | - |
| c063/t_fchownat | fchownat_fd | skip | needs-root | C55 |
| c063/t_fchownat | fchownat_fdcwd | skip | needs-root | C55 |
| c063/t_fchownat | fchownat_fdcwderr | skip | needs-root | C55 |
| c063/t_fchownat | fchownat_fderr1 | skip | needs-root | C55 |
| c063/t_fchownat | fchownat_fderr2 | skip | needs-root | C55 |
| c063/t_fchownat | fchownat_fderr3 | skip | needs-root | C55 |
| c063/t_fchownat | fchownat_fdlink | skip | needs-root | C55 |
| c063/t_fexecve | unknown-bodies | excluded | excluded:fexecve does not exist yet (16 3.B.1.1) | - |
| c063/t_fstatat | fstatat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fstatat | fstatat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fstatat | fstatat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fstatat | fstatat_fderr1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fstatat | fstatat_fderr2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fstatat | fstatat_fderr3 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_fstatat | fstatat_fdlink | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_linkat | linkat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_linkat | linkat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_linkat | linkat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_linkat | linkat_fderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_linkat | linkat_fdlink1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_linkat | linkat_fdlink2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkdirat | mkdirat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkdirat | mkdirat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkdirat | mkdirat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkdirat | mkdirat_fderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkfifoat | mkfifoat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkfifoat | mkfifoat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkfifoat | mkfifoat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkfifoat | mkfifoat_fderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mkfifoat | mknodat_s_ififo | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_mknodat | mknodat_fd | skip | needs-root | C55 |
| c063/t_mknodat | mknodat_fdcwd | skip | needs-root | C55 |
| c063/t_mknodat | mknodat_fdcwderr | skip | needs-root | C55 |
| c063/t_mknodat | mknodat_fderr | skip | needs-root | C55 |
| c063/t_o_search | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/c063/t_o_search.c:268:47: error: ‘O_SEARCH’ undeclared (first use in this function) | - |
| c063/t_openat | openat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_openat | openat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_openat | openat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_openat | openat_fderr1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_openat | openat_fderr2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_openat | openat_fderr3 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_readlinkat | readlinkat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_readlinkat | readlinkat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_readlinkat | readlinkat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_readlinkat | readlinkat_fderr1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_readlinkat | readlinkat_fderr2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_renameat | renameat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_renameat | renameat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_renameat | renameat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_renameat | renameat_fderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_symlinkat | symlinkat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_symlinkat | symlinkat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_symlinkat | symlinkat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_symlinkat | symlinkat_fderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_unlinkat | unlinkat_dir | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/c063/t_unlinkat.c:161: errno == EPERM not met | - |
| c063/t_unlinkat | unlinkat_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_unlinkat | unlinkat_fdcwd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_unlinkat | unlinkat_fdcwderr | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_unlinkat | unlinkat_fderr1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_unlinkat | unlinkat_fderr2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_unlinkat | unlinkat_fderr3 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| c063/t_utimensat | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| db/t_db.sh | shell-test | skip | needs sh (O-B2) | C65 |
| db/t_db_hash_seq | unknown-bodies | oracle-fail | oracle-build:missing-header:db.h | - |
| gen/execve/t_execve | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| gen/posix_spawn/t_fileactions | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: gen/posix_spawn/t_fileactions | - |
| gen/posix_spawn/t_spawn | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: gen/posix_spawn/t_spawn | - |
| gen/posix_spawn/t_spawnattr | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| gen/t_alarm | alarm_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_alarm | alarm_fork | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_alarm | alarm_previous | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_arc4random | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| gen/t_assert | assert_false | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_assert | assert_true | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_basedirname | basename_posix | oracle-fail | oracle-behaviour:Input buffer was modified. | - |
| gen/t_basedirname | dirname_posix | oracle-fail | oracle-behaviour:Input buffer was modified. | - |
| gen/t_closefrom | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/gen/t_closefrom.c:59:25: error: ‘F_MAXFD’ undeclared (first use in this function) | - |
| gen/t_cpuset | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/gen/t_cpuset.c:47:9: error: unknown type name ‘cpuset_t’; did you mean ‘cpu_set_t’? | - |
| gen/t_ctype | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| gen/t_dir | seekdir_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_dir | telldir_leak | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_floatunditf | floatunditf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fmtcheck | unknown-bodies | oracle-fail | oracle-build:missing-symbol:fmtcheck | - |
| gen/t_fnmatch | fnmatch_backslashes | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fnmatch | fnmatch_casefold | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fnmatch | fnmatch_initialbracket | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fnmatch | fnmatch_leadingdir | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fnmatch | fnmatch_noescape | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fnmatch | fnmatch_pathname | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fnmatch | fnmatch_period | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_fpclassify | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/endian.h | - |
| gen/t_fpsetmask | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| gen/t_fpsetround | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: gen/t_fpsetround.c | - |
| gen/t_ftok | ftok_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_ftok | ftok_link | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_getcwd | getcwd_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_getcwd | getcwd_fts | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_getentropy | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/gen/t_getentropy.c:44:12: error: ‘GETENTROPY_MAX’ undeclared here (not in a function) | - |
| gen/t_getgrent | getgrent_loop | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_getgrent | getgrent_setgid | skip | needs-root | C44 |
| gen/t_glob | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| gen/t_humanize_number | unknown-bodies | oracle-fail | oracle-build:missing-header:util.h | - |
| gen/t_isnan | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: gen/t_isnan.c | - |
| gen/t_nice | nice_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_nice | nice_priority | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_nice | nice_root | skip | needs-root | C44 |
| gen/t_nice | nice_thread | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_pause | pause_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_pause | pause_kill | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_raise | raise_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_raise | raise_ret | oracle-fail | oracle-behaviour:raise(3) returned before signal handler | - |
| gen/t_raise | raise_sig | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_raise | raise_stress | oracle-fail | oracle-behaviour:not all signals were caught | - |
| gen/t_randomid | unknown-bodies | oracle-fail | oracle-build:missing-header:randomid.h | - |
| gen/t_realpath | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcat | - |
| gen/t_setdomainname | setdomainname_basic | skip | needs-root | C44 |
| gen/t_setdomainname | setdomainname_limit | skip | needs-root | C44 |
| gen/t_setdomainname | setdomainname_perm | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_sethostname | sethostname_basic | skip | needs-root | C44 |
| gen/t_sethostname | sethostname_limit | skip | needs-root | C44 |
| gen/t_sethostname | sethostname_perm | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_siginfo | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| gen/t_sleep | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/event.h | - |
| gen/t_syslog | syslog_invalid_priority | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_syslog | syslog_pthread | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_time | time_copy | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_time | time_mono | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_time | time_timeofday | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_timespec_get | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/gen/t_timespec_get.c:45:45: error: ‘TIME_MONOTONIC’ undeclared (first use in this function); did you mean ‘CLOCK_MONOTONIC’? | - |
| gen/t_ttyname | ttyname_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_ttyname | ttyname_r_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_ttyname | ttyname_r_stdin | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_ttyname | ttyname_stdin | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| gen/t_vis | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| hash/t_hash.sh | shell-test | skip | needs sh (O-B2) | C65 |
| hash/t_hmac | unknown-bodies | oracle-fail | oracle-build:missing-symbol:EVP_md5 | - |
| hash/t_sha2 | unknown-bodies | oracle-fail | oracle-build:missing-header:sha2.h | - |
| inet/t_inet_addr | inet_addr_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C51 |
| inet/t_inet_addr | inet_addr_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C51 |
| inet/t_inet_network | inet_network_basic | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/inet/t_inet_network.c:65: inet_network("1 bar") != (in_addr_t) 0x00000001: inet_network("1 bar") returned: 0xFFFFFFFF, expected: 0x00000001 | - |
| inet/t_inet_network | inet_network_err | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/inet/t_inet_network.c:95: inet_network("x1") != (in_addr_t) 0xffffffff: inet_network("x1") returned: 0x00000001, expected: 0xffffffff | - |
| locale/t_btowc | btowc | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_btowc.c:126: setlocale(LC_CTYPE, t->locale) != NULL not met | - |
| locale/t_btowc | btowc_posix | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_btowc.c:202: setlocale(LC_ALL, "POSIX") != "POSIX" (C != POSIX) | - |
| locale/t_btowc | stdc_iso_10646 | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_btowc.c:88: setlocale(LC_CTYPE, t->locale) != NULL not met | - |
| locale/t_c16rtomb | c16rtomb_c_locale_test | oracle-fail | oracle-behaviour:2 checks failed; see output for more details | - |
| locale/t_c16rtomb | c16rtomb_iso2022jp_locale_test | oracle-fail | oracle-behaviour:setlocale(LC_CTYPE, "ja_JP.ISO-2022-JP") failed; errno=2 | - |
| locale/t_c16rtomb | c16rtomb_iso_8859_15_test | oracle-fail | oracle-behaviour:setlocale(LC_CTYPE, "en_US.ISO8859-15") failed; errno=2 | - |
| locale/t_c16rtomb | c16rtomb_iso_8859_1_test | oracle-fail | oracle-behaviour:setlocale(LC_CTYPE, "en_US.ISO8859-1") failed; errno=2 | - |
| locale/t_c16rtomb | c16rtomb_utf_8_test | oracle-fail | oracle-behaviour:2 checks failed; see output for more details | - |
| locale/t_c32rtomb | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| locale/t_c8rtomb | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_c8rtomb.c:54:15: error: unknown type name ‘uint8_t’ | - |
| locale/t_digittoint | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| locale/t_ducet | unknown-bodies | oracle-fail | oracle-build:missing-header:ducet_test.h | - |
| locale/t_iconv | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| locale/t_io | unknown-bodies | oracle-fail | oracle-build:missing-symbol:funopen | - |
| locale/t_mbrtoc16 | mbrtoc16_c_locale_test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_mbrtoc16 | mbrtoc16_iso2022jp_locale_test | oracle-fail | oracle-behaviour:setlocale(LC_CTYPE, "ja_JP.ISO-2022-JP") failed; errno=2 | - |
| locale/t_mbrtoc16 | mbrtoc16_iso_8859_15_test | oracle-fail | oracle-behaviour:setlocale(LC_CTYPE, "en_US.ISO8859-15") failed; errno=2 | - |
| locale/t_mbrtoc16 | mbrtoc16_iso_8859_1_test | oracle-fail | oracle-behaviour:setlocale(LC_CTYPE, "en_US.ISO8859-1") failed; errno=2 | - |
| locale/t_mbrtoc16 | mbrtoc16_utf_8_test | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_mbrtoc32 | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| locale/t_mbrtoc8 | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_mbrtoc8.c:62:8: error: unknown type name ‘char8_t’ | - |
| locale/t_mbrtowc | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| locale/t_mbsnrtowcs | mbsnrtowcs | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_mbstowcs | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| locale/t_mbtowc | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| locale/t_sprintf | sprintf | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_sprintf.c:241: setlocale(LC_NUMERIC, t->locale) != NULL not met | - |
| locale/t_sprintf | sscanf | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_sprintf.c:350: setlocale(LC_NUMERIC, t->locale) != NULL not met | - |
| locale/t_sprintf | strto | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_sprintf.c:281: setlocale(LC_NUMERIC, t->locale) != NULL not met | - |
| locale/t_strfmon | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcat | - |
| locale/t_toupper | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| locale/t_uchar | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_uchar.c:35:9: error: unknown type name ‘char8_t’ | - |
| locale/t_wcscoll | wcscoll | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_wcscspn | wcscspn | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_wcspbrk | wcspbrk | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_wcsrtombs | wcsrtombs_advance | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_wcsspn | wcsspn | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_wcstod | wcstod | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/locale/t_wcstod.c:449: n != t->len | - |
| locale/t_wcstod | wcstombs | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C48 |
| locale/t_wctomb | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| locale/t_wctype | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| membar/t_dekker | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| membar/t_seqlock | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| membar/t_spinlock | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/atomic.h | - |
| misc/t_ubsan | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: misc/ubsan.c | - |
| misc/t_vis | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): TESTS_C inside .if/.for | - |
| net/getaddrinfo/t_getaddrinfo.sh | shell-test | skip | needs sh (O-B2) | C51 |
| net/t_ether_aton | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: net/aton_ether_subr.c | - |
| net/t_getprotoent | endprotoent_rewind | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/net/t_getprotoent.c:63: p->p_proto == i not met | - |
| net/t_getprotoent | getprotobyname_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C51 |
| net/t_getprotoent | getprotobyname_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C51 |
| net/t_getprotoent | getprotobynumber_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C51 |
| net/t_getprotoent | getprotobynumber_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C51 |
| net/t_getprotoent | getprotoent_next | oracle-fail | oracle-behaviour:52 checks failed; see output for more details | - |
| net/t_getprotoent | setprotoent_rewind | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/net/t_getprotoent.c:202: p->p_proto == 1 not met | - |
| net/t_hostent.sh | shell-test | skip | needs sh (O-B2) | C51 |
| net/t_if_nametoindex | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: net/t_if_nametoindex.c | - |
| net/t_nsdispatch.sh | shell-test | skip | needs sh (O-B2) | C51 |
| net/t_protoent.sh | shell-test | skip | needs sh (O-B2) | C51 |
| net/t_servent.sh | shell-test | skip | needs sh (O-B2) | C51 |
| nls/t_catalog | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/nls/t_catalog.c:53:25: error: ‘sys_nerr’ undeclared (first use in this function) | - |
| regex/t_exhaust | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: regex/t_exhaust.c | - |
| regex/t_regex.sh | shell-test | skip | needs sh (O-B2) | C45 |
| regex/t_regex_att | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: regex/t_regex_att.c | - |
| regex/t_regex_binary | negative_char | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C45 |
| regex/t_regex_binary | negative_ranges | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C45 |
| rpc/t_rpc | unknown-bodies | excluded | excluded:SunRPC is outside the scaffold (16 3.B.1.1) | - |
| rpc/t_xdr | unknown-bodies | excluded | excluded:SunRPC is outside the scaffold (16 3.B.1.1) | - |
| setjmp/t_setjmp | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: setjmp/t_setjmp.c | - |
| setjmp/t_sigstack | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| setjmp/t_threadjmp | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: setjmp/t_threadjmp.c | - |
| ssp/t_ssp.sh | shell-test | skip | needs sh (O-B2) | C50 |
| stdio/t_clearerr | clearerr_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_clearerr | clearerr_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_fflush | unknown-bodies | oracle-fail | oracle-build:missing-symbol:fpurge | - |
| stdio/t_fmemopen | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/stdio/t_fmemopen.c:893:29: error: ‘i’ undeclared (first use in this function) | - |
| stdio/t_fopen | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/module.h | - |
| stdio/t_fputc | unknown-bodies | excluded | excluded:glibc oracle is nondeterministic for the errno cases; not a libc verdict | - |
| stdio/t_intr.sh | shell-test | skip | needs sh (O-B2) | C56 |
| stdio/t_open_memstream | test_open_memstream | oracle-fail | oracle-behaviour:1 checks failed; see output for more details | - |
| stdio/t_popen | popen_zeropad | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_c99 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_dotzero | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_double_a | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_float | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_long_double_a | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_posarg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_posarg_error | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | snprintf_posarg_width | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_printf | sprintf_zeropad | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stdio/t_scanf | unknown-bodies | oracle-fail | oracle-build:missing-symbol:___STRING | - |
| stdlib/t_a64l | unknown-bodies | oracle-fail | oracle-build:missing-symbol:l64a_r | - |
| stdlib/t_abs | abs_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_abs | imaxabs_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_abs | labs_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_abs | llabs_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_atexit.sh | shell-test | skip | needs sh (O-B2) | C41 |
| stdlib/t_atoi | atof_strtod | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_atoi | atoi_strtol | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_atoi | atol_strtol | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_atoi | atoll_strtoll | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_div | div_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_div | ldiv_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_div | lldiv_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_exit | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcpy | - |
| stdlib/t_getenv | clearenv_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_getenv | getenv_basic | oracle-fail | oracle-behaviour:1 checks failed; see output for more details | - |
| stdlib/t_getenv | putenv_basic | oracle-fail | oracle-behaviour:no result file (signal 11) | - |
| stdlib/t_getenv | setenv_basic | oracle-fail | oracle-behaviour:no result file (signal 11) | - |
| stdlib/t_getenv | setenv_mixed | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_getenv_thread | unknown-bodies | oracle-fail | oracle-build:missing-symbol:getenv_r | - |
| stdlib/t_getopt.sh | shell-test | skip | needs sh (O-B2) | C41 |
| stdlib/t_hsearch | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: stdlib/t_hsearch.c | - |
| stdlib/t_mi_vector_hash | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/stdlib/t_mi_vector_hash.c:50:9: error: unknown type name ‘uint32_t’ | - |
| stdlib/t_mktemp | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/stdlib/t_mktemp.c:196:17: error: ‘O_SHLOCK’ undeclared (first use in this function); did you mean ‘F_SHLCK’? | - |
| stdlib/t_posix_memalign | aligned_alloc_basic | oracle-fail | oracle-behaviour:24 checks failed; see output for more details | - |
| stdlib/t_posix_memalign | posix_memalign_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_random | random_same | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_sort.sh | shell-test | skip | needs sh (O-B2) | C41 |
| stdlib/t_strtod | strtod_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtod_gherman_bug | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtod_hex | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtod_inf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtod_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtod_round | skip | Requires <fenv.h> support | C41 |
| stdlib/t_strtod | strtod_underflow | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtof_inf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtof_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtold_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtold_inf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtod | strtold_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtoi | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strtoi | - |
| stdlib/t_strtol | strtol_base | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtol | strtol_case | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtol | strtol_invbase | oracle-fail | oracle-behaviour:6 checks failed; see output for more details | - |
| stdlib/t_strtol | strtol_range | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_strtol | strtol_signed | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| stdlib/t_system | system_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| string/t_bm | unknown-bodies | oracle-fail | oracle-build:missing-header:bm.h | - |
| string/t_memchr | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/string/t_memchr.c:178:37: error: ‘SIZE_MAX’ undeclared (first use in this function) | - |
| string/t_memcpy | unknown-bodies | oracle-fail | oracle-build:missing-header:md5.h | - |
| string/t_memmem | memmem_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_memmem | memmem_oob | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_memset | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/string/t_memset.c:52:31: error: ‘explicit_memset’ undeclared here (not in a function) | - |
| string/t_popcount | unknown-bodies | oracle-fail | oracle-build:missing-symbol:popcount | - |
| string/t_strcat | strcat_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strcat | strncat_simple | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strchr | strchr_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strchrnul | strchrnul_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strcmp | strcmp_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strcmp | strcmp_simple | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strcoll | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| string/t_strcpy | strcpy_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strcspn | strcspn | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strerror | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/string/t_strerror.c:51:25: error: ‘sys_nerr’ undeclared (first use in this function) | - |
| string/t_stresep | unknown-bodies | oracle-fail | oracle-build:missing-symbol:stresep | - |
| string/t_strlen | strlen_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strlen | strlen_huge | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strlen | strnlen_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strpbrk | strpbrk | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strrchr | strrchr_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_strspn | strspn | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| string/t_swab | swab_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| sys/t_access | access_access | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_access | access_fault | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_access | access_inval | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_access | access_notdir | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_access | access_notexist | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_access | access_toolong | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_aio_cancel | cancel_active_write | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_aio_cancel | cancel_completed_request | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_aio_cancel | cancel_invalid_fd | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_aio_lio | lio_nowait | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_aio_lio | lio_wait_write_then_read | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_aio_rw | write_then_read_back | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_aio_suspend | suspend_any | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_bind | bind_foreign_family | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_bind.c:67: EAFNOSUPPORT == errno not met | - |
| sys/t_chroot | unknown-bodies | oracle-fail | oracle-build:missing-symbol:fchroot | - |
| sys/t_clock_gettime | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| sys/t_clock_nanosleep | clock_nanosleep_remain | oracle-fail | oracle-behaviour:1 checks failed; see output for more details | - |
| sys/t_clone | unknown-bodies | oracle-fail | oracle-build:missing-symbol:__UNVOLATILE | - |
| sys/t_connect | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_connect.c:69:15: error: ‘IP_PORTRANGE_LOW’ undeclared (first use in this function) | - |
| sys/t_dup | dup2_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup2_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup2_max | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup2_mode | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup3_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup3_max | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup3_mode | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_dup | dup_max | oracle-fail | oracle-behaviour:system call error | - |
| sys/t_dup | dup_mode | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_eventfd | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/event.h | - |
| sys/t_fork | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| sys/t_fsync | fsync_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_fsync | fsync_sync | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_fsync.c:104: fd != -1 not met | - |
| sys/t_futex_ops | unknown-bodies | oracle-fail | oracle-build:missing-header:atomic.h | - |
| sys/t_futex_robust | unknown-bodies | oracle-fail | oracle-build:missing-header:lwp.h | - |
| sys/t_getcontext | getcontext_err | oracle-fail | oracle-behaviour:no result file (signal 11) | - |
| sys/t_getcontext | setcontext_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getcontext | setcontext_link | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getgroups | getgroups_err | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_getgroups.c:56: errno == EFAULT not met | - |
| sys/t_getgroups | getgroups_getgid | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getgroups | getgroups_setgid | skip | needs-root | C55 |
| sys/t_getgroups | getgroups_zero | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getitimer | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_getlogin | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_getlogin.c:64:18: error: ‘MAXLOGNAME’ undeclared (first use in this function) | - |
| sys/t_getpid | getpid_process | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getpid | getpid_thread | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getrandom | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_getrandom.c:45:8: error: unknown type name ‘uint8_t’ | - |
| sys/t_getrusage | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_getrusage.c:181:12: error: ‘struct sockaddr_in’ has no member named ‘sin_len’ | - |
| sys/t_getsid | getsid_current | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getsid | getsid_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getsid | getsid_process | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_getsockname | getsockname_unix | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_gettimeofday | gettimeofday_err | oracle-fail | oracle-behaviour:no result file (signal 11) | - |
| sys/t_gettimeofday | gettimeofday_mono | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_issetugid | unknown-bodies | oracle-fail | oracle-build:missing-symbol:issetugid | - |
| sys/t_kevent | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/event.h | - |
| sys/t_kill | kill_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_kill | kill_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_kill | kill_int_min | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_kill | kill_perm | skip | needs-root | C55 |
| sys/t_kill | kill_pgrp_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_kill | kill_pgrp_zero | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_link | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcat | - |
| sys/t_listen | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_listen.c:112:15: error: ‘IP_PORTRANGE_LOW’ undeclared (first use in this function) | - |
| sys/t_lwp_create | unknown-bodies | excluded | excluded:NetBSD lwp_* private syscalls; no equivalent planned (16 3.B.1.1) | - |
| sys/t_lwp_ctl | unknown-bodies | excluded | excluded:NetBSD lwp_* private syscalls; no equivalent planned (16 3.B.1.1) | - |
| sys/t_mincore | unknown-bodies | oracle-fail | oracle-build:missing-header:kvm.h | - |
| sys/t_minherit | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| sys/t_mkdir | mkdir_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_mkdir | mkdir_mode | skip | needs-root | C55 |
| sys/t_mkdir | mkdir_perm | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_mkdir | mkdir_trail | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_mkfifo | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_mknod | mknod_err | skip | needs-root | C55 |
| sys/t_mknod | mknod_exist | skip | needs-root | C55 |
| sys/t_mknod | mknod_perm | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_mknod.c:125: Expected true value in mknod(path, S_IFCHR, 0) == -1 | - |
| sys/t_mknod | mknod_stat | skip | needs-root | C55 |
| sys/t_mlock | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| sys/t_mmap | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/disklabel.h | - |
| sys/t_mprotect | unknown-bodies | excluded | excluded:kernel memory-protection behaviour, not a libc wrapper (16 3.B.1.1) | - |
| sys/t_msgctl | unknown-bodies | excluded | excluded:SysV IPC is a SKIP area (16 3.B.1.1) | - |
| sys/t_msgget | unknown-bodies | excluded | excluded:SysV IPC is a SKIP area (16 3.B.1.1) | - |
| sys/t_msgrcv | unknown-bodies | excluded | excluded:SysV IPC is a SKIP area (16 3.B.1.1) | - |
| sys/t_msgsnd | unknown-bodies | excluded | excluded:SysV IPC is a SKIP area (16 3.B.1.1) | - |
| sys/t_msync | msync_async | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_msync | msync_err | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_msync.c:175: errno == EFAULT not met | - |
| sys/t_msync | msync_invalidate | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_msync | msync_sync | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_nanosleep | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_nanosleep.c:80:45: error: expected expression before ‘<=’ token | - |
| sys/t_pipe | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_pipe2 | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: sys/t_pipe2.c | - |
| sys/t_poll | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_pollts | unknown-bodies | oracle-fail | oracle-build:missing-symbol:pollts | - |
| sys/t_posix_fadvise | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): TESTS_C inside .if/.for | - |
| sys/t_posix_fallocate | ebadf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_ppoll | basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_ppoll | err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_ppoll | sigmask | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_ptrace | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| sys/t_ptrace_kill | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_ptrace_sigchld | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_ptrace_wait | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_ptrace_wait3 | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_ptrace_wait4 | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_ptrace_wait6 | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_ptrace_waitid | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_ptrace_waitpid | unknown-bodies | excluded | excluded:NetBSD ptrace(2); no equivalent planned (16 3.B.1.1) | - |
| sys/t_recvmmsg | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: sys/t_recvmmsg.c | - |
| sys/t_revoke | revoke_basic | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_revoke.c:78: revoke(path) == 0 not met | - |
| sys/t_revoke | revoke_err | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_revoke.c:111: Expected errno 14, got 38, in revoke((char *)-1) == -1 | - |
| sys/t_revoke | revoke_perm | skip | needs-root | C55 |
| sys/t_select | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_select.c:63:6: error: unknown type name ‘uint8_t’ | - |
| sys/t_sendmmsg | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_sendmmsg.c:147:58: error: ‘FIONSPACE’ undeclared (first use in this function) | - |
| sys/t_sendrecv | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_sendrecv.c:124:55: error: ‘SO_RERROR’ undeclared (first use in this function); did you mean ‘SO_ERROR’? | - |
| sys/t_setrlimit | unknown-bodies | oracle-fail | oracle-build:missing-header:lwp.h | - |
| sys/t_setuid | setuid_perm | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_setuid | setuid_real | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_setuid | setuid_root | skip | needs-root | C55 |
| sys/t_sigaction | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_sigaltstack | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_sigio | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_sigqueue | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_sigqueue.c:53:17: error: ‘SIGINFO’ undeclared (first use in this function); did you mean ‘SIGIO’? | - |
| sys/t_sigtimedwait | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_socketpair | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: sys/t_socketpair.c | - |
| sys/t_stat | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_stat.c:71:35: error: ‘UF_NODUMP’ undeclared (first use in this function) | - |
| sys/t_swapcontext | unknown-bodies | oracle-fail | oracle-build:missing-header:lwp.h | - |
| sys/t_syscall | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/endian.h | - |
| sys/t_timer_create | unknown-bodies | oracle-fail | oracle-build:missing-header:h_macros.h | - |
| sys/t_timerfd | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/event.h | - |
| sys/t_truncate | ftruncate_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_truncate | ftruncate_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_truncate | truncate_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_truncate | truncate_err | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_truncate.c:167: Expected errno 13, got 2, in truncate(buf, 999) == -1 | - |
| sys/t_ucontext | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_ucontext.c:84:37: error: expected ‘)’ before ‘PRIxREGISTER’ | - |
| sys/t_umask | umask_fork | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_umask | umask_open | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_umask | umask_previous | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_unlink | unlink_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_unlink | unlink_err | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_unlink.c:88: Expected errno 16, got 21, in unlink("/") == -1 | - |
| sys/t_unlink | unlink_fifo | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_unlink | unlink_perm | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| sys/t_vfork | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| sys/t_wait | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_wait.c:58:24: error: storage size of ‘wru’ isn’t known | - |
| sys/t_wait_noproc | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_wait_noproc.c:83:17: error: ‘WTRAPPED’ undeclared (first use in this function); did you mean ‘CLD_TRAPPED’? | - |
| sys/t_wait_noproc_wnohang | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_wait_noproc.c:83:17: error: ‘WTRAPPED’ undeclared (first use in this function); did you mean ‘CLD_TRAPPED’? | - |
| sys/t_write | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/sys/t_write.c:257:23: error: ‘_PATH_DEVZERO’ undeclared (first use in this function) | - |
| t_cdb | unknown-bodies | oracle-fail | oracle-build:missing-header:cdbr.h | - |
| t_convfp | cast_ulong | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| t_convfp | cast_ulong2 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| t_convfp | conv_uint | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| t_convfp | conv_ulong | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| t_gdtoa | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: t_gdtoa.c | - |
| t_long_double | long_double | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| termios/t_tcsetpgrp | tcsetpgrp_err | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C55 |
| time/t_mktime | localtime_r_gmt | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| time/t_mktime | mktime_negyear | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| time/t_mktime | timegm_epoch | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| time/t_strptime | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/time/t_strptime.c:506:31: error: expected expression before ‘time_t’ | - |
| tls/t_tls_dlopen | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): TESTS_C inside .if/.for | - |
| tls/t_tls_dynamic | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): TESTS_C inside .if/.for | - |
| tls/t_tls_static | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/tls.h | - |
| ttyio/t_ptm | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/ttyio/t_ptm.c:63:23: error: storage size of ‘ptm’ isn’t known | - |
| ttyio/t_ttyio | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libc/ttyio/t_ttyio.c:56:2: error: #error where openpty? | - |

Programs of netbsd-libc:

* atomic/t___sync_add: 0 pass
* atomic/t___sync_and: 0 pass
* atomic/t___sync_compare_and_swap: 0 pass
* atomic/t___sync_lock: 0 pass
* atomic/t___sync_nand: 0 pass
* atomic/t___sync_or: 0 pass
* atomic/t___sync_sub: 0 pass
* atomic/t___sync_xor: 0 pass
* atomic/t_atomic_add: 0 pass
* atomic/t_atomic_and: 0 pass
* atomic/t_atomic_cas: 0 pass
* atomic/t_atomic_dec: 0 pass
* atomic/t_atomic_inc: 0 pass
* atomic/t_atomic_or: 0 pass
* atomic/t_atomic_swap: 0 pass
* c063/t_faccessat: 0 pass
* c063/t_fchmodat: 0 pass
* c063/t_fchownat: 0 pass
* c063/t_fexecve: 0 pass
* c063/t_fstatat: 0 pass
* c063/t_linkat: 0 pass
* c063/t_mkdirat: 0 pass
* c063/t_mkfifoat: 0 pass
* c063/t_mknodat: 0 pass
* c063/t_o_search: 0 pass
* c063/t_openat: 0 pass
* c063/t_readlinkat: 0 pass
* c063/t_renameat: 0 pass
* c063/t_symlinkat: 0 pass
* c063/t_unlinkat: 0 pass
* c063/t_utimensat: 0 pass
* db/t_db.sh: 0 pass
* db/t_db_hash_seq: 0 pass
* gen/execve/t_execve: 0 pass
* gen/posix_spawn/t_fileactions: 0 pass
* gen/posix_spawn/t_spawn: 0 pass
* gen/posix_spawn/t_spawnattr: 0 pass
* gen/t_alarm: 0 pass
* gen/t_arc4random: 0 pass
* gen/t_assert: 0 pass
* gen/t_basedirname: 0 pass
* gen/t_closefrom: 0 pass
* gen/t_cpuset: 0 pass
* gen/t_ctype: 0 pass
* gen/t_dir: 0 pass
* gen/t_floatunditf: 0 pass
* gen/t_fmtcheck: 0 pass
* gen/t_fnmatch: 0 pass
* gen/t_fpclassify: 0 pass
* gen/t_fpsetmask: 0 pass
* gen/t_fpsetround: 0 pass
* gen/t_ftok: 0 pass
* gen/t_getcwd: 0 pass
* gen/t_getentropy: 0 pass
* gen/t_getgrent: 0 pass
* gen/t_glob: 0 pass
* gen/t_humanize_number: 0 pass
* gen/t_isnan: 0 pass
* gen/t_nice: 0 pass
* gen/t_pause: 0 pass
* gen/t_raise: 0 pass
* gen/t_randomid: 0 pass
* gen/t_realpath: 0 pass
* gen/t_setdomainname: 0 pass
* gen/t_sethostname: 0 pass
* gen/t_siginfo: 0 pass
* gen/t_sleep: 0 pass
* gen/t_syslog: 0 pass
* gen/t_time: 0 pass
* gen/t_timespec_get: 0 pass
* gen/t_ttyname: 0 pass
* gen/t_vis: 0 pass
* hash/t_hash.sh: 0 pass
* hash/t_hmac: 0 pass
* hash/t_sha2: 0 pass
* inet/t_inet_addr: 0 pass
* inet/t_inet_network: 0 pass
* locale/t_btowc: 0 pass
* locale/t_c16rtomb: 0 pass
* locale/t_c32rtomb: 0 pass
* locale/t_c8rtomb: 0 pass
* locale/t_digittoint: 0 pass
* locale/t_ducet: 0 pass
* locale/t_iconv: 0 pass
* locale/t_io: 0 pass
* locale/t_mbrtoc16: 0 pass
* locale/t_mbrtoc32: 0 pass
* locale/t_mbrtoc8: 0 pass
* locale/t_mbrtowc: 0 pass
* locale/t_mbsnrtowcs: 0 pass
* locale/t_mbstowcs: 0 pass
* locale/t_mbtowc: 0 pass
* locale/t_sprintf: 0 pass
* locale/t_strfmon: 0 pass
* locale/t_toupper: 0 pass
* locale/t_uchar: 0 pass
* locale/t_wcscoll: 0 pass
* locale/t_wcscspn: 0 pass
* locale/t_wcspbrk: 0 pass
* locale/t_wcsrtombs: 0 pass
* locale/t_wcsspn: 0 pass
* locale/t_wcstod: 0 pass
* locale/t_wctomb: 0 pass
* locale/t_wctype: 0 pass
* membar/t_dekker: 0 pass
* membar/t_seqlock: 0 pass
* membar/t_spinlock: 0 pass
* misc/t_ubsan: 0 pass
* misc/t_vis: 0 pass
* net/getaddrinfo/t_getaddrinfo.sh: 0 pass
* net/t_ether_aton: 0 pass
* net/t_getprotoent: 0 pass
* net/t_hostent.sh: 0 pass
* net/t_if_nametoindex: 0 pass
* net/t_nsdispatch.sh: 0 pass
* net/t_protoent.sh: 0 pass
* net/t_servent.sh: 0 pass
* nls/t_catalog: 0 pass
* regex/t_exhaust: 0 pass
* regex/t_regex.sh: 0 pass
* regex/t_regex_att: 0 pass
* regex/t_regex_binary: 0 pass
* rpc/t_rpc: 0 pass
* rpc/t_xdr: 0 pass
* setjmp/t_setjmp: 0 pass
* setjmp/t_sigstack: 0 pass
* setjmp/t_threadjmp: 0 pass
* ssp/t_ssp.sh: 0 pass
* stdio/t_clearerr: 0 pass
* stdio/t_fflush: 0 pass
* stdio/t_fmemopen: 0 pass
* stdio/t_fopen: 0 pass
* stdio/t_fputc: 0 pass
* stdio/t_intr.sh: 0 pass
* stdio/t_open_memstream: 0 pass
* stdio/t_popen: 0 pass
* stdio/t_printf: 0 pass
* stdio/t_scanf: 0 pass
* stdlib/t_a64l: 0 pass
* stdlib/t_abs: 0 pass
* stdlib/t_atexit.sh: 0 pass
* stdlib/t_atoi: 0 pass
* stdlib/t_div: 0 pass
* stdlib/t_exit: 0 pass
* stdlib/t_getenv: 0 pass
* stdlib/t_getenv_thread: 0 pass
* stdlib/t_getopt.sh: 0 pass
* stdlib/t_hsearch: 0 pass
* stdlib/t_mi_vector_hash: 0 pass
* stdlib/t_mktemp: 0 pass
* stdlib/t_posix_memalign: 0 pass
* stdlib/t_random: 0 pass
* stdlib/t_sort.sh: 0 pass
* stdlib/t_strtod: 0 pass
* stdlib/t_strtoi: 0 pass
* stdlib/t_strtol: 0 pass
* stdlib/t_system: 0 pass
* string/t_bm: 0 pass
* string/t_memchr: 0 pass
* string/t_memcpy: 0 pass
* string/t_memmem: 0 pass
* string/t_memset: 0 pass
* string/t_popcount: 0 pass
* string/t_strcat: 0 pass
* string/t_strchr: 0 pass
* string/t_strchrnul: 0 pass
* string/t_strcmp: 0 pass
* string/t_strcoll: 0 pass
* string/t_strcpy: 0 pass
* string/t_strcspn: 0 pass
* string/t_strerror: 0 pass
* string/t_stresep: 0 pass
* string/t_strlen: 0 pass
* string/t_strpbrk: 0 pass
* string/t_strrchr: 0 pass
* string/t_strspn: 0 pass
* string/t_swab: 0 pass
* sys/t_access: 0 pass
* sys/t_aio_cancel: 0 pass
* sys/t_aio_lio: 0 pass
* sys/t_aio_rw: 0 pass
* sys/t_aio_suspend: 0 pass
* sys/t_bind: 0 pass
* sys/t_chroot: 0 pass
* sys/t_clock_gettime: 0 pass
* sys/t_clock_nanosleep: 0 pass
* sys/t_clone: 0 pass
* sys/t_connect: 0 pass
* sys/t_dup: 0 pass
* sys/t_eventfd: 0 pass
* sys/t_fork: 0 pass
* sys/t_fsync: 0 pass
* sys/t_futex_ops: 0 pass
* sys/t_futex_robust: 0 pass
* sys/t_getcontext: 0 pass
* sys/t_getgroups: 0 pass
* sys/t_getitimer: 0 pass
* sys/t_getlogin: 0 pass
* sys/t_getpid: 0 pass
* sys/t_getrandom: 0 pass
* sys/t_getrusage: 0 pass
* sys/t_getsid: 0 pass
* sys/t_getsockname: 0 pass
* sys/t_gettimeofday: 0 pass
* sys/t_issetugid: 0 pass
* sys/t_kevent: 0 pass
* sys/t_kill: 0 pass
* sys/t_link: 0 pass
* sys/t_listen: 0 pass
* sys/t_lwp_create: 0 pass
* sys/t_lwp_ctl: 0 pass
* sys/t_mincore: 0 pass
* sys/t_minherit: 0 pass
* sys/t_mkdir: 0 pass
* sys/t_mkfifo: 0 pass
* sys/t_mknod: 0 pass
* sys/t_mlock: 0 pass
* sys/t_mmap: 0 pass
* sys/t_mprotect: 0 pass
* sys/t_msgctl: 0 pass
* sys/t_msgget: 0 pass
* sys/t_msgrcv: 0 pass
* sys/t_msgsnd: 0 pass
* sys/t_msync: 0 pass
* sys/t_nanosleep: 0 pass
* sys/t_pipe: 0 pass
* sys/t_pipe2: 0 pass
* sys/t_poll: 0 pass
* sys/t_pollts: 0 pass
* sys/t_posix_fadvise: 0 pass
* sys/t_posix_fallocate: 0 pass
* sys/t_ppoll: 0 pass
* sys/t_ptrace: 0 pass
* sys/t_ptrace_kill: 0 pass
* sys/t_ptrace_sigchld: 0 pass
* sys/t_ptrace_wait: 0 pass
* sys/t_ptrace_wait3: 0 pass
* sys/t_ptrace_wait4: 0 pass
* sys/t_ptrace_wait6: 0 pass
* sys/t_ptrace_waitid: 0 pass
* sys/t_ptrace_waitpid: 0 pass
* sys/t_recvmmsg: 0 pass
* sys/t_revoke: 0 pass
* sys/t_select: 0 pass
* sys/t_sendmmsg: 0 pass
* sys/t_sendrecv: 0 pass
* sys/t_setrlimit: 0 pass
* sys/t_setuid: 0 pass
* sys/t_sigaction: 0 pass
* sys/t_sigaltstack: 0 pass
* sys/t_sigio: 0 pass
* sys/t_sigqueue: 0 pass
* sys/t_sigtimedwait: 0 pass
* sys/t_socketpair: 0 pass
* sys/t_stat: 0 pass
* sys/t_swapcontext: 0 pass
* sys/t_syscall: 0 pass
* sys/t_timer_create: 0 pass
* sys/t_timerfd: 0 pass
* sys/t_truncate: 0 pass
* sys/t_ucontext: 0 pass
* sys/t_umask: 0 pass
* sys/t_unlink: 0 pass
* sys/t_vfork: 0 pass
* sys/t_wait: 0 pass
* sys/t_wait_noproc: 0 pass
* sys/t_wait_noproc_wnohang: 0 pass
* sys/t_write: 0 pass
* t_cdb: 0 pass
* t_convfp: 0 pass
* t_gdtoa: 0 pass
* t_long_double: 0 pass
* termios/t_tcsetpgrp: 0 pass
* time/t_mktime: 0 pass
* time/t_strptime: 0 pass
* tls/t_tls_dlopen: 0 pass
* tls/t_tls_dynamic: 0 pass
* tls/t_tls_static: 0 pass
* ttyio/t_ptm: 0 pass
* ttyio/t_ttyio: 0 pass

#### netbsd-libcrypt: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| t_crypt | crypt_salts | oracle-fail | oracle-behaviour:16 checks failed; see output for more details | - |

Programs of netbsd-libcrypt:

* t_crypt: 0 pass

#### netbsd-libm: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| t_acos | unknown-bodies | oracle-fail | oracle-build:missing-header:t_libm.h | - |
| t_asin | asin_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asin_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asin_inrange | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asin_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asin_range | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asin_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asin_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asinf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asinf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asinf_inrange | oracle-fail | oracle-behaviour:Test case was expecting a failure but none were raised | - |
| t_asin | asinf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asinf_range | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asinf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_asin | asinf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_atan | unknown-bodies | oracle-fail | oracle-build:missing-header:t_libm.h | - |
| t_bit | signbit | oracle-fail | oracle-behaviour:atfu_signbit_body:70 iteration 0 signbitf is wrong about the sign of -1.000000 | - |
| t_casinh | casinh | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrt_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrt_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrt_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrt_pow | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrt_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrt_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtf_powf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtl_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtl_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtl_powl | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtl_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cbrt | cbrtl_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceil_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceil_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceil_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceil_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceil_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceil_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceilf_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceilf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceilf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceilf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceilf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceilf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceill_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceill_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceill_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceill_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceill_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | ceill_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floor_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floor_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floor_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floor_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floor_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floor_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorf_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorl_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorl_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorl_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorl_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | floorl_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | trunc_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | trunc_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | trunc_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | trunc_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | trunc_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | trunc_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncf_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncl_basic | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncl_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncl_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncl_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ceil | truncl_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cos_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cos_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cos_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cos_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cos_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cos_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosf_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosl_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosl_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosl_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosl_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cos | cosl_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | cosh_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | cosh_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | cosh_inrange | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | cosh_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | cosh_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | cosh_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | coshf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | coshf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | coshf_inrange | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | coshf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | coshf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_cosh | coshf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erfc_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erfc_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erfc_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erfcf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erfcf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erfcf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erff_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erff_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erff_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erff_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_erf | erff_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_errhandling | unknown-bodies | oracle-fail | oracle-build:missing-symbol:__USE | - |
| t_exp | unknown-bodies | oracle-fail | oracle-build:missing-header:t_libm.h | - |
| t_fenv | t_nofenv | skip | no fenv.h support on this architecture | C47 |
| t_fmod | unknown-bodies | oracle-fail | oracle-build:missing-header:isqemu.h | - |
| t_hypot | hypot_exact | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_hypot | hypot_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_hypot | hypot_trivial | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_hypot | hypotf_exact | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_hypot | hypotf_trivial | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_hypot | hypotl_exact | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_hypot | hypotl_trivial | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_hypot | pr50698 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ilogb | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libm/t_ilogb.c:42:41: error: ‘__nothing’ undeclared (first use in this function) | - |
| t_infinity | infinity_double | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_infinity | infinity_float | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_infinity | infinity_long_double | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_ldexp | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libm/t_ldexp.c:432:42: error: expected ‘)’ before ‘___STRING’ | - |
| t_log | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libm/t_log.c:466:12: error: ‘uint64_t’ undeclared here (not in a function) | - |
| t_modf | modf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_modf | modff | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_modf | modfl | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | next_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | next_near_0 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | next_near_1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | next_near_1_5 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | next_near_infinity | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | next_near_sub_normal | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | next_signed_0 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextf_near_0 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextf_near_1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextf_near_1_5 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextf_near_infinity | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextf_near_sub_normal | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextf_signed_0 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextl_near_0 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextl_near_1 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextl_near_1_5 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextl_near_infinity | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextl_near_sub_normal | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_next | nextl_signed_0 | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_inf_neg_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_inf_neg_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_inf_pos_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_inf_pos_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_nan_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_nan_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_one_neg_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_one_pos_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_zero_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | pow_zero_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_inf_neg_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_inf_neg_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_inf_pos_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_inf_pos_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_nan_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_nan_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_one_neg_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_one_pos_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_zero_x | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_pow | powf_zero_y | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_precision | t_precision | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_remquo | remquo_args | oracle-fail | oracle-behaviour:2 checks failed; see output for more details | - |
| t_round | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libm/t_round.c:107:24: error: expected ‘)’ before ‘PRIu64’ | - |
| t_scalbn | scalbn_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbn_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbn_ldexp | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbn_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbn_val | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbn_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbn_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnf_ldexpf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnf_val | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnl_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnl_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnl_val | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnl_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_scalbn | scalbnl_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sin_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sin_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sin_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sin_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sin_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sin_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sinf_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sinf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sinf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sinf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sinf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sin | sinf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincos_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincos_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincos_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincos_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincos_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincos_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosf_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosl_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosl_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosl_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosl_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sincos | sincosl_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinh_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinh_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinh_inrange | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinh_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinh_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinh_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinhf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinhf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinhf_inrange | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinhf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinhf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sinh | sinhf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrt_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrt_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrt_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrt_pow | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrt_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrt_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtf_powf | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtl_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtl_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtl_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtl_powl | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtl_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_sqrt | sqrtl_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tan_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tan_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tan_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tan_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tan_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tan_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tanf_angles | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tanf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tanf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tanf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tanf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tan | tanf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanh_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanh_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanh_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanh_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanh_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanhf_inf_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanhf_inf_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanhf_nan | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanhf_zero_neg | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| t_tanh | tanhf_zero_pos | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |

Programs of netbsd-libm:

* t_acos: 0 pass
* t_asin: 0 pass
* t_atan: 0 pass
* t_bit: 0 pass
* t_casinh: 0 pass
* t_cbrt: 0 pass
* t_ceil: 0 pass
* t_cos: 0 pass
* t_cosh: 0 pass
* t_erf: 0 pass
* t_errhandling: 0 pass
* t_exp: 0 pass
* t_fenv: 0 pass
* t_fmod: 0 pass
* t_hypot: 0 pass
* t_ilogb: 0 pass
* t_infinity: 0 pass
* t_ldexp: 0 pass
* t_log: 0 pass
* t_modf: 0 pass
* t_next: 0 pass
* t_pow: 0 pass
* t_precision: 0 pass
* t_remquo: 0 pass
* t_round: 0 pass
* t_scalbn: 0 pass
* t_sin: 0 pass
* t_sincos: 0 pass
* t_sinh: 0 pass
* t_sqrt: 0 pass
* t_tan: 0 pass
* t_tanh: 0 pass

Programs of netbsd-libposix:


#### netbsd-libpthread: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| dlopen/t_dlopen | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): TESTS_C inside .if/.for | - |
| dlopen/t_dso_pthread_create | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): TESTS_C inside .if/.for | - |
| dlopen/t_main_pthread_create | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): TESTS_C inside .if/.for | - |
| t_atexit.sh | shell-test | skip | needs sh (O-B2) | C57 |
| t_barrier | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_call_once | call_once | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_cancel.sh | shell-test | skip | needs sh (O-B2) | C57 |
| t_cancellation | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/event.h | - |
| t_cnd | cnd_broadcast | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_cnd | cnd_init | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_cnd | cnd_signal | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_cnd | cnd_timedwait | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_compat_cancel | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/event.h | - |
| t_cond | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_condwait | unknown-bodies | oracle-fail | oracle-build:missing-header:isqemu.h | - |
| t_detach | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_equal | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_exit.sh | shell-test | skip | needs sh (O-B2) | C57 |
| t_fork | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_fpu | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_join | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_kill | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_mtx | mtx_init | oracle-fail | oracle-behaviour:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libpthread/t_mtx.c:64: mtx_init(&m, mtx_recursive) != thrd_error | - |
| t_mtx | mtx_lock | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_mtx | mtx_timedlock | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_mtx | mtx_trylock | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_mutex | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sched.h | - |
| t_name | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_once | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_preempt | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_resolv.sh | shell-test | skip | needs sh (O-B2) | C57 |
| t_rwlock | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_sem | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_siglongjmp | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_sigmask | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_sigsuspend | sigsuspend | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_sleep | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_stack | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sysctl.h | - |
| t_swapcontext | unknown-bodies | oracle-fail | oracle-build:missing-header:h_common.h | - |
| t_thrd | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/netbsd/libpthread/t_thrd.c:179:26: error: storage size of ‘timer’ isn’t known | - |
| t_thread_local_dtor.sh | shell-test | skip | needs sh (O-B2) | C57 |
| t_timedmutex | unknown-bodies | oracle-fail | oracle-build:missing-header:sys/sched.h | - |
| t_tss | tss_create | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_tss | tss_destructor_main_thread | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_tss | tss_destructor_thread_exit | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| t_tss | tss_set | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C57 |
| weak/t_pthread_weak_nothread | unknown-bodies | oracle-fail | oracle-build:missing-header:h_pthread_weak.h | - |
| weak/t_pthread_weak_threaded | unknown-bodies | oracle-fail | oracle-build:missing-header:h_pthread_weak.h | - |

Programs of netbsd-libpthread:

* dlopen/t_dlopen: 0 pass
* dlopen/t_dso_pthread_create: 0 pass
* dlopen/t_main_pthread_create: 0 pass
* t_atexit.sh: 0 pass
* t_barrier: 0 pass
* t_call_once: 0 pass
* t_cancel.sh: 0 pass
* t_cancellation: 0 pass
* t_cnd: 0 pass
* t_compat_cancel: 0 pass
* t_cond: 0 pass
* t_condwait: 0 pass
* t_detach: 0 pass
* t_equal: 0 pass
* t_exit.sh: 0 pass
* t_fork: 0 pass
* t_fpu: 0 pass
* t_join: 0 pass
* t_kill: 0 pass
* t_mtx: 0 pass
* t_mutex: 0 pass
* t_name: 0 pass
* t_once: 0 pass
* t_preempt: 0 pass
* t_resolv.sh: 0 pass
* t_rwlock: 0 pass
* t_sem: 0 pass
* t_siglongjmp: 0 pass
* t_sigmask: 0 pass
* t_sigsuspend: 0 pass
* t_sleep: 0 pass
* t_stack: 0 pass
* t_swapcontext: 0 pass
* t_thrd: 0 pass
* t_thread_local_dtor.sh: 0 pass
* t_timedmutex: 0 pass
* t_tss: 0 pass
* weak/t_pthread_weak_nothread: 0 pass
* weak/t_pthread_weak_threaded: 0 pass

#### openbsd-libc: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| arc4random-fork/arc4random-fork | unknown-bodies | oracle-fail | oracle-build:missing-symbol:arc4random | - |
| atexit/atexit_test | unknown-bodies | oracle-fail | oracle-build:missing-header:include/namespace.h | - |
| basename/basename_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcat | - |
| cephes/ieetst | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| db/dbtest | unknown-bodies | oracle-fail | oracle-build:missing-header:db.h | - |
| dirname/dirname_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcat | - |
| env/envtest | run | oracle-fail | oracle-behaviour:expected exit 0, got 2 | - |
| explicit_bzero/explicit_bzero | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C65 |
| ffs/ffs_test | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| fmemopen/fmemopentest | run | oracle-fail | oracle-behaviour:expected exit 0, got 2 | - |
| fnmatch/fnm_test | unknown-bodies | oracle-fail | oracle-build:missing-header:util.h | - |
| fpclassify/fpclassify | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| fread/fread | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| gcvt/gcvt_test | run | oracle-fail | oracle-behaviour:expected exit 0, got 2 | - |
| getaddrinfo/gaitest | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/openbsd/libc/getaddrinfo/gaitest.c:73:63: error: ‘struct sockaddr’ has no member named ‘sa_len’ | - |
| getcap/getcaptest | unknown-bodies | oracle-fail | oracle-build:missing-symbol:cgetnext | - |
| getopt/getopt-test | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| getopt_long/getopt_long_test | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| glob/globtest | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/openbsd/libc/glob/globtest.c:112:16: error: ‘glob_t’ has no member named ‘gl_matchc’; did you mean ‘gl_pathc’? | - |
| hash/hash_test | unknown-bodies | oracle-fail | oracle-build:missing-header:md5.h | - |
| ifnameindex/ifnitest | run | oracle-fail | oracle-behaviour:expected exit 0, got 1 | - |
| ldexp/ldexp_test | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| mkstemp/mkstemp_test | run | oracle-fail | oracle-behaviour:expected exit 0, got 1 | - |
| modf/modf_test | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C47 |
| netdb/netdb | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/openbsd/libc/netdb/netdb.c:103:30: error: storage size of ‘protoent_data’ isn’t known | - |
| open_memstream/open_memstreamtest | run | oracle-fail | oracle-behaviour:expected exit 0, got 1 | - |
| open_wmemstream/open_wmemstreamtest | run | oracle-fail | oracle-behaviour:expected exit 0, got 1 | - |
| orientation/orientation_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:fgetln | - |
| popen/popen | unknown-bodies | oracle-fail | oracle-build:missing-symbol:arc4random | - |
| qsort/qsort_test | unknown-bodies | excluded | excluded:refused by vendor gate (REVIEW.txt): source not vendored: qsort/antiqsort.c | - |
| sigthr/sigthr_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/openbsd/libc/sigthr/sigthr_test.c:29:31: error: ‘SIGTHR’ undeclared (first use in this function); did you mean ‘SIGPWR’? | - |
| sleep/sleep_test | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcpy | - |
| sprintf/sprintf_test | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C56 |
| stpncpy/stpncpy_test | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| strchr/strchrtest | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| strerror/strerror_test | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/openbsd/libc/strerror/strerror_test.c:26:14: error: size of array ‘buffer’ is negative | - |
| strlcat/strlcattest | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcat | - |
| strlcpy/strlcpytest | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strlcpy | - |
| strnlen/strnlentest | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C40 |
| strtod/strtodtest | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C41 |
| strtol/strtoltest | run | oracle-fail | oracle-behaviour:expected exit 0, got 1 | - |
| strtonum/strtonumtest | unknown-bodies | oracle-fail | oracle-build:missing-symbol:strtonum | - |
| telldir/telldir | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C46 |
| timingsafe/timingsafe | unknown-bodies | oracle-fail | oracle-build:missing-symbol:arc4random_buf | - |
| uuid/uuidtest | unknown-bodies | oracle-fail | oracle-build:missing-header:uuid.h | - |
| vis/vis_test | unknown-bodies | oracle-fail | oracle-build:missing-header:vis.h | - |
| wprintf/wfp | run | oracle-fail | oracle-behaviour:expected exit 0, got 1 | - |

Programs of openbsd-libc:

* arc4random-fork/arc4random-fork: 0 pass
* atexit/atexit_test: 0 pass
* basename/basename_test: 0 pass
* cephes/ieetst: 0 pass
* db/dbtest: 0 pass
* dirname/dirname_test: 0 pass
* env/envtest: 0 pass
* explicit_bzero/explicit_bzero: 0 pass
* ffs/ffs_test: 0 pass
* fmemopen/fmemopentest: 0 pass
* fnmatch/fnm_test: 0 pass
* fpclassify/fpclassify: 0 pass
* fread/fread: 0 pass
* gcvt/gcvt_test: 0 pass
* getaddrinfo/gaitest: 0 pass
* getcap/getcaptest: 0 pass
* getopt/getopt-test: 0 pass
* getopt_long/getopt_long_test: 0 pass
* glob/globtest: 0 pass
* hash/hash_test: 0 pass
* ifnameindex/ifnitest: 0 pass
* ldexp/ldexp_test: 0 pass
* mkstemp/mkstemp_test: 0 pass
* modf/modf_test: 0 pass
* netdb/netdb: 0 pass
* open_memstream/open_memstreamtest: 0 pass
* open_wmemstream/open_wmemstreamtest: 0 pass
* orientation/orientation_test: 0 pass
* popen/popen: 0 pass
* qsort/qsort_test: 0 pass
* sigthr/sigthr_test: 0 pass
* sleep/sleep_test: 0 pass
* sprintf/sprintf_test: 0 pass
* stpncpy/stpncpy_test: 0 pass
* strchr/strchrtest: 0 pass
* strerror/strerror_test: 0 pass
* strlcat/strlcattest: 0 pass
* strlcpy/strlcpytest: 0 pass
* strnlen/strnlentest: 0 pass
* strtod/strtodtest: 0 pass
* strtol/strtoltest: 0 pass
* strtonum/strtonumtest: 0 pass
* telldir/telldir: 0 pass
* timingsafe/timingsafe: 0 pass
* uuid/uuidtest: 0 pass
* vis/vis_test: 0 pass
* wprintf/wfp: 0 pass

#### picolibc: rows that are not `pass`
| program | case | verdict | reason | ticket |
|---|---|---|---|---|
| test-argv | run | oracle-fail | oracle-behaviour:expected exit 0, got 77 | - |
| test-assert | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/picolibc/test/test-assert.c:43:32: error: macro "assert" passed 2 arguments, but takes just 1 | - |
| test-atomic | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-crt0-page-table | unknown-bodies | excluded | excluded:board page-table feature, not a libc function | - |
| test-environ | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-except | run | oracle-fail | oracle-behaviour:crash:4 | - |
| test-getdate | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-getopt | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-hello | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-hosted-exit | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-hosted-exit-fail | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-ilp32 | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-ilp32-sub | unknown-bodies | oracle-fail | oracle-build:missing-symbol:main | - |
| test-lseek-overflow | run | oracle-fail | oracle-behaviour:expected exit 0, got 77 | - |
| test-lseek64-linux | run | oracle-fail | oracle-behaviour:expected exit 0, got 77 | - |
| test-raise | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-regex | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-riscv-jvt | unknown-bodies | excluded | excluded:RISC-V board feature, not a host libc function | - |
| test-scmpu | unknown-bodies | excluded | excluded:ARM SCMPU board feature, not a host libc function | - |
| test-setjmp | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-ssize-max | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |
| test-stack-smash | unknown-bodies | oracle-fail | oracle-build:missing-header:ssp/ssp.h | - |
| test-strftime | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| test-thread-safety | unknown-bodies | oracle-fail | oracle-build:build-other:cc: error: unrecognized command-line option ‘-Wthread-safety’ | - |
| test-time | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| test-time-deprecated | unknown-bodies | oracle-fail | oracle-build:missing-symbol:main | - |
| test-time2 | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| test-timegm | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| test-tls | unknown-bodies | oracle-fail | oracle-build:missing-header:picotls.h | - |
| test-ubsan | run | oracle-fail | oracle-behaviour:expected exit 0, got 1 | - |
| test-uchar | unknown-bodies | oracle-fail | oracle-build:build-other:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/tests/libc-oracle/vendor/picolibc/test/test-uchar.c:50:5: error: unknown type name ‘char8_t’ | - |
| test-wcsftime | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C44 |
| test-wctomb | run | not-run | sysroot-incomplete:/home/bo/work/ravenos-workspace/minions/carl/ravenbsd/build/x86_64/sysroot/usr/include/stdio.h | C50 |

Programs of picolibc:

* test-argv: 0 pass
* test-assert: 0 pass
* test-atomic: 0 pass
* test-crt0-page-table: 0 pass
* test-environ: 0 pass
* test-except: 0 pass
* test-getdate: 0 pass
* test-getopt: 0 pass
* test-hello: 0 pass
* test-hosted-exit: 0 pass
* test-hosted-exit-fail: 0 pass
* test-ilp32: 0 pass
* test-ilp32-sub: 0 pass
* test-lseek-overflow: 0 pass
* test-lseek64-linux: 0 pass
* test-raise: 0 pass
* test-regex: 0 pass
* test-riscv-jvt: 0 pass
* test-scmpu: 0 pass
* test-setjmp: 0 pass
* test-ssize-max: 0 pass
* test-stack-smash: 0 pass
* test-strftime: 0 pass
* test-thread-safety: 0 pass
* test-time: 0 pass
* test-time-deprecated: 0 pass
* test-time2: 0 pass
* test-timegm: 0 pass
* test-tls: 0 pass
* test-ubsan: 0 pass
* test-uchar: 0 pass
* test-wcsftime: 0 pass
* test-wctomb: 0 pass
