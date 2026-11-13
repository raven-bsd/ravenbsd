/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/search.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 * plus the FreeBSD AVL and `hsearch_r` declarations of doc 03 sections 3.10 and the X17 ring-1
 * `_r` family.
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef _SEARCH_H
#define _SEARCH_H

#include <sys/cdefs.h>

#define __NEED_size_t
#include <bits/alltypes.h>

__BEGIN_DECLS

typedef enum { FIND, ENTER } ACTION;
typedef enum { preorder, postorder, endorder, leaf } VISIT;

typedef struct entry {
	char *key;
	void *data;
} ENTRY;

int hcreate (size_t);
void hdestroy (void);
ENTRY *hsearch (ENTRY, ACTION);

#if __BSD_VISIBLE || defined(_GNU_SOURCE)
struct hsearch_data {
	void *__tab;
	unsigned int __unused1;
	unsigned int __unused2;
};

int hcreate_r (size_t, struct hsearch_data *);
void hdestroy_r (struct hsearch_data *);
int hsearch_r (ENTRY, ACTION, ENTRY **, struct hsearch_data *);
#endif

void insque (void *, void *);
void remque (void *);

void *lsearch (const void *, void *, size_t *, size_t,
	int (*)(const void *, const void *));
void *lfind (const void *, const void *, size_t *, size_t,
	int (*)(const void *, const void *));

void *tdelete (const void *__restrict, void **__restrict,
	int (*)(const void *, const void *));
void *tfind (const void *, void *const *, int (*)(const void *, const void *));
void *tsearch (const void *, void **, int (*)(const void *, const void *));
void twalk (const void *, void (*)(const void *, VISIT, int));

#if __BSD_VISIBLE || defined(_GNU_SOURCE)
struct qelem {
	struct qelem *q_forw, *q_back;
	char q_data[1];
};

void tdestroy (void *, void (*)(void *));
#endif

__END_DECLS

#endif /* _SEARCH_H */
