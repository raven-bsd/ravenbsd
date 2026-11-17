// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// C74l regression target: a normally linked PIE with a global pointer. The pointer forces one
// R_X86_64_RELATIVE relocation, the kind the dynamic-exec fixture does not have. The exit
// status is zero only when the loader applied the relative relocation and main ran.

#include <stdio.h>

static int target = 7;
int *pointer = &target;

int main(void) {
    puts("rtld-relative: main");
    return *pointer == 7 ? 0 : 1;
}
