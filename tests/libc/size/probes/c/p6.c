// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P6: dlopen(argv[1], RTLD_NOW). Static stub (R-16) or dynamic loader.
#include <dlfcn.h>

int main(int argc, char **argv) {
    const char *file = argc > 1 ? argv[1] : "libnone.so";
    void *handle = dlopen(file, RTLD_NOW);
    if (handle == 0) {
        (void)dlerror();
        return 1;
    }
    return 0;
}
