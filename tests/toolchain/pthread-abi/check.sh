#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K158: compare the three target-libc copies of the RavenBSD pthread object sizes and alignments
# with the authoritative raven-libc ABI. Host-only; it does not need the patched toolchain.

set -u

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root" || exit 1

abi=libc/ravenlibc/crates/raven-libc-abi/src/generated/structs.rs
vendor=toolchain/vendor/libc/src/unix/ravenbsd/mod.rs
uutils=userland/uutils/libc-ravenbsd/src/unix/ravenbsd/mod.rs
patch=toolchain/patches/0002-libc-ravenbsd.patch
once=libc/ravenlibc/crates/raven-libc-thread/src/thread/once.rs
spin=libc/ravenlibc/crates/raven-libc-thread/src/spin.rs

for file in "$abi" "$vendor" "$uutils" "$patch" "$once" "$spin"; do
    if [ ! -f "$file" ]; then
        echo "FAIL missing $file" >&2
        exit 1
    fi
done

tmp=$(mktemp "${TMPDIR:-/tmp}/pthread-abi.XXXXXX") || exit 1
trap 'rm -f "$tmp"' EXIT INT TERM
awk '
    /^\+\+\+ b\/library\/libc-ravenbsd\/src\/unix\/ravenbsd\/mod.rs$/ { inside = 1; next }
    inside && /^\+\+\+ b\// { exit }
    inside { print }
' "$patch" >"$tmp"
if [ ! -s "$tmp" ]; then
    echo "FAIL cannot extract the patch ravenbsd module" >&2
    exit 1
fi

abi_size() {
    grep -o "size_of::<$1>() == [0-9][0-9]*" "$abi" 2>/dev/null |
        grep -o '[0-9][0-9]*' | head -1
}
abi_align() {
    grep -o "align_of::<$1>() == [0-9][0-9]*" "$abi" 2>/dev/null |
        grep -o '[0-9][0-9]*' | head -1
}
value() {
    file=$1
    name=$2
    sed -n -E "s/^\+?const _PTHREAD_${name}_SIZE: usize = ([0-9]+);$/\1/p" \
        "$file" 2>/dev/null | head -1
}
align() {
    file=$1
    type=$2
    grep -B3 "pub struct $type" "$file" 2>/dev/null |
        grep -o 'align([0-9][0-9]*)' | tail -1 | tr -dc '0-9'
}

failures=0
fail() {
    echo "FAIL $*" >&2
    failures=$((failures + 1))
}

printf '%-24s %-9s %-9s %-9s %-9s\n' type abi vendor patch uutils
check_type() {
    type=$1
    name=$2
    abi_type=$3
    expected_size=$(abi_size "$abi_type")
    expected_align=$(abi_align "$abi_type")
    if [ -z "$expected_size" ] || [ -z "$expected_align" ]; then
        fail "$type: cannot read $abi_type from the ABI"
        return
    fi
    vendor_size=$(value "$vendor" "$name")
    patch_size=$(value "$tmp" "$name")
    uutils_size=$(value "$uutils" "$name")
    vendor_align=$(align "$vendor" "$type")
    patch_align=$(align "$tmp" "$type")
    uutils_align=$(align "$uutils" "$type")
    printf '%-24s %-9s %-9s %-9s %-9s\n' "$type" \
        "${expected_size}/${expected_align}" \
        "${vendor_size}/${vendor_align}" \
        "${patch_size}/${patch_align}" \
        "${uutils_size}/${uutils_align}"
    [ "$vendor_size" = "$expected_size" ] || fail "$type vendor size $vendor_size != $expected_size"
    [ "$patch_size" = "$expected_size" ] || fail "$type patch size $patch_size != $expected_size"
    [ "$uutils_size" = "$expected_size" ] || fail "$type uutils size $uutils_size != $expected_size"
    [ "$vendor_align" = "$expected_align" ] || fail "$type vendor align $vendor_align != $expected_align"
    [ "$patch_align" = "$expected_align" ] || fail "$type patch align $patch_align != $expected_align"
    [ "$uutils_align" = "$expected_align" ] || fail "$type uutils align $uutils_align != $expected_align"
}

check_type pthread_attr_t ATTR PthreadAttrT
check_type pthread_mutex_t MUTEX PthreadMutexT
check_type pthread_cond_t COND PthreadCondT
check_type pthread_rwlock_t RWLOCK PthreadRwlockT
check_type pthread_barrier_t BARRIER PthreadBarrierT
check_type pthread_mutexattr_t MUTEXATTR PthreadMutexattrT
check_type pthread_condattr_t CONDATTR PthreadCondattrT
check_type pthread_barrierattr_t BARRIERATTR PthreadBarrierattrT
check_type pthread_rwlockattr_t RWLOCKATTR PthreadRwlockattrT

once_size=$(grep -o 'size_of::<Once>() == [0-9][0-9]*' "$once" 2>/dev/null |
    grep -o '[0-9][0-9]*' | head -1)
spin_size=$(grep -o 'size_of::<SpinLock>() == [0-9][0-9]*' "$spin" 2>/dev/null |
    grep -o '[0-9][0-9]*' | head -1)
for spec in "pthread_once_t ONCE $once_size" "pthread_spinlock_t SPINLOCK $spin_size"; do
    set -- $spec
    type=$1
    name=$2
    expected=$3
    if [ -z "$expected" ]; then
        fail "$type: cannot read the libc size"
        continue
    fi
    vendor_size=$(value "$vendor" "$name")
    patch_size=$(value "$tmp" "$name")
    uutils_size=$(value "$uutils" "$name")
    vendor_align=$(align "$vendor" "$type")
    patch_align=$(align "$tmp" "$type")
    uutils_align=$(align "$uutils" "$type")
    printf '%-24s %-9s %-9s %-9s %-9s\n' "$type" "${expected}/4" \
        "${vendor_size}/${vendor_align}" "${patch_size}/${patch_align}" \
        "${uutils_size}/${uutils_align}"
    [ "$vendor_size" = "$expected" ] || fail "$type vendor size $vendor_size != $expected"
    [ "$patch_size" = "$expected" ] || fail "$type patch size $patch_size != $expected"
    [ "$uutils_size" = "$expected" ] || fail "$type uutils size $uutils_size != $expected"
    [ "$vendor_align" = 4 ] || fail "$type vendor align $vendor_align != 4"
    [ "$patch_align" = 4 ] || fail "$type patch align $patch_align != 4"
    [ "$uutils_align" = 4 ] || fail "$type uutils align $uutils_align != 4"
done

copies=$(git grep -l 'const _PTHREAD_ATTR_SIZE' -- '*.rs' 'toolchain/patches/*.patch' 2>/dev/null |
    grep -v -E '/src/unix/redox/mod\.rs$' | wc -l | tr -d ' ')
echo "target-libc copies with the pthread size table: $copies"
if [ "$copies" -ne 3 ]; then
    fail "expected 3 target-libc pthread size copies (vendor, uutils, patch), found $copies"
fi

# The Redox donor modules are frozen; only the RavenBSD copy may carry the 56/40/48 ABI.
if git grep -l 'const _PTHREAD_ATTR_SIZE: usize = 56' -- '*/src/unix/redox/mod.rs' 2>/dev/null |
    grep -q .; then
    fail "a frozen Redox donor module was changed to the Raven pthread sizes"
fi

for file in "$vendor" "$tmp" "$uutils"; do
    if grep -q '__SIZEOF_PTHREAD' "$file" 2>/dev/null; then
        fail "$file contains a second __SIZEOF_PTHREAD size family"
    fi
done

if [ "$failures" -ne 0 ]; then
    echo "pthread-abi: FAIL ($failures divergence)" >&2
    exit 1
fi
echo "pthread-abi: ok (three copies match raven-libc-abi)"
