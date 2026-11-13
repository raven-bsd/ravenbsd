/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD CLEANROOM: the bionic donors for this header are not in
 * ravenbsd-resources/donors.lock (only the project workspace lock names the
 * checkout), so the provenance gate rejects a COPY import.  The interface is
 * written from the documented declarations; no donor text is reproduced.
 */

#ifndef _EXECINFO_H_
#define _EXECINFO_H_

#include <sys/cdefs.h>

__BEGIN_DECLS
int backtrace(void **__buffer, int __size);
char **backtrace_symbols(void *const *__buffer, int __size);
void backtrace_symbols_fd(void *const *__buffer, int __size, int __fd);
__END_DECLS

#endif /* _EXECINFO_H_ */
