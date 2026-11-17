/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * P2r libc native-spawn child: verify the kernel command name the native spawner set before
 * this thread started.
 */

#define RL_CASE "libc-spawn-std-child"
#include "rl_test.h"

#include <string.h>

#if defined(__x86_64__)
static long rl_proc_self_info(unsigned long long *words, unsigned long count)
{
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(0x35L), "D"(words), "S"(count)
                     : "rcx", "r11", "memory");
    return result;
}
#else
#error "libc-spawn-std-child is an x86_64 case"
#endif

int main(void)
{
    unsigned long long words[41] = { 0 };

    RL_REQUIRE(rl_proc_self_info(words, 41) == 0);
    RL_CHECK("cmd", strcmp((const char *)&words[8], "libc-spawn-std-child") == 0);
    return rl_finish();
}
