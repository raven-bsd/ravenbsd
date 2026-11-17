#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1c-unistd compile probes. Each pair is a header and an interface that must be visible under
# _POSIX_C_SOURCE/_XOPEN_SOURCE (the same flags the POSIX gate uses). The Raven 64-bit spellings
# alias the native 64-bit off_t.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/p1c-unistd.XXXXXX")
trap 'rm -rf "$tmp"' 0 1 2 15

arch=x86_64
if [ "${1:-}" = "--arch" ]; then
    [ "$#" -ge 2 ] || { echo "usage: $0 [--arch x86_64|aarch64]" >&2; exit 2; }
    arch=$2
    shift 2
fi
case "$arch" in
    x86_64)
        cc=${CC:-cc}
        arch_include=libc/ravenlibc/arch/x86_64/include
        ldbl_mant_dig=64
        ;;
    aarch64)
        cc=${AARCH64_CC:-aarch64-linux-gnu-gcc}
        arch_include=libc/ravenlibc/arch/aarch64/include
        ldbl_mant_dig=113
        ;;
    *)
        echo "p1c-unistd: unknown arch: $arch" >&2
        exit 2
        ;;
esac

cat >"$tmp/pairs" <<'EOF'
dirent.h versionsort
fcntl.h creat64
fcntl.h open64
poll.h ppoll
sys/resource.h prlimit
sys/stat.h fchownat
sys/stat.h fstat64
sys/stat.h lstat64
sys/stat.h stat64
sys/wait.h wait3
sys/wait.h wait4
unistd.h chroot
unistd.h copy_file_range
unistd.h daemon
unistd.h endusershell
unistd.h execvpe
unistd.h getentropy
unistd.h getresgid
unistd.h getresuid
unistd.h issetugid
unistd.h lockf
unistd.h lseek64
unistd.h pread64
unistd.h pwrite64
unistd.h setresgid
unistd.h setresuid
unistd.h setusershell
unistd.h syscall
unistd.h vfork
EOF

count=0
while read -r header symbol; do
    [ -n "$header" ] || continue
    src=$tmp/probe_$count.c
    cat >"$src" <<EOF
#include <$header>
static void *const probe = (void *)&$symbol;
int main(void) { return 0; }
EOF
    if ! "$cc" -I ./libc/ravenlibc/include -I ./"$arch_include" \
        -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 \
        -Wno-unused-value "$src"; then
        echo "FAIL: <$header> does not expose $symbol ($arch)" >&2
        exit 1
    fi
    count=$((count + 1))
done <"$tmp/pairs"

check() {
    name=$1
    src=$tmp/check_$count.c
    cat >"$src"
    if ! "$cc" -I ./libc/ravenlibc/include -I ./"$arch_include" \
        -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 \
        -Wno-unused-value "$src"; then
        echo "FAIL: check $name ($arch)" >&2
        exit 1
    fi
    count=$((count + 1))
}

check char-signedness <<'EOF'
#include <limits.h>
#if defined(__CHAR_UNSIGNED__)
_Static_assert(CHAR_MIN == 0 && CHAR_MAX == 255, "unsigned char limits");
#else
_Static_assert(CHAR_MIN == -128 && CHAR_MAX == 127, "signed char limits");
#endif
int main(void) { return 0; }
EOF

check wchar-limits <<'EOF'
#include <stddef.h>
#include <wchar.h>
_Static_assert((wchar_t)-1 > 0 ? WCHAR_MIN == 0 : WCHAR_MIN < 0, "wchar min");
int main(void) { return 0; }
EOF

check long-double <<EOF
#include <float.h>
_Static_assert(sizeof(long double) == 16, "long double size");
_Static_assert(LDBL_MANT_DIG == $ldbl_mant_dig, "long double mantissa");
int main(void) { return 0; }
EOF

check float-double-t <<'EOF'
#include <math.h>
_Static_assert(sizeof(float_t) == sizeof(float), "float_t");
_Static_assert(sizeof(double_t) == sizeof(double), "double_t");
int main(void) { return 0; }
EOF

check long-double-prototypes <<'EOF'
#include <math.h>
#include <stdlib.h>
static long double (*const acosl_probe)(long double) = acosl;
static long double (*const strtold_probe)(const char *, char **) = strtold;
int main(void) { return 0; }
EOF

check wchar-prototypes <<'EOF'
#include <stddef.h>
#include <wchar.h>
static size_t (*const wcslen_probe)(const wchar_t *) = wcslen;
static wchar_t *(*const wmemset_probe)(wchar_t *, wchar_t, size_t) = wmemset;
int main(void) { return 0; }
EOF

printf 'p1c-unistd: %s header probes passed (%s)\n' "$count" "$arch"
