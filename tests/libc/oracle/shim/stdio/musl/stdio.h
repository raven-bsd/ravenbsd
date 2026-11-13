/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <stdio.h> for the musl stdio oracle, written from scratch.  It declares only
 * what the compiled donor files use; the real FILE image is defined by the replacement
 * <stdio_impl.h>.  The host header is deliberately not included so the donor's FILE type is
 * the only one in each translation unit.
 */
#ifndef RL_STDIO_MUSL_STDIO_H
#define RL_STDIO_MUSL_STDIO_H

#include <stddef.h>
#include <stdarg.h>
#include <sys/types.h>
#include <bits/types/mbstate_t.h>
#include <bits/types/wint_t.h>

#ifndef __FILE_defined
#define __FILE_defined 1
#endif
struct _IO_FILE;
typedef struct _IO_FILE FILE;
typedef struct _IO_FILE __FILE;
typedef long long fpos_t;

#define EOF (-1)
#define BUFSIZ 1024
#define FOPEN_MAX 16
#define L_tmpnam 20
#define FILENAME_MAX 4096
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2
#define _IOFBF 0
#define _IOLBF 1
#define _IONBF 2
#define TMP_MAX 10000

#define hidden
#define weak __attribute__((__weak__))
#define weak_alias(old, new) /* the oracle exports the donor name only */
#define __weak_alias(old, new)

typedef struct {
	ssize_t (*read)(void *, char *, size_t);
	ssize_t (*write)(void *, const char *, size_t);
	int (*seek)(void *, off64_t *, int);
	int (*close)(void *);
} cookie_io_functions_t;

FILE *fopen(const char *, const char *);
FILE *fdopen(int, const char *);
FILE *freopen(const char *, const char *, FILE *);
int fclose(FILE *);
int fflush(FILE *);
size_t fread(void *, size_t, size_t, FILE *);
size_t fwrite(const void *, size_t, size_t, FILE *);
char *fgets(char *, int, FILE *);
int fputs(const char *, FILE *);
int puts(const char *);
int fgetc(FILE *);
int fputc(int, FILE *);
int getc(FILE *);
int putc(int, FILE *);
int getc_unlocked(FILE *);
int putc_unlocked(int, FILE *);
int fgetc_unlocked(FILE *);
int fputc_unlocked(int, FILE *);
int getchar(void);
int putchar(int);
int ungetc(int, FILE *);
int fseek(FILE *, long, int);
long ftell(FILE *);
int fseeko(FILE *, off_t, int);
off_t ftello(FILE *);
void rewind(FILE *);
int fgetpos(FILE *, fpos_t *);
int fsetpos(FILE *, const fpos_t *);
void clearerr(FILE *);
int feof(FILE *);
int ferror(FILE *);
int fileno(FILE *);
int setvbuf(FILE *, char *, int, size_t);
void setbuf(FILE *, char *);
void setbuffer(FILE *, char *, size_t);
void setlinebuf(FILE *);
ssize_t getdelim(char **, size_t *, int, FILE *);
ssize_t getline(char **, size_t *, FILE *);
char *fgetln(FILE *, size_t *);
void flockfile(FILE *);
int ftrylockfile(FILE *);
void funlockfile(FILE *);
void perror(const char *);
int remove(const char *);
int rename(const char *, const char *);
FILE *tmpfile(void);
FILE *fmemopen(void *, size_t, const char *);
FILE *open_memstream(char **, size_t *);
FILE *fopencookie(void *, const char *, cookie_io_functions_t);

#endif
