#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S745: donor-independent layout and constant check for the declared interface and route
# records. The expected numbers are written from the FreeBSD 133344405 headers
# (`sys/net/if.h`, `sys/net/route.h`, `sys/sys/sockio.h`) selected by DR-69, and are compiled
# against Raven's staged public headers on both architectures.
#
# Usage:
#   sh tests/conformance/interfaces/check.sh                 # normal profile check
#   sh tests/conformance/interfaces/check.sh --mutation NAME  # a wrong belief must not compile
#
# Exit codes: 0 pass (or a mutation was caught), 1 mismatch, 77 clang missing.
set -eu

if ! command -v clang >/dev/null 2>&1; then
	echo "SKIP: clang not found" >&2
	exit 77
fi

ROOT=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/interfaces.XXXXXX")
trap 'rm -rf "$WORK"' EXIT HUP INT TERM
RESOURCE=$(clang -print-resource-dir)

mutation=
if [ "${1:-}" = "--mutation" ]; then
	mutation=${2:-}
	if [ -z "$mutation" ]; then
		echo "usage: check.sh [--mutation NAME]" >&2
		exit 1
	fi
fi

case "$mutation" in
linux-multicast)
	cat >"$WORK/probe.c" <<'EOF'
#include <net/if.h>
_Static_assert(IFF_MULTICAST == 0x1000, "Linux multicast flag");
int main(void) { return 0; }
EOF
	;;
linux-ifreq)
	cat >"$WORK/probe.c" <<'EOF'
#include <net/if.h>
_Static_assert(sizeof(struct ifreq) == 40, "Linux ifreq record");
int main(void) { return 0; }
EOF
	;;
linux-rtentry)
	cat >"$WORK/probe.c" <<'EOF'
#include <net/route.h>
_Static_assert(sizeof(struct rtentry) == 120, "Linux rtentry record");
int main(void) { return 0; }
EOF
	;;
linux-in6-rtmsg)
	cat >"$WORK/probe.c" <<'EOF'
#include <net/route.h>
int main(void) { return (int)sizeof(struct in6_rtmsg); }
EOF
	;;
'')
	cat >"$WORK/probe.c" <<'EOF'
#include <ifaddrs.h>
#include <net/if.h>
#include <net/route.h>

/* Interface records: FreeBSD sys/net/if.h at 133344405. */
_Static_assert(IF_NAMESIZE == 16, "IF_NAMESIZE");
_Static_assert(IFNAMSIZ == 16, "IFNAMSIZ");
_Static_assert(sizeof(struct if_nameindex) == 16, "if_nameindex size");
_Static_assert(__builtin_offsetof(struct if_nameindex, if_index) == 0, "if_index");
_Static_assert(__builtin_offsetof(struct if_nameindex, if_name) == 8, "if_name");
_Static_assert(sizeof(struct ifreq) == 32, "ifreq size");
_Static_assert(__builtin_offsetof(struct ifreq, ifr_name) == 0, "ifr_name");
_Static_assert(__builtin_offsetof(struct ifreq, ifr_ifru) == 16, "ifr_ifru");
_Static_assert(__builtin_offsetof(struct ifreq, ifr_flags) == 16, "ifr_flags");
_Static_assert(__builtin_offsetof(struct ifreq, ifr_index) == 16, "ifr_index");
_Static_assert(__builtin_offsetof(struct ifreq, ifr_mtu) == 16, "ifr_mtu");
_Static_assert(sizeof(struct ifconf) == 16, "ifconf size");
_Static_assert(__builtin_offsetof(struct ifconf, ifc_len) == 0, "ifc_len");
_Static_assert(__builtin_offsetof(struct ifconf, ifc_ifcu) == 8, "ifc_ifcu");
_Static_assert(sizeof(struct if_data) == 152, "if_data size");
_Static_assert(__builtin_offsetof(struct if_data, ifi_type) == 0, "ifi_type");
_Static_assert(__builtin_offsetof(struct if_data, ifi_mtu) == 8, "ifi_mtu");
_Static_assert(__builtin_offsetof(struct if_data, ifi_baudrate) == 16, "ifi_baudrate");
_Static_assert(__builtin_offsetof(struct if_data, ifi_hwassist) == 120, "ifi_hwassist");
_Static_assert(sizeof(struct if_msghdr) == 168, "if_msghdr size");
_Static_assert(__builtin_offsetof(struct if_msghdr, ifm_data) == 16, "ifm_data");
_Static_assert(sizeof(struct ifa_msghdr) == 20, "ifa_msghdr size");
_Static_assert(__builtin_offsetof(struct ifa_msghdr, ifam_addrs) == 4, "ifam_addrs");
_Static_assert(__builtin_offsetof(struct ifa_msghdr, ifam_flags) == 8, "ifam_flags");
_Static_assert(__builtin_offsetof(struct ifa_msghdr, ifam_index) == 12, "ifam_index");
_Static_assert(__builtin_offsetof(struct ifa_msghdr, ifam_metric) == 16, "ifam_metric");
_Static_assert(sizeof(struct if_announcemsghdr) == 24, "if_announcemsghdr size");
_Static_assert(__builtin_offsetof(struct if_announcemsghdr, ifan_index) == 4, "ifan_index");
_Static_assert(__builtin_offsetof(struct if_announcemsghdr, ifan_name) == 6, "ifan_name");
_Static_assert(__builtin_offsetof(struct if_announcemsghdr, ifan_what) == 22, "ifan_what");

/* Interface flags: FreeBSD values, never the Linux 0x1000 multicast bit. */
_Static_assert(IFF_UP == 0x1, "IFF_UP");
_Static_assert(IFF_BROADCAST == 0x2, "IFF_BROADCAST");
_Static_assert(IFF_DEBUG == 0x4, "IFF_DEBUG");
_Static_assert(IFF_LOOPBACK == 0x8, "IFF_LOOPBACK");
_Static_assert(IFF_POINTOPOINT == 0x10, "IFF_POINTOPOINT");
_Static_assert(IFF_DRV_RUNNING == 0x40, "IFF_DRV_RUNNING");
_Static_assert(IFF_RUNNING == 0x40, "IFF_RUNNING");
_Static_assert(IFF_MULTICAST == 0x8000, "IFF_MULTICAST");
_Static_assert(IFF_CANTCONFIG == 0x10000, "IFF_CANTCONFIG");

/* Route records: FreeBSD sys/net/route.h at 133344405. */
_Static_assert(sizeof(struct rt_metrics) == 112, "rt_metrics size");
_Static_assert(sizeof(struct rt_msghdr) == 152, "rt_msghdr size");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_msglen) == 0, "rtm_msglen");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_version) == 2, "rtm_version");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_type) == 3, "rtm_type");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_index) == 4, "rtm_index");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_flags) == 8, "rtm_flags");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_addrs) == 12, "rtm_addrs");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_pid) == 16, "rtm_pid");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_seq) == 20, "rtm_seq");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_errno) == 24, "rtm_errno");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_fmask) == 28, "rtm_fmask");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_inits) == 32, "rtm_inits");
_Static_assert(__builtin_offsetof(struct rt_msghdr, rtm_rmx) == 40, "rtm_rmx");
_Static_assert(RTM_VERSION == 5, "RTM_VERSION");
_Static_assert(RTM_ADD == 0x1, "RTM_ADD");
_Static_assert(RTM_GET == 0x4, "RTM_GET");
_Static_assert(RTM_NEWADDR == 0xc, "RTM_NEWADDR");
_Static_assert(RTM_IFINFO == 0xe, "RTM_IFINFO");
_Static_assert(RTM_IFANNOUNCE == 0x11, "RTM_IFANNOUNCE");
_Static_assert(RTF_UP == 0x1, "RTF_UP");
_Static_assert(RTF_GATEWAY == 0x2, "RTF_GATEWAY");
_Static_assert(RTF_HOST == 0x4, "RTF_HOST");
_Static_assert(RTF_REJECT == 0x8, "RTF_REJECT");
_Static_assert(RTF_STATIC == 0x800, "RTF_STATIC");
_Static_assert(RTA_DST == 0x1, "RTA_DST");
_Static_assert(RTA_GATEWAY == 0x2, "RTA_GATEWAY");
_Static_assert(RTAX_MAX == 8, "RTAX_MAX");

/*
 * The Linux rtentry/in6_rtmsg records are checked by the linux-rtentry and
 * linux-in6-rtmsg mutations: an incomplete or removed record must not compile
 * against the native header.
 */

/* ifaddrs.h: FreeBSD layout shared by the declared discovery API. */
_Static_assert(sizeof(struct ifaddrs) == 56, "ifaddrs size");
_Static_assert(__builtin_offsetof(struct ifaddrs, ifa_next) == 0, "ifa_next");
_Static_assert(__builtin_offsetof(struct ifaddrs, ifa_name) == 8, "ifa_name");
_Static_assert(__builtin_offsetof(struct ifaddrs, ifa_flags) == 16, "ifa_flags");
_Static_assert(__builtin_offsetof(struct ifaddrs, ifa_addr) == 24, "ifa_addr");
_Static_assert(__builtin_offsetof(struct ifaddrs, ifa_netmask) == 32, "ifa_netmask");
_Static_assert(__builtin_offsetof(struct ifaddrs, ifa_ifu) == 40, "ifa_ifu");
_Static_assert(__builtin_offsetof(struct ifaddrs, ifa_data) == 48, "ifa_data");

int main(void) { return 0; }
EOF
	;;
*)
	echo "unknown mutation: $mutation" >&2
	exit 1
	;;
esac

for arch in x86_64 aarch64; do
	if clang --target="$arch-unknown-ravenbsd" -nostdinc \
		-isystem "$RESOURCE/include" \
		-isystem "$ROOT/libc/ravenlibc/include" \
		-isystem "$ROOT/libc/ravenlibc/abi/generated/include" \
		-isystem "$ROOT/libc/ravenlibc/arch/$arch/include" \
		-std=gnu11 -Werror -fsyntax-only "$WORK/probe.c" 2>"$WORK/$arch.log"; then
		compiled=0
	else
		compiled=1
	fi
	if [ -n "$mutation" ]; then
		if [ "$compiled" -eq 0 ]; then
			echo "FAIL: mutation $mutation was not caught on $arch" >&2
			exit 1
		fi
		echo "ok: mutation $mutation caught on $arch"
	else
		if [ "$compiled" -ne 0 ]; then
			echo "FAIL: $arch public interface/route records do not match the FreeBSD profile" >&2
			sed -n '1,20p' "$WORK/$arch.log" >&2
			exit 1
		fi
		echo "ok: $arch interface/route records"
	fi
done
