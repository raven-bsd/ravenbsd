// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Target-only prelude force-included after `rl_shim.h`. The target `<fcntl.h>` undefines
// `SEEK_SET`/`SEEK_CUR`/`SEEK_END` without defining them again, so include it once here and
// restore the three constants after the undef; the source's later `<fcntl.h>` is guarded.

#ifndef RL_TARGET_PRELUDE_H
#define RL_TARGET_PRELUDE_H

#include <stdlib.h>
#include <unistd.h>

#include <fcntl.h>

#ifndef SEEK_SET
#define SEEK_SET 0
#endif
#ifndef SEEK_CUR
#define SEEK_CUR 1
#endif
#ifndef SEEK_END
#define SEEK_END 2
#endif

/*
 * The G1 runner starts a donor body from a writable directory. The current `PROC_EXEC` request
 * carries no working directory, so the wrapper publishes it as `ATF_CWD` and this constructor
 * applies it before `main`. The variable is absent for a normal target program.
 */
static void rl_apply_atf_cwd(void) __attribute__((constructor));
static void
rl_apply_atf_cwd(void)
{
	const char *cwd = getenv("ATF_CWD");

	if (cwd != NULL && cwd[0] == '/')
		(void)chdir(cwd);
}

#endif /* RL_TARGET_PRELUDE_H */
