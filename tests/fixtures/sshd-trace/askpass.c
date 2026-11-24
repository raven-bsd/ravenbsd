// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K547 SSH_ASKPASS helper for the guest-loopback control. The Raven kernel
// executes ELF only, so this is a binary rather than a script. It is a test
// fixture, not a product credential: the image password is in profiles/ssh.d.

#include <unistd.h>

int
main(void)
{
	static const char password[] = "raven\n";

	return write(STDOUT_FILENO, password, sizeof(password) - 1) ==
	    (long)(sizeof(password) - 1) ? 0 : 1;
}
