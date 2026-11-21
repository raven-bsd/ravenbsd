/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Synthetic donor fixture for the RACK coverage parser. It exercises prototypes, macros with
 * braces, comments and strings; it is not FreeBSD source.
 */

#define FAKE_BLOCK { if (0) { } }
#define MULTI_LINE(x) \
    { x; }

static int helper(int value);
int public_fn(const char *text, int length);

static int
helper(int value)
{
    if (value > 0) {
        return 1;
    }
    return 0;
}

int
public_fn(const char *text,
          int length)
{
    /* a } in a comment must not close a body */
    const char *marker = "a { in a string";
    if (helper(length) && text != marker) {
        return length;
    }
    return 0;
}
