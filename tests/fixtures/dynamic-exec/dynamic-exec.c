// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// S81 dynamic-exec target: a normally linked PIE with PT_INTERP and DT_NEEDED libc.so.1.
// The exit status is zero only when `strlen` resolved through the dynamic loader and `main`
// ran; a failed load/crash cannot produce this status.

#include <string.h>

int main(void) {
    return strlen("dynamic-exec") == 12 ? 0 : 1;
}
