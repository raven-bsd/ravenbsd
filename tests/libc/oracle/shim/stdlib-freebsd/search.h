/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the FreeBSD <search.h> types without FreeBSD's machine headers.
 * The donor sources are compiled against this declaration set.
 */
#ifndef _ORACLE_SEARCH_H
#define _ORACLE_SEARCH_H

#include <stddef.h>

typedef struct entry {
	char *key;
	void *data;
} ENTRY;

typedef enum { FIND, ENTER } ACTION;
typedef enum { preorder, postorder, endorder, leaf } VISIT;

typedef struct __posix_tnode {
	void *key;
	struct __posix_tnode *llink, *rlink;
	signed char balance;
} posix_tnode;

struct __hsearch;
struct hsearch_data {
	struct __hsearch *__hsearch;
};

int hcreate(size_t);
void hdestroy(void);
ENTRY *hsearch(ENTRY, ACTION);
int hcreate_r(size_t, struct hsearch_data *);
void hdestroy_r(struct hsearch_data *);
int hsearch_r(ENTRY, ACTION, ENTRY **, struct hsearch_data *);

void *tdelete(const void * __restrict, posix_tnode ** __restrict,
    int (*)(const void *, const void *));
posix_tnode *tfind(const void *, posix_tnode * const *,
    int (*)(const void *, const void *));
posix_tnode *tsearch(const void *, posix_tnode **,
    int (*)(const void *, const void *));
void twalk(const posix_tnode *, void (*)(const posix_tnode *, VISIT, int));
void tdestroy(void *, void (*)(void *));

#endif
