/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * CLEANROOM: the POSIX/BSD regex interface. Flag and error values are facts read
 * from the BSD headers (NetBSD/FreeBSD/OpenBSD agree); no donor header text is
 * copied. The engine lives in raven-libc-regex (Spencer, BSD-3).
 */
#ifndef _REGEX_H_
#define _REGEX_H_

#include <sys/cdefs.h>
#include <sys/types.h>

__BEGIN_DECLS

typedef off_t regoff_t;

typedef struct {
	unsigned long __opaque[8];
} regex_t;

typedef struct {
	regoff_t rm_so;
	regoff_t rm_eo;
} regmatch_t;

/* regcomp() flags */
#define REG_BASIC	0x0000
#define REG_EXTENDED	0x0001
#define REG_ICASE	0x0002
#define REG_NOSUB	0x0004
#define REG_NEWLINE	0x0010
#define REG_NOSPEC	0x0020
#define REG_PEND	0x0040
#define REG_DUMP	0x0200
#define REG_GNU		0x0400

/* regexec() flags */
#define REG_NOTBOL	0x0001
#define REG_NOTEOL	0x0002
#define REG_STARTEND	0x0004
#define REG_TRACE	0x0400
#define REG_LARGE	0x1000
#define REG_BACKR	0x2000

/* regerror() special requests */
#define REG_ATOI	255
#define REG_ITOA	0x0400

/* error codes */
#define REG_NOMATCH	1
#define REG_BADPAT	2
#define REG_ECOLLATE	3
#define REG_ECTYPE	4
#define REG_EESCAPE	5
#define REG_ESUBREG	6
#define REG_EBRACK	7
#define REG_EPAREN	8
#define REG_EBRACE	9
#define REG_BADBR	10
#define REG_ERANGE	11
#define REG_ESPACE	12
#define REG_BADRPT	13
#define REG_EMPTY	14
#define REG_ASSERT	15
#define REG_INVARG	16
#define REG_ILLSEQ	17

int	regcomp(regex_t *__restrict, const char *__restrict, int);
size_t	regerror(int, const regex_t *__restrict, char *__restrict, size_t);
int	regexec(const regex_t *__restrict, const char *__restrict, size_t,
	    regmatch_t [], int);
void	regfree(regex_t *);

#if __BSD_VISIBLE
ssize_t	regnsub(char *, size_t, const char *, const regmatch_t *, const char *);
ssize_t	regasub(char **, const char *, const regmatch_t *, const char *);
#endif

__END_DECLS

#endif /* _REGEX_H_ */
