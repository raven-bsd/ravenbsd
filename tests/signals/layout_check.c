/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Compiled against the freshly packed sysroot for the target architecture. The Rust fixture's
 * local `Sigaction` assertions and this C check must agree with the installed public header; this
 * is what makes the row prove the installed public type rather than a mirror.
 */
#include <signal.h>
#include <stddef.h>

_Static_assert(sizeof(struct sigaction) == 152, "public struct sigaction size");
_Static_assert(offsetof(struct sigaction, __sa_handler) == 0, "public handler offset");
_Static_assert(offsetof(struct sigaction, sa_mask) == 8, "public mask offset");
_Static_assert(offsetof(struct sigaction, sa_flags) == 136, "public flags offset");
_Static_assert(offsetof(struct sigaction, sa_restorer) == 144, "public restorer offset");

int raven_sigaction_layout_check(void) {
    return 0;
}
