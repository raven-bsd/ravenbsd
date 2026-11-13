// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// B20 static libc fixture: one line to stdout and a clean exit.
//
// `write` is a direct syscall; no stdio, no malloc and no locale setup, so the only libc startup
// work this program needs is the thread pointer (musl's `__init_tls`). The line is exactly
// "hello from linuxd\n" with no prefix, because the qtest asserts it verbatim.

#include <unistd.h>

int main(void)
{
	static const char line[] = "hello from linuxd\n";
	ssize_t written = write(1, line, sizeof(line) - 1);

	return written == (ssize_t)(sizeof(line) - 1) ? 0 : 1;
}
