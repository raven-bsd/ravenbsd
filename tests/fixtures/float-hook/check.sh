#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C74cu: readelf proof for the optional float plug-in (no QEMU).
#
# The static `printf("%f")`/`strfromd` client is linked against the server libc.a twice:
# with `-u __rl_fmt_float_impl` (the driver request) and without it. The strong member and
# dtoa must be present only with the request; the lazy floor keeps the weak reference and no
# dtoa. The dynamic `file-tools-test` initfs then proves the image driver still links the
# `/bin` program against libc.so.1, which exports the plug-in and dtoa.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

readelf=${READELF:-readelf}
if ! command -v "$readelf" >/dev/null 2>&1; then
    echo "float-hook: readelf not found" >&2
    exit 1
fi

# The server libc.a and the crt objects; the default libc-pack set is `server`.
cargo xtask libc-pack --arch x86_64 >/dev/null

# The dynamic initfs check needs the shared libc and loader staged.
if [ ! -f build/x86_64/libc.so.1 ] || [ ! -f build/x86_64/ld-elf.so.1 ]; then
    cargo xtask libc-link --arch x86_64 >/dev/null
fi

out=build/float-hook
mkdir -p "$out"

link() {
    output=$1
    shift
    rustc --edition 2024 --crate-type bin --target x86_64-unknown-none \
        -C panic=abort -C opt-level=z -C relocation-model=static \
        -C link-arg=--gc-sections -C link-arg=-no-pie \
        "$@" \
        -C link-arg=build/x86_64/crt1.o \
        -C link-arg=build/x86_64/crti.o \
        -C link-arg=build/x86_64/libc.a \
        -C link-arg=build/x86_64/crtn.o \
        -C link-arg=-Tuserland/linker/x86_64.ld \
        -o "$output" tests/fixtures/float-hook/client.rs
}

with_hook="$out/client-with-float-hook"
without_hook="$out/client-without-float-hook"
link "$with_hook" -C link-arg=-u -C link-arg=__rl_fmt_float_impl
link "$without_hook"

defined_hook='GLOBAL[[:space:]]+DEFAULT[[:space:]]+[0-9]+[[:space:]]+__rl_fmt_float_impl$'
if ! "$readelf" -sW "$with_hook" | grep -Eq "$defined_hook"; then
    echo "float-hook: the -u client has no strong __rl_fmt_float_impl" >&2
    exit 1
fi
if ! "$readelf" -sW "$with_hook" | grep -q 'raven_libc_dtoa4dtoa'; then
    echo "float-hook: the -u client has no dtoa" >&2
    exit 1
fi
if "$readelf" -sW "$without_hook" | grep -Eq "$defined_hook"; then
    echo "float-hook: the lazy client unexpectedly defines __rl_fmt_float_impl" >&2
    exit 1
fi
if "$readelf" -sW "$without_hook" | grep -q 'raven_libc_dtoa4dtoa'; then
    echo "float-hook: the lazy client unexpectedly contains dtoa" >&2
    exit 1
fi
if ! "$readelf" -sW "$without_hook" | grep -Eq 'WEAK[[:space:]]+DEFAULT[[:space:]]+UND[[:space:]]+__rl_fmt_float_impl$'; then
    echo "float-hook: the lazy client has no weak plug-in reference" >&2
    exit 1
fi

# The dynamic image path: the `/bin/uname` program is linked as a PIE against libc.so.1,
# whose exported plug-in and local dtoa are the shared-libc half of the same hook.
cargo xtask initfs --arch x86_64 --profile file-tools-test >/dev/null
if ! "$readelf" -d build/x86_64/user/raven-uname | grep -q 'NEEDED.*libc\.so\.1'; then
    echo "float-hook: the dynamic /bin/uname does not need libc.so.1" >&2
    exit 1
fi
if ! "$readelf" -sW build/x86_64/libc.so.1 | grep -Eq 'GLOBAL[[:space:]]+DEFAULT[[:space:]]+[0-9]+[[:space:]]+__rl_fmt_float_impl'; then
    echo "float-hook: libc.so.1 does not export __rl_fmt_float_impl" >&2
    exit 1
fi
if ! "$readelf" -sW build/x86_64/libc.so.1 | grep -q 'raven_libc_dtoa4dtoa'; then
    echo "float-hook: libc.so.1 has no dtoa" >&2
    exit 1
fi

echo "float-hook: static -u pulls __rl_fmt_float_impl + dtoa; lazy floor stays dtoa-free; dynamic libc.so.1 ok"
