// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K547 accepted-socket probe. It owns a tap listener and a loopback listener in
// the same boot and reports the accepted fd state through the kernel debug
// writer, so the host tap and guest loopback cases are measured with the same
// libc/netd path. Test fixture only.

#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define TAP_PORT 23147
#define LOOP_PORT 23148
#define PPOLL_PORT 23149

// raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len) -> bytes written.
static long
raven_debug_write(const char *bytes, size_t len)
{
	register long number asm("rax") = 3;
	register const char *pointer asm("rdi") = bytes;
	register size_t length asm("rsi") = len;

	__asm__ volatile("syscall"
	                 : "+r"(number)
	                 : "r"(pointer), "r"(length)
	                 : "rcx", "r11", "memory");
	return number;
}

static void
sayf(const char *format, ...)
{
	char line[320];
	va_list arguments;
	int length;

	va_start(arguments, format);
	length = vsnprintf(line, sizeof(line), format, arguments);
	va_end(arguments);
	if (length <= 0)
		return;
	if ((size_t)length >= sizeof(line))
		length = (int)sizeof(line) - 1;
	(void)raven_debug_write(line, (size_t)length);
}

static int
listen_on(uint32_t address, uint16_t port)
{
	struct sockaddr_in local;
	int fd;
	int one = 1;

	fd = socket(AF_INET, SOCK_STREAM, 0);
	if (fd < 0)
		return -1;
	(void)setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one));
	memset(&local, 0, sizeof(local));
	local.sin_family = AF_INET;
	local.sin_addr.s_addr = address;
	local.sin_port = htons(port);
	if (bind(fd, (struct sockaddr *)&local, sizeof(local)) < 0)
		return -1;
	if (listen(fd, 4) < 0)
		return -1;
	return fd;
}

static int
wait_accept(int listener, int timeout_ms)
{
	struct pollfd waiting = {listener, POLLIN, 0};

	if (poll(&waiting, 1, timeout_ms) <= 0)
		return -1;
	return accept(listener, NULL, NULL);
}

// sshd.c:1015 waits with ppoll(..., NULL, &osigset); reproduce that exact masked wait so a
// listener that works under plain poll and fails here isolates the ppoll/signal-mask boundary.
static int
wait_accept_ppoll(int listener, int timeout_ms)
{
	struct pollfd waiting = {listener, POLLIN, 0};
	struct timespec timeout;
	sigset_t blocked;
	sigset_t previous;
	int ready;

	sigemptyset(&blocked);
	sigaddset(&blocked, SIGCHLD);
	sigprocmask(SIG_BLOCK, &blocked, &previous);
	timeout.tv_sec = timeout_ms / 1000;
	timeout.tv_nsec = (long)(timeout_ms % 1000) * 1000000L;
	ready = ppoll(&waiting, 1, &timeout, &previous);
	sayf("sshd-trace-accept: ppoll ready=%d revents=0x%x errno=%d\n",
	    ready, (unsigned)waiting.revents, errno);
	sigprocmask(SIG_SETMASK, &previous, NULL);
	if (ready <= 0)
		return -1;
	return accept(listener, NULL, NULL);
}

static void
report(const char *tag, int fd)
{
	struct stat status;
	struct sockaddr_in peer;
	socklen_t peer_length = sizeof(peer);
	socklen_t type_length = sizeof(int);
	socklen_t error_length = sizeof(int);
	int type = -1;
	int error = -1;
	int fstat_result;
	int type_result;
	int error_result;
	int peer_result;

	memset(&status, 0, sizeof(status));
	memset(&peer, 0, sizeof(peer));
	fstat_result = fstat(fd, &status);
	type_result = getsockopt(fd, SOL_SOCKET, SO_TYPE, &type, &type_length);
	if (type_result < 0)
		type = -1;
	error_result = getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &error_length);
	if (error_result < 0)
		error = -1;
	peer_result = getpeername(fd, (struct sockaddr *)&peer, &peer_length);
	if (peer_result < 0)
		peer.sin_addr.s_addr = 0xffffffffu;
	sayf("sshd-trace-accept: %s fd=%d fstat=%d is_sock=%d ino=%lu "
	     "getsockopt=%d type=%d error=%d peer=%s:%u\n",
	    tag, fd, fstat_result, S_ISSOCK(status.st_mode), (unsigned long)status.st_ino,
	    type_result, type, error, inet_ntoa(peer.sin_addr), ntohs(peer.sin_port));
}

int
main(void)
{
	int tap_listener;
	int tap_connection;
	int ppoll_listener;
	int ppoll_connection;
	int loop_listener;
	int loop_client;
	int loop_connection;
	ssize_t wrote;
	uint32_t loopback = htonl(INADDR_LOOPBACK);

	tap_listener = listen_on(htonl(INADDR_ANY), TAP_PORT);
	if (tap_listener < 0) {
		sayf("sshd-trace-accept: tap listen errno=%d\n", errno);
		return 1;
	}
	sayf("sshd-trace-accept: tap listening port=%d\n", TAP_PORT);
	tap_connection = wait_accept(tap_listener, 120000);
	if (tap_connection < 0) {
		sayf("sshd-trace-accept: tap accept timeout errno=%d\n", errno);
	} else {
		report("tap", tap_connection);
		wrote = write(tap_connection, "PROBE-TAP\n", 10);
		sayf("sshd-trace-accept: tap write=%ld\n", (long)wrote);
		close(tap_connection);
	}
	close(tap_listener);

	ppoll_listener = listen_on(htonl(INADDR_ANY), PPOLL_PORT);
	if (ppoll_listener < 0) {
		sayf("sshd-trace-accept: ppoll listen errno=%d\n", errno);
		return 1;
	}
	sayf("sshd-trace-accept: ppoll listening port=%d\n", PPOLL_PORT);
	ppoll_connection = wait_accept_ppoll(ppoll_listener, 10000);
	if (ppoll_connection < 0) {
		sayf("sshd-trace-accept: ppoll accept timeout errno=%d\n", errno);
	} else {
		report("ppoll", ppoll_connection);
		wrote = write(ppoll_connection, "PROBE-PPOLL\n", 12);
		sayf("sshd-trace-accept: ppoll write=%ld\n", (long)wrote);
		close(ppoll_connection);
	}
	close(ppoll_listener);

	loop_listener = listen_on(loopback, LOOP_PORT);
	if (loop_listener < 0) {
		sayf("sshd-trace-accept: loop listen errno=%d\n", errno);
		return 1;
	}
	loop_client = socket(AF_INET, SOCK_STREAM, 0);
	if (loop_client < 0) {
		sayf("sshd-trace-accept: loop client errno=%d\n", errno);
		return 1;
	}
	{
		struct sockaddr_in target;

		memset(&target, 0, sizeof(target));
		target.sin_family = AF_INET;
		target.sin_addr.s_addr = loopback;
		target.sin_port = htons(LOOP_PORT);
		if (connect(loop_client, (struct sockaddr *)&target, sizeof(target)) < 0) {
			sayf("sshd-trace-accept: loop connect errno=%d\n", errno);
			return 1;
		}
	}
	loop_connection = accept(loop_listener, NULL, NULL);
	if (loop_connection < 0) {
		sayf("sshd-trace-accept: loop accept errno=%d\n", errno);
		return 1;
	}
	report("loop", loop_connection);
	wrote = write(loop_connection, "PROBE-LOOP\n", 11);
	sayf("sshd-trace-accept: loop write=%ld\n", (long)wrote);
	close(loop_client);
	close(loop_connection);
	close(loop_listener);
	sayf("sshd-trace-accept: done\n");
	return 0;
}
