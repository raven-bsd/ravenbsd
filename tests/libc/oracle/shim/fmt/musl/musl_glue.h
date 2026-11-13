/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Late glue for the musl printf/scanf oracle, written from scratch.  It is
 * pulled in by the replacement <string.h> after src/internal/stdio_impl.h has
 * defined struct _IO_FILE.  The string-stream cookie callbacks do the real
 * work; these helpers only move bytes.
 */
#ifndef RL_FMT_MUSL_GLUE_H
#define RL_FMT_MUSL_GLUE_H

#ifdef _STDIO_IMPL_H

__attribute__((__weak__)) size_t
__fwritex(const unsigned char *s, size_t l, FILE *f)
{
	if (l == 0 || f->write == 0)
		return (0);
	return f->write(f, s, l);
}

__attribute__((__weak__)) int
__towrite(FILE *f)
{
	f->mode |= f->mode - 1;
	if (f->flags & F_NOWR) {
		f->flags |= F_ERR;
		return (EOF);
	}
	f->rpos = f->rend = 0;
	f->wpos = f->wbase = f->buf;
	f->wend = f->buf + f->buf_size;
	return (0);
}

__attribute__((__weak__)) int
__toread(FILE *f)
{
	f->mode |= f->mode - 1;
	if (f->wpos != f->wbase)
		f->write(f, 0, 0);
	f->wpos = f->wbase = f->wend = 0;
	if (f->flags & F_NORD) {
		f->flags |= F_ERR;
		return (EOF);
	}
	f->rpos = f->rend = f->buf + f->buf_size;
	return (0);
}

__attribute__((__weak__)) int
__overflow(FILE *f, int c)
{
	unsigned char b = (unsigned char)c;
	return f->write(f, &b, 1) == 1 ? c : EOF;
}

__attribute__((__weak__)) int
__uflow(FILE *f)
{
	unsigned char c;
	return f->read(f, &c, 1) == 1 ? c : EOF;
}

__attribute__((__weak__)) int
__lockfile(FILE *f)
{
	(void)f;
	return (0);
}

__attribute__((__weak__)) void
__unlockfile(FILE *f)
{
	(void)f;
}

#endif /* _STDIO_IMPL_H */

#endif
