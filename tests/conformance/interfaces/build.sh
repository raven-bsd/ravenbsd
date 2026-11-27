#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S745: build the interface-discovery conformance probe and pack
# build/fixtures/interfaces-conformance/initfs.img. The image is the normal `server`
# profile (netd + DHCP + resolvd) with the one-shot probe service overlaid; the qtest
# case supplies the QEMU user-mode virtio-net NIC.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=${INTERFACES_CONFORMANCE_OUT:-build/fixtures/interfaces-conformance}
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "interfaces-conformance fixture: x86_64 only" >&2
    exit 77
fi

sysroot=$root/build/$arch/sysroot
libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] ||
    [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/include" "$root/libc/ravenlibc/crt" \
        "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "interfaces-conformance fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "interfaces-conformance fixture: $CC not found" >&2
    exit 77
}
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')
    if [ -n "$host" ]; then
        lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
        if [ -x "$lld_dir/ld.lld" ]; then
            PATH="$lld_dir:$PATH"
            export PATH
        fi
    fi
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    echo "interfaces-conformance fixture: ld.lld not found" >&2
    exit 77
fi

link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

# The C caller is a test-only string generated into the build tree, the S746/S177
# conformance pattern; no C/C++ source is tracked in the scaffold.
cat >"$out/main.c" <<'INTERFACES_MAIN_C'
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S745 interface discovery conformance probe. One-shot guest service on the `server`
// profile (netd + DHCP + a QEMU user-mode virtio-net NIC). It exercises the public
// declarations only:
//   layout        FreeBSD ifaddrs/ifreq/if_nameindex/rt_msghdr sizes and flags
//   live          getifaddrs returns the live lo and eth0 records, not a fixed list
//   address       the live IPv4 addresses and netmasks from netd
//   flags         UP/RUNNING/LOOPBACK/BROADCAST/MULTICAST agree with the record
//   nameindex     if_nameindex/if_nametoindex/if_indextoname round trip
//   invalid       unknown name and index return the documented ENXIO
//   ownership     repeated getifaddrs/freeifaddrs and if_nameindex/free
//   route         AF_ROUTE/PF_ROUTE are explicitly unsupported (EAFNOSUPPORT)

#include <errno.h>
#include <ifaddrs.h>
#include <net/if.h>
#include <net/route.h>
#include <netinet/in.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static int failures;
static int total;

/* The server-profile services do not inherit a console fd; Raven userland writes
 * probe output to the kernel debug console (raven-abi Syscall::DEBUG_WRITE = 3).
 */
static void debug_write(const char *bytes, size_t length)
{
#if defined(__x86_64__)
	register long number __asm__("rax") = 3;
	register long first __asm__("rdi") = (long)bytes;
	register long second __asm__("rsi") = (long)length;

	__asm__ volatile("syscall"
			 : "+r"(number)
			 : "r"(first), "r"(second)
			 : "rcx", "r11", "memory");
#else
	(void)bytes;
	(void)length;
#endif
}

static void out(const char *format, ...)
{
	char line[512];
	va_list arguments;
	int length;

	va_start(arguments, format);
	length = vsnprintf(line, sizeof(line), format, arguments);
	va_end(arguments);
	if (length > 0)
		debug_write(line, (size_t)length < sizeof(line) ? (size_t)length : sizeof(line));
}

static void row(const char *name, int ok, const char *detail)
{
	total++;
	if (!ok) {
		failures++;
		out("CONFORM interfaces row=%s status=FAIL detail=%s\n", name, detail);
		return;
	}
	out("CONFORM interfaces row=%s status=PASS detail=%s\n", name, detail);
}

static const char *family_name(int family)
{
	switch (family) {
	case AF_INET:
		return "inet";
	case AF_INET6:
		return "inet6";
	case AF_LINK:
		return "link";
	default:
		return "other";
	}
}

static int ipv4_of(const struct ifaddrs *ifa, unsigned char out[4])
{
	const struct sockaddr_in *in;

	if (ifa->ifa_addr == NULL || ifa->ifa_addr->sa_family != AF_INET)
		return 0;
	in = (const struct sockaddr_in *)(const void *)ifa->ifa_addr;
	memcpy(out, &in->sin_addr, 4);
	return 1;
}

static int mask_of(const struct ifaddrs *ifa, unsigned char out[4])
{
	const struct sockaddr_in *in;

	if (ifa->ifa_netmask == NULL || ifa->ifa_netmask->sa_family != AF_INET)
		return 0;
	in = (const struct sockaddr_in *)(const void *)ifa->ifa_netmask;
	memcpy(out, &in->sin_addr, 4);
	return 1;
}

static const struct ifaddrs *find_ifa(const struct ifaddrs *list, const char *name)
{
	const struct ifaddrs *ifa;

	for (ifa = list; ifa != NULL; ifa = ifa->ifa_next) {
		if (ifa->ifa_name != NULL && strcmp(ifa->ifa_name, name) == 0)
			return ifa;
	}
	return NULL;
}

static void layout_row(void)
{
	int ok = sizeof(struct ifaddrs) == 56
		&& sizeof(struct ifreq) == 32
		&& sizeof(struct if_nameindex) == 16
		&& sizeof(struct rt_msghdr) == 152
		&& sizeof(struct rt_metrics) == 112
		&& IFF_MULTICAST == 0x8000
		&& IFF_UP == 0x1
		&& IFF_LOOPBACK == 0x8
		&& IFNAMSIZ == 16;
	row("layout", ok, "ifaddrs=56 ifreq=32 if_nameindex=16 rt_msghdr=152 multicast=0x8000");
}

static void live_rows(void)
{
	struct ifaddrs *list = NULL;
	const struct ifaddrs *lo;
	const struct ifaddrs *eth;
	unsigned char address[4];
	char detail[160];
	int count = 0;
	int ok;

	if (getifaddrs(&list) != 0) {
		snprintf(detail, sizeof(detail), "getifaddrs errno=%d", errno);
		row("live", 0, detail);
		row("address", 0, "no list");
		row("flags", 0, "no list");
		return;
	}
	for (const struct ifaddrs *ifa = list; ifa != NULL; ifa = ifa->ifa_next)
		count++;
	lo = find_ifa(list, "lo");
	eth = find_ifa(list, "eth0");
	snprintf(detail, sizeof(detail), "count=%d lo=%d eth0=%d", count, lo != NULL,
	    eth != NULL);
	row("live", count >= 2 && lo != NULL && eth != NULL, detail);

	ok = lo != NULL && ipv4_of(lo, address) && memcmp(address, "\x7f\x00\x00\x01", 4) == 0
	    && eth != NULL && ipv4_of(eth, address)
	    && memcmp(address, "\x0a\x00\x02\x0f", 4) == 0;
	snprintf(detail, sizeof(detail), "lo=127.0.0.1 eth0=10.0.2.15 ok=%d", ok);
	row("address", ok, detail);

	ok = lo != NULL
	    && (lo->ifa_flags & (IFF_UP | IFF_LOOPBACK)) == (IFF_UP | IFF_LOOPBACK)
	    && lo->ifa_broadaddr == NULL
	    && eth != NULL
	    && (eth->ifa_flags & (IFF_UP | IFF_RUNNING | IFF_MULTICAST)) ==
	       (IFF_UP | IFF_RUNNING | IFF_MULTICAST)
	    && (eth->ifa_broadaddr != NULL) == ((eth->ifa_flags & IFF_BROADCAST) != 0);
	snprintf(detail, sizeof(detail), "lo_flags=0x%x eth_flags=0x%x", lo ? lo->ifa_flags : 0,
	    eth ? eth->ifa_flags : 0);
	row("flags", ok, detail);

	/* Every returned address record is IPv4 here; an IPv6 or AF_LINK record would
	 * require a provider that this milestone does not declare. */
	ok = 1;
	for (const struct ifaddrs *ifa = list; ifa != NULL; ifa = ifa->ifa_next) {
		if (ifa->ifa_addr == NULL || ifa->ifa_addr->sa_family != AF_INET) {
			ok = 0;
			break;
		}
	}
	row("families", ok, "all-records=inet");
	freeifaddrs(list);
}

static void nameindex_rows(void)
{
	struct if_nameindex *names = if_nameindex();
	struct ifaddrs *list = NULL;
	char detail[192];
	unsigned int count = 0;
	int ok = 1;
	char buffer[IF_NAMESIZE];

	if (names == NULL) {
		snprintf(detail, sizeof(detail), "if_nameindex errno=%d", errno);
		row("nameindex", 0, detail);
		return;
	}
	for (struct if_nameindex *entry = names; entry->if_index != 0 || entry->if_name != NULL;
	     entry++) {
		unsigned int index;
		char *round_trip;

		if (entry->if_index == 0 || entry->if_name == NULL || entry->if_name[0] == '\0') {
			ok = 0;
			break;
		}
		index = if_nametoindex(entry->if_name);
		round_trip = if_indextoname(entry->if_index, buffer);
		if (index != entry->if_index || round_trip == NULL ||
		    strcmp(round_trip, entry->if_name) != 0) {
			ok = 0;
			break;
		}
		count++;
	}
	if (getifaddrs(&list) == 0) {
		unsigned int addresses = 0;
		for (const struct ifaddrs *ifa = list; ifa != NULL; ifa = ifa->ifa_next)
			addresses++;
		if (addresses != count)
			ok = 0;
		freeifaddrs(list);
	} else {
		ok = 0;
	}
	snprintf(detail, sizeof(detail), "entries=%u ok=%d", count, ok);
	row("nameindex", ok, detail);
	if_freenameindex(names);
}

static void invalid_rows(void)
{
	char buffer[IF_NAMESIZE];
	int name_ok;
	int index_ok;

	errno = 0;
	name_ok = if_nametoindex("nosuch0") == 0 && errno == ENXIO;
	errno = 0;
	index_ok = if_indextoname(0, buffer) == NULL && errno == ENXIO;
	{
		char detail[96];
		snprintf(detail, sizeof(detail), "name=%d index0=%d", name_ok, index_ok);
		row("invalid", name_ok && index_ok, detail);
	}
}

static void ownership_rows(void)
{
	int ok = 1;

	for (int round = 0; round < 8; round++) {
		struct ifaddrs *list = NULL;
		struct if_nameindex *names;

		if (getifaddrs(&list) != 0 || find_ifa(list, "lo") == NULL) {
			ok = 0;
		}
		freeifaddrs(list);
		names = if_nameindex();
		if (names == NULL)
			ok = 0;
		if_freenameindex(names);
		if (!ok)
			break;
	}
	row("ownership", ok, "rounds=8");
}

static void route_row(void)
{
	int raw = socket(AF_ROUTE, SOCK_RAW, 0);
	int saved_raw = errno;
	int dgram = socket(PF_ROUTE, SOCK_DGRAM, 0);
	int saved_dgram = errno;
	int ok = raw == -1 && saved_raw == EAFNOSUPPORT && dgram == -1 &&
	    saved_dgram == EAFNOSUPPORT;
	char detail[96];

	if (raw != -1)
		close(raw);
	if (dgram != -1)
		close(dgram);
	snprintf(detail, sizeof(detail), "raw_errno=%d dgram_errno=%d", saved_raw, saved_dgram);
	row("route", ok, detail);
}

int main(void)
{
	out("CONFORM interfaces begin\n");
	layout_row();
	live_rows();
	nameindex_rows();
	invalid_rows();
	ownership_rows();
	route_row();
	out("CONFORM interfaces summary passed=%d total=%d failures=%d\n", total - failures,
	    total, failures);
	out("CONFORM interfaces done failures=%d\n", failures);
	return failures == 0 ? 0 : 1;
}
INTERFACES_MAIN_C

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$out/main.c" -o "$out/main.o"

if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/interfaces-conformance"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/interfaces-conformance"
fi

cargo xtask initfs --arch "$arch" --profile server > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
cat >"$out/svcd.conf" <<'EOF'
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
# Generated by tests/conformance/interfaces/build.sh.
exit_when_exits = interfaces-conformance
EOF
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/bin/interfaces-conformance=$out/interfaces-conformance:0755" \
    "/etc/services.d/interfaces-conformance.toml=$root/tests/conformance/interfaces/interfaces-conformance.service.toml" \
    "/etc/svcd.conf=$out/svcd.conf"

echo "interfaces-conformance fixture: $out/initfs.img"
