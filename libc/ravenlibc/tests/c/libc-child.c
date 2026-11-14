/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-child: the initfs child used by libc-spawn. It opens the console itself because
 * the descriptor table is not inherited today, prints one line and exits with RL_EXIT (7 by
 * default). When RL_SYNC=1 it waits for the parent's /tmp/rl-spawn-go file so the parent's
 * `spawn ok` marker is printed first.
 */

#define RL_CASE "libc-child"
#include "rl_test.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <time.h>
#include <fcntl.h>

static void wait_for_parent(void)
{
    struct timespec pause;
    int index;

    pause.tv_sec = 0;
    pause.tv_nsec = 10 * 1000 * 1000;
    for (index = 0; index < 200; index++) {
        int fd = open("/tmp/rl-spawn-go", O_RDONLY);
        if (fd >= 0) {
            close(fd);
            return;
        }
        (void)nanosleep(&pause, NULL);
    }
}

int main(int argc, char **argv)
{
    const char *exit_text = getenv("RL_EXIT");
    const char *quiet = getenv("RL_QUIET");
    const char *sync = getenv("RL_SYNC");
    const char *env_value = getenv("RL");
    char line[160];

    if (sync != NULL) {
        wait_for_parent();
    }
    if (quiet == NULL) {
        snprintf(line, sizeof(line), "libc-child: argc=%d argv1=%s argv2=%s RL=%s", argc,
                 argc > 1 ? argv[1] : "(missing)", argc > 2 ? argv[2] : "(missing)",
                 env_value != NULL ? env_value : "(missing)");
        rl_out(line);
    }
    if (exit_text != NULL) {
        return atoi(exit_text);
    }
    return 7;
}
