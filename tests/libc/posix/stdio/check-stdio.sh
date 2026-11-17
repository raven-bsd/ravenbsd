#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b stdio behavior evidence: run the lower-level stdio engine tests and verify the exported
# stdio aliases are present in the packaged libc.a.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-stdio

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix stdio: $archive is missing" >&2; exit 1; }

for symbol in \
    clearerr_unlocked ctermid feof_unlocked ferror_unlocked fflush_unlocked fgetc_unlocked \
    fgetpos fgetpos64 fgets_unlocked fileno_unlocked flockfile fopen64 fputc_unlocked \
    fputs_unlocked fread_unlocked freopen64 fseeko fseeko64 fsetpos fsetpos64 ftello \
    ftello64 ftrylockfile funlockfile fwrite_unlocked getc getc_unlocked getchar_unlocked \
    getdelim getw putc putc_unlocked putchar_unlocked putw setbuf setbuffer setlinebuf
do
    if ! nm -g --defined-only "$archive" 2>/dev/null | grep -Eq " $symbol$"; then
        echo "posix stdio: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix stdio: PASS"
