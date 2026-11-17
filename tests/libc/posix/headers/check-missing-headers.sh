#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1 batch (a) header evidence: compile the headers added to the ravenlibc include root and
# exercise the macro/interface surface the profile names. POSIX.1-2024 feature macros are the
# same ones the posix-check gate uses.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cc=${CC:-cc}
include=$root/libc/ravenlibc/include
arch_include=$root/libc/ravenlibc/arch/x86_64/include

if ! command -v "$cc" >/dev/null 2>&1; then
    echo "posix headers: $cc not found" >&2
    exit 1
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/posix-p1-headers.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

compile() {
    name=$1
    source=$2
    printf '%s\n' "$source" > "$tmp/$name.c"
    if ! "$cc" -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 \
        -I "$include" -I "$arch_include" "$tmp/$name.c"; then
        echo "posix headers: FAIL $name" >&2
        exit 1
    fi
    echo "posix headers: ok $name"
}

compile float '#include <float.h>
int main(void) { return FLT_RADIX > 0 ? 0 : 1; }'

compile iso646 '#include <iso646.h>
int main(void) { int a = 1, b = 3; return (a and b) == 1 && (a or b) == 1 ? 0 : 1; }'

compile libintl '#include <libintl.h>
static void *const probes[] = {
    (void *)gettext, (void *)dgettext, (void *)dcgettext,
    (void *)ngettext, (void *)dngettext, (void *)dcngettext,
    (void *)textdomain, (void *)bindtextdomain, (void *)bind_textdomain_codeset,
};
int main(void) { return probes[0] == 0; }'

compile stdarg '#include <stdarg.h>
static int first(int count, ...) {
    va_list ap;
    va_start(ap, count);
    int value = va_arg(ap, int);
    va_end(ap);
    return value;
}
static int copied(int count, ...) {
    va_list ap, copy;
    va_start(ap, count);
    va_copy(copy, ap);
    int value = va_arg(copy, int);
    va_end(copy);
    va_end(ap);
    return value;
}
int main(void) { return first(1, 0) + copied(1, 0); }'

compile stdatomic '#include <stdatomic.h>
int main(void) { atomic_int value = 0; atomic_store(&value, 1); return atomic_load(&value) == 1 ? 0 : 1; }'

compile stdbool '#include <stdbool.h>
int main(void) { bool value = true; return value == false; }'

compile stddef '#include <stddef.h>
struct sample { int first; int second; };
int main(void) { return offsetof(struct sample, second) == sizeof(int) && NULL == 0 ? 0 : 1; }'

compile tgmath '#include <tgmath.h>
int main(void) { double value = sqrt(4.0); float small = sinf(0.0f); return value > small ? 0 : 1; }'

echo "posix headers: PASS (8 headers)"
