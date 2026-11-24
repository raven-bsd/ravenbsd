// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S198 test-only ssh-hostkeys helper. The release service runs
// `/bin/rsh -c "mkdir -p /tmp/ssh; ssh-keygen ..."`, but the command child faults at rip=0 on
// this base and `/tmp/ssh` is never created, so sshd exits for lack of host keys. This helper
// creates the directory and execs the staged static ssh-keygen directly. It changes only the
// test image's host-key step; sshd, the probe and the product are unchanged.

#include <errno.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
    if (mkdir("/tmp/ssh", 0700) != 0 && errno != EEXIST) {
        return 1;
    }
    char *args[] = {
        "/bin/ssh-keygen", "-q", "-t", "ed25519", "-N", "",
        "-f", "/tmp/ssh/ssh_host_ed25519_key", (char *)0,
    };
    execv(args[0], args);
    return 1;
}
