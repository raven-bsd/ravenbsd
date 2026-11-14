/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libplug: the shared object loaded by libc-dynamic. It exports a function, a data
 * symbol, a constructor/destructor pair and a TLS variable. Its libc calls are resolved
 * through the global scope, which is exactly what the dynamic case tests.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int plug_inited;
int plug_counter = 41;

_Thread_local int plug_tls = 11;

int plug_add(int left, int right)
{
    return left + right;
}

const char *plug_describe(void)
{
    char *text = malloc(16);

    if (text == NULL) {
        return NULL;
    }
    snprintf(text, 16, "plug %s", "1.0");
    return text;
}

int plug_tls_get(void)
{
    return plug_tls;
}

__attribute__((constructor)) static void plug_constructor(void)
{
    plug_inited = 1;
}

__attribute__((destructor)) static void plug_destructor(void)
{
    static const char text[] = "libc-dynamic: plug fini\n";

    (void)write(1, text, sizeof(text) - 1);
}
