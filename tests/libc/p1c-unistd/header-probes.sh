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

cc=${CC:-cc}
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
    if ! "$cc" -I ./libc/ravenlibc/include -I ./libc/ravenlibc/arch/x86_64/include \
        -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 \
        -Wno-unused-value "$src"; then
        echo "FAIL: <$header> does not expose $symbol" >&2
        exit 1
    fi
    count=$((count + 1))
done <"$tmp/pairs"

printf 'p1c-unistd: %s header probes passed\n' "$count"
