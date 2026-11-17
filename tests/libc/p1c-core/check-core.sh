#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1i core-header evidence: compile the declarations this slice fixed under the exact POSIX
# feature macros the posix-check gate uses. The gate's interface probe takes the address of the
# symbol, so every probe below does the same; the macro probe exercises the language features
# that are not libc.a symbols.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cc=${CC:-cc}
include=$root/libc/ravenlibc/include
arch_include=$root/libc/ravenlibc/arch/x86_64/include

if ! command -v "$cc" >/dev/null 2>&1; then
    echo "p1c-core: $cc not found" >&2
    exit 1
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/p1c-core.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

compile() {
    name=$1
    source=$2
    printf '%s\n' "$source" > "$tmp/$name.c"
    if ! "$cc" -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 \
        -Wno-unused-value -I "$include" -I "$arch_include" "$tmp/$name.c"; then
        echo "p1c-core: FAIL $name" >&2
        exit 1
    fi
    echo "p1c-core: ok $name"
}

compile macros '#include <assert.h>
#include <stdarg.h>
#include <stddef.h>
static int first(int count, ...) {
    va_list ap, copy;
    va_start(ap, count);
    va_copy(copy, ap);
    int value = va_arg(copy, int);
    va_end(copy);
    va_end(ap);
    return value;
}
struct sample { int first; int second; };
int main(void) {
    assert(offsetof(struct sample, second) == sizeof(int));
    return first(1, 0);
}'

compile string '#include <string.h>
#include <strings.h>
static void *const probes[] = {
    (void *)strlcpy, (void *)strlcat, (void *)wcslcpy, (void *)wcslcat,
    (void *)flsl, (void *)flsll,
};
int main(void) { return probes[0] == 0; }'

compile sched '#include <sched.h>
static void *const probes[] = {
    (void *)sched_getcpu, (void *)sched_getaffinity, (void *)sched_setaffinity,
};
int main(void) {
    cpu_set_t set;
    return probes[0] == 0 || sizeof set == 0;
}'

compile dlfcn '#include <dlfcn.h>
static void *const probes[] = { (void *)dlclose, (void *)dlinfo };
int main(void) {
    Dl_info info;
    return probes[0] == 0 || info.dli_fname == 0;
}'

compile signal '#include <signal.h>
static void *const probes[] = {
    (void *)sigisemptyset, (void *)sigorset, (void *)sigandset,
    (void *)psignal, (void *)psiginfo,
};
int main(void) { return probes[0] == 0; }'

compile stdlib '#include <stdlib.h>
static void *const probes[] = {
    (void *)reallocarray, (void *)secure_getenv, (void *)strtonum,
    (void *)strtold, (void *)mkstemp, (void *)at_quick_exit, (void *)quick_exit,
    (void *)ecvt_r, (void *)fcvt_r,
};
int main(void) { return probes[0] == 0; }'

compile ctype '#include <ctype.h>
static void *const probe = (void *)iswlower;
int main(void) { return probe == 0; }'

compile socket '#include <sys/socket.h>
static void *const probe = (void *)recvmmsg;
int main(void) { return probe == 0; }'

echo "p1c-core: PASS"
