/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C73 libc-dynamic: the interpreter, DT_NEEDED, dlopen/dlsym/dlerror, RTLD_GLOBAL,
 * constructor/destructor ordering, plug TLS and symbol versioning. The case is blocked until
 * PG-49 (spawn with PT_INTERP), PG-2, PG-13 and C62's dlvsym are present; the program still
 * builds and every marker is printed only after its check.
 */

#define RL_CASE "libc-dynamic"
#include "rl_test.h"

#include <dlfcn.h>
#include <link.h>
#include <elf.h>
#include <sys/auxv.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

extern void *dlvsym(void *, const char *, const char *);

struct visit {
    int count;
    int main_seen;
    int libc_seen;
};

static int visit_callback(struct dl_phdr_info *info, size_t size, void *data)
{
    struct visit *visit = data;

    (void)size;
    visit->count++;
    if (info->dlpi_name == NULL || info->dlpi_name[0] == '\0') {
        visit->main_seen = 1;
    } else if (strstr(info->dlpi_name, "libc.so.1") != NULL) {
        visit->libc_seen = 1;
    }
    return 0;
}

struct needed {
    int count;
    int names_ok;
};

static int needed_callback(struct dl_phdr_info *info, size_t size, void *data)
{
    struct needed *needed = data;
    Elf64_Addr string_table = 0;
    const Elf64_Phdr *headers = (const Elf64_Phdr *)(uintptr_t)info->dlpi_phdr;
    const Elf64_Dyn *dynamic = NULL;
    int index;

    (void)size;
    if (info->dlpi_name != NULL && info->dlpi_name[0] != '\0') {
        return 0;
    }
    for (index = 0; index < info->dlpi_phnum; index++) {
        if (headers[index].p_type == PT_DYNAMIC) {
            dynamic = (const Elf64_Dyn *)(uintptr_t)(info->dlpi_addr + headers[index].p_vaddr);
            break;
        }
    }
    if (dynamic == NULL) {
        return 0;
    }
    for (index = 0; dynamic[index].d_tag != DT_NULL; index++) {
        if (dynamic[index].d_tag == DT_STRTAB) {
            string_table = dynamic[index].d_un.d_ptr;
            break;
        }
    }
    if (string_table == 0) {
        return 0;
    }
    needed->names_ok = 1;
    for (index = 0; dynamic[index].d_tag != DT_NULL; index++) {
        if (dynamic[index].d_tag == DT_NEEDED) {
            const char *name = (const char *)(uintptr_t)(string_table + dynamic[index].d_un.d_val);
            needed->count++;
            if (strcmp(name, "libc.so.1") != 0) {
                needed->names_ok = 0;
            }
        }
    }
    return 1;
}

static int check_interp(void)
{
    return getauxval(AT_BASE) != 0 && getauxval(AT_PHDR) != 0 &&
           getauxval(AT_PHENT) == sizeof(Elf64_Phdr) && getauxval(AT_PHNUM) > 0;
}

static int check_needed(void)
{
    struct needed needed = { 0, 0 };

    dl_iterate_phdr(needed_callback, &needed);
    return needed.count == 1 && needed.names_ok;
}

static int count_objects(void)
{
    struct visit visit = { 0, 0, 0 };

    dl_iterate_phdr(visit_callback, &visit);
    return visit.count;
}

static int main_objects_ok(void)
{
    struct visit visit = { 0, 0, 0 };

    dl_iterate_phdr(visit_callback, &visit);
    return visit.main_seen && visit.libc_seen;
}

static void *tls_worker(void *argument)
{
    int (*get)(void) = argument;

    return (void *)(intptr_t)get();
}

int main(void)
{
    void *handle;
    void *global;
    void *symbol;
    int (*add)(int, int);
    int *counter;
    int *inited;
    const char *(*describe)(void);
    int (*tls_get)(void);
    const char *error;
    pthread_t thread;
    void *thread_result = NULL;
    int before;

    rl_out("libc-dynamic: start");
    RL_CHECK("interp", check_interp());
    RL_CHECK("needed", check_needed());

    before = count_objects();
    handle = dlopen("/lib/libplug.so", RTLD_NOW | RTLD_LOCAL);
    RL_CHECK("dlopen", handle != NULL);

    add = (int (*)(int, int))dlsym(handle, "plug_add");
    counter = (int *)dlsym(handle, "plug_counter");
    inited = (int *)dlsym(handle, "plug_inited");
    describe = (const char *(*)(void))dlsym(handle, "plug_describe");
    tls_get = (int (*)(void))dlsym(handle, "plug_tls_get");
    RL_CHECK("dlsym", add != NULL && add(2, 3) == 5 && counter != NULL && *counter == 41 &&
                         describe != NULL && tls_get != NULL &&
                         dlsym(handle, "no_such_symbol") == NULL);
    RL_CHECK("init", inited != NULL && *inited == 1);

    (void)dlerror();
    symbol = dlopen("/lib/missing.so", RTLD_NOW);
    error = dlerror();
    RL_CHECK("error", symbol == NULL && error != NULL && strstr(error, "missing.so") != NULL &&
                          dlerror() == NULL);

    global = dlopen("/lib/libplug.so", RTLD_NOW | RTLD_GLOBAL);
    add = (int (*)(int, int))dlsym(RTLD_DEFAULT, "plug_add");
    RL_CHECK("global", global != NULL && add != NULL && add(1, 1) == 2);
    if (global != NULL) {
        dlclose(global);
    }

    RL_CHECK("libc", describe != NULL && strcmp(describe(), "plug 1.0") == 0);
    RL_CHECK("tls", tls_get != NULL && tls_get() == 11 &&
                        pthread_create(&thread, NULL, tls_worker, (void *)tls_get) == 0 &&
                        pthread_join(thread, &thread_result) == 0 &&
                        (intptr_t)thread_result == 11);

    RL_CHECK("iterate", before == 2 && main_objects_ok() && count_objects() == before + 1);
    if (handle != NULL) {
        RL_CHECK("dlclose", dlclose(handle) == 0);
        RL_CHECK("closed dlsym", dlsym(handle, "plug_add") == NULL && dlerror() != NULL);
    }
    RL_CHECK("version", dlvsym(RTLD_DEFAULT, "strlcpy", "RAVEN_1.0") != NULL);
    rl_skip("security ok", "H-7");
    return rl_finish();
}
