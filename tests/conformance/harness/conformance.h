/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * Shared guest-side row harness for the conformance tables.
 *
 * A topic probe links this file, calls conform_begin() once, reports every row through
 * conform_check() and ends with conform_finish().  The host runner
 * (tests/conformance/harness/runner.py) parses exactly these lines and owns the process exit
 * mapping, so a failing row is a row result and not an ambiguous qtest exit status.
 *
 * Line contract (one serial line per record):
 *   CONFORM <topic> begin
 *   CONFORM <topic> row=<name> status=PASS|FAIL [detail=<key=value ...>]
 *   CONFORM <topic> summary passed=<n> total=<n> failures=<n>
 *   CONFORM <topic> done failures=<n>
 *   CONFORM-MUTATION topic=<topic> begin=<mutation>
 *   CONFORM-MUTATION topic=<topic> end=<mutation> failures=<n>
 */

#ifndef CONFORMANCE_H
#define CONFORMANCE_H

#include <stdarg.h>

#define CONFORM_MAX_ROWS 64u
#define CONFORM_DETAIL_MAX 512u

struct conform_state {
    const char *topic;
    unsigned total;
    unsigned passed;
    unsigned failed;
};

/* Start a normal row section. */
void conform_begin(struct conform_state *state, const char *topic);

/* Report one row.  `detail` may be NULL; otherwise it is printed verbatim after
 * `detail=`.  The caller formats it with conform_detail() so the row keeps exact values. */
void conform_check(struct conform_state *state, const char *name, int pass,
                   const char *detail);

/* Format a detail string from a printf-style list. */
void conform_detail(char *buffer, unsigned size, const char *format, ...);

/* Print the summary and done lines; returns 0 when every row passed, 1 otherwise. */
int conform_finish(struct conform_state *state);

/* Start/end a controlled-mutation section.  The same rows run against the mutated model. */
void conform_mutation_begin(const char *topic, const char *mutation);
int conform_mutation_end(struct conform_state *state, const char *mutation);

/* Plain diagnostic line for a child/detail that is not a row. */
void conform_note(struct conform_state *state, const char *format, ...);

#endif /* CONFORMANCE_H */
