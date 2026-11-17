#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1c locale/names evidence:
#   * the seven pure-crate unit suites;
#   * the 51-row POSIX/XSI compile probe;
#   * a defined-symbol check for every claimed row in the target staticlib.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

cargo nextest run \
    -p raven-libc-locale \
    -p raven-libc-iconv \
    -p raven-libc-nss \
    -p raven-libc-net \
    -p raven-libc-regex \
    -p raven-libc-pwd \
    -p raven-libc-fts

cc -I libc/ravenlibc/include \
   -I libc/ravenlibc/arch/x86_64/include \
   -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 \
   -Wno-unused-value tests/libc/p1c-locale/probe.c

cargo build -p raven-libc --target x86_64-unknown-none
archive=target/x86_64-unknown-none/debug/libraven_libc.a
[ -f "$archive" ] || { echo "p1c locale: $archive is missing" >&2; exit 1; }

for symbol in \
    basename c16rtomb c32rtomb c8rtomb catclose catgets catopen closelog dirname fmtmsg \
    hcreate hcreate_r hdestroy hdestroy_r hsearch_r iconv iconv_close iconv_open \
    mbrtoc16 mbrtoc32 mbrtoc8 openlog setlogmask strfmon strfmon_l syslog umaxabs vsyslog \
    if_freenameindex if_indextoname if_nameindex if_nametoindex \
    putgrent getgrouplist setgroups initgroups setgroupent putpwent setpassent \
    regnsub regasub ftw64 nftw64 updwtmpx utmpxname \
    gethostbyname_r gethostbyname2_r gethostbyaddr_r getservbyname_r getservbyport_r herror
do
    if ! nm -g --defined-only "$archive" 2>/dev/null | grep -Eq " $symbol$"; then
        echo "p1c locale: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "P1c locale/names: PASS"
