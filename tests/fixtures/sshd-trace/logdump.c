// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K547 loopback log filter. Reads /tmp/sshd-trace.log and prints only the
// session-close sequence lines through the kernel debug writer, so the guest
// console never has to cat the full DEBUG3 log (K498 aborted on that read).
// Test fixture only.

#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define LOG_PATH "/tmp/sshd-trace.log"

static const char *const markers[] = {
	"Starting session:",
	"channel 0: read",
	"channel 0: sent",
	"Received SIGCHLD",
	"session_by_pid: pid",
	"session_exit_message: session",
	"session_exit_message: release channel",
	"channel 0: send eof",
	"channel 0: send close",
	"channel 0: rcvd eof",
	"channel 0: rcvd close",
	"Close session:",
	"session_unused",
	"do_cleanup",
	"debug3: send packet: type 98",
	"Server listening",
	"Bind to port",
	"Connection from",
	"Accepted",
	"banner",
	"kex_exchange_identification",
	"sshd-session",
	"fatal",
	"error",
};

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
say(const char *text)
{
	(void)raven_debug_write(text, strlen(text));
}

int
main(void)
{
	char line[512];
	FILE *log;
	unsigned long total = 0;
	unsigned long matched = 0;
	size_t index;

	log = fopen(LOG_PATH, "r");
	if (log == NULL) {
		say("LOGFILE-OPEN-FAILED\n");
		say("LOGDONE\n");
		return 1;
	}
	while (fgets(line, sizeof(line), log) != NULL) {
		total++;
		for (index = 0; index < sizeof(markers) / sizeof(markers[0]); index++) {
			if (strstr(line, markers[index]) != NULL) {
				say("LOGLINE ");
				say(line);
				matched++;
				break;
			}
		}
	}
	fclose(log);
	{
		char summary[96];
		int length = snprintf(summary, sizeof(summary),
		    "LOGLINES total=%lu matched=%lu\n", total, matched);
		if (length > 0)
			(void)raven_debug_write(summary, (size_t)length);
	}
	say("LOGDONE\n");
	return 0;
}
