/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include "conformance.h"

#include <stdio.h>
#include <string.h>

void conform_begin(struct conform_state *state, const char *topic) {
    memset(state, 0, sizeof(*state));
    state->topic = topic;
    printf("CONFORM %s begin\n", topic);
    fflush(stdout);
}

void conform_detail(char *buffer, unsigned size, const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    (void)vsnprintf(buffer, size, format, arguments);
    va_end(arguments);
}

void conform_check(struct conform_state *state, const char *name, int pass,
                   const char *detail) {
    state->total++;
    if (pass) {
        state->passed++;
    } else {
        state->failed++;
    }
    if (detail != NULL && detail[0] != '\0') {
        printf("CONFORM %s row=%s status=%s detail=%s\n", state->topic, name,
               pass ? "PASS" : "FAIL", detail);
    } else {
        printf("CONFORM %s row=%s status=%s\n", state->topic, name,
               pass ? "PASS" : "FAIL");
    }
    fflush(stdout);
}

void conform_note(struct conform_state *state, const char *format, ...) {
    char detail[CONFORM_DETAIL_MAX];
    va_list arguments;
    va_start(arguments, format);
    (void)vsnprintf(detail, sizeof(detail), format, arguments);
    va_end(arguments);
    printf("CONFORM %s note=%s\n", state->topic, detail);
    fflush(stdout);
}

int conform_finish(struct conform_state *state) {
    printf("CONFORM %s summary passed=%u total=%u failures=%u\n", state->topic,
           state->passed, state->total, state->failed);
    printf("CONFORM %s done failures=%u\n", state->topic, state->failed);
    fflush(stdout);
    return state->failed == 0 ? 0 : 1;
}

void conform_mutation_begin(const char *topic, const char *mutation) {
    printf("CONFORM-MUTATION topic=%s begin=%s\n", topic, mutation);
    fflush(stdout);
}

int conform_mutation_end(struct conform_state *state, const char *mutation) {
    printf("CONFORM-MUTATION topic=%s end=%s failures=%u\n", state->topic, mutation,
           state->failed);
    fflush(stdout);
    return state->failed == 0 ? 0 : 1;
}
