/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: the gettext message-catalog interface used by the base utilities and brush. The
 * declarations follow the GNU gettext ABI implemented by raven-libc-locale; the POSIX profile
 * carries the same names for tool consumers.
 */
#ifndef _RAVENBSD_LIBINTL_H
#define _RAVENBSD_LIBINTL_H

#include <sys/cdefs.h>

__BEGIN_DECLS

char *gettext(const char *);
char *dgettext(const char *, const char *);
char *dcgettext(const char *, const char *, int);
char *ngettext(const char *, const char *, unsigned long int);
char *dngettext(const char *, const char *, const char *, unsigned long int);
char *dcngettext(const char *, const char *, const char *, unsigned long int, int);
char *textdomain(const char *);
char *bindtextdomain(const char *, const char *);
char *bind_textdomain_codeset(const char *, const char *);

__END_DECLS

#endif /* _RAVENBSD_LIBINTL_H */
