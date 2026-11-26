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
