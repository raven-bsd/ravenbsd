// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Target-only prelude force-included after `rl_shim.h`. The target `<fcntl.h>` undefines
// `SEEK_SET`/`SEEK_CUR`/`SEEK_END` without defining them again, so include it once here and
// restore the three constants after the undef; the source's later `<fcntl.h>` is guarded.

#ifndef RL_TARGET_PRELUDE_H
#define RL_TARGET_PRELUDE_H

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

#endif /* RL_TARGET_PRELUDE_H */
