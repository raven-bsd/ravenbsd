/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD CLEANROOM: the bionic donor for this header is not in
 * ravenbsd-resources/donors.lock (only the project workspace lock names the
 * checkout), so the provenance gate rejects a COPY import.  The interface is
 * written from the documented declarations; no donor text is reproduced.
 */

#ifndef _ERROR_H_
#define _ERROR_H_

#include <sys/cdefs.h>

__BEGIN_DECLS
extern void (*error_print_progname)(void);
extern unsigned int error_message_count;
extern int error_one_per_line;
void error(int __status, int __errnum, const char *__format, ...) __printflike(3, 4);
void error_at_line(int __status, int __errnum, const char *__filename,
    unsigned int __line_number, const char *__format, ...) __printflike(5, 6);
__END_DECLS

#endif /* _ERROR_H_ */
