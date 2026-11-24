// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S199 guarded-buffer regression for the SIGACTION seam (x86_64). Test-only.
//
// The kernel 0x87 request is the S160 40-byte layout (kernel/src/syscall/thread.rs
// `UserSigAction`). This probe places that request between canary words, passes a non-null `act`
// and a non-null `old`, and asserts every canary is unchanged. The pre-aef6f7f58 kernel copied the
// 152-byte C `struct sigaction`; it overwrote the 112 bytes after the 40-byte `old` and reported
// flags from the wrong offset, so the guarded rows turn red on that kernel and stay green here.
//
// The probe talks to the kernel directly: libc's `sigaction(3)` builds the 40-byte request on its
// own stack, where this test could not guard it. The raw `syscall` instruction is the documented
// raven-abi convention (number in rax, args in rdi/rsi/rdx; see raven-libc-sys arch/x86_64.rs).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define SIGUSR1 10
#define SIGUSR2 12
#define SIGACTION_SYSCALL ((long)0x87)
#define SIG_DFL ((uint64_t)0)
#define SA_RESTART 0x10000000u

#define CANARY_WORDS 8

// The kernel request: handler 0, mask 8, flags 24, restorer 32 (40 bytes).
struct kernel_sigaction {
    uint64_t handler;
    uint64_t mask[2];
    uint32_t flags;
    uint32_t pad;
    uint64_t restorer;
};

_Static_assert(sizeof(struct kernel_sigaction) == 40, "the SIGACTION request is 40 bytes");
_Static_assert(offsetof(struct kernel_sigaction, mask) == 8, "mask offset");
_Static_assert(offsetof(struct kernel_sigaction, flags) == 24, "flags offset");
_Static_assert(offsetof(struct kernel_sigaction, restorer) == 32, "restorer offset");

// The request sits between two canary arrays. `tail` only exists so a wrong-width kernel read of
// 152 bytes stays inside mapped memory and sees deterministic zeroes at the old flags/restorer
// offsets; the canaries that a wrong-width write damages are still immediately after the request.
struct guarded {
    uint64_t before[CANARY_WORDS];
    struct kernel_sigaction act;
    uint64_t after[CANARY_WORDS];
    uint64_t tail[16];
};

static uint64_t canary_word(size_t index) {
    return 0xC0DEC0DE00000000ULL ^ ((uint64_t)index * 0x0101010101010101ULL);
}

static void guard_init(struct guarded *guard) {
    size_t index;
    for (index = 0; index < CANARY_WORDS; index++) {
        guard->before[index] = canary_word(index);
        guard->after[index] = canary_word(CANARY_WORDS + index);
    }
    memset(&guard->act, 0, sizeof(guard->act));
    memset(guard->tail, 0, sizeof(guard->tail));
}

// Return the number of damaged canary words; print the exact word and values for each one.
static int guard_check(const struct guarded *guard, const char *side) {
    int bad = 0;
    size_t index;
    for (index = 0; index < CANARY_WORDS; index++) {
        uint64_t want = canary_word(index);
        if (guard->before[index] != want) {
            printf("sigact: canary-bad side=%s-before word=%zu expected=0x%llx got=0x%llx\n",
                   side, index, (unsigned long long)want,
                   (unsigned long long)guard->before[index]);
            bad++;
        }
    }
    for (index = 0; index < CANARY_WORDS; index++) {
        uint64_t want = canary_word(CANARY_WORDS + index);
        if (guard->after[index] != want) {
            printf("sigact: canary-bad side=%s-after word=%zu expected=0x%llx got=0x%llx\n",
                   side, index, (unsigned long long)want,
                   (unsigned long long)guard->after[index]);
            bad++;
        }
    }
    return bad;
}

static const char *verdict(int ok) {
    return ok ? "ok" : "bad";
}

static long raw_sigaction(int signo, const void *act, void *old) {
    long result;
    __asm__ volatile(
        "syscall"
        : "=a"(result)
        : "a"(SIGACTION_SYSCALL), "D"((long)signo), "S"(act), "d"(old)
        : "rcx", "r11", "memory");
    return result;
}

// Test-only handler: its address is the user handler value the round-trip must return.
static void sigusr1_handler(int signo) {
    (void)signo;
}

int main(void) {
    struct guarded new_guard;
    struct guarded old_guard;
    const uint64_t handler_value = (uint64_t)(uintptr_t)sigusr1_handler;
    const uint64_t mask_value = 1ULL << (SIGUSR2 - 1);
    int failures = 0;
    long result;

    printf("sigact: layout size=%zu handler=%zu mask=%zu flags=%zu restorer=%zu\n",
           sizeof(struct kernel_sigaction),
           offsetof(struct kernel_sigaction, handler),
           offsetof(struct kernel_sigaction, mask),
           offsetof(struct kernel_sigaction, flags),
           offsetof(struct kernel_sigaction, restorer));
    if (sizeof(struct kernel_sigaction) != 40 || offsetof(struct kernel_sigaction, mask) != 8 ||
        offsetof(struct kernel_sigaction, flags) != 24 ||
        offsetof(struct kernel_sigaction, restorer) != 32) {
        failures++;
    }

    // Row 1: install a caught SIGUSR1 action from a guarded 40-byte request (non-null act).
    guard_init(&new_guard);
    new_guard.act.handler = handler_value;
    new_guard.act.mask[0] = mask_value;
    new_guard.act.flags = SA_RESTART;
    result = raw_sigaction(SIGUSR1, &new_guard.act, NULL);
    {
        int bad = guard_check(&new_guard, "install");
        printf("sigact: install ret=%ld new-before=%s new-after=%s\n", result,
               verdict(bad == 0), verdict(bad == 0));
        if (result != 0 || bad != 0) {
            failures++;
        }
    }

    // Row 2: query with non-null act and non-null old; the previous action must round-trip.
    guard_init(&old_guard);
    result = raw_sigaction(SIGUSR1, &new_guard.act, &old_guard.act);
    {
        int new_bad = guard_check(&new_guard, "query-new");
        int old_bad = guard_check(&old_guard, "query-old");
        int handler_ok = old_guard.act.handler == handler_value;
        int mask_ok = old_guard.act.mask[0] == mask_value && old_guard.act.mask[1] == 0;
        int flags_ok = old_guard.act.flags == SA_RESTART;
        printf("sigact: query ret=%ld new-null=%d old-null=%d handler=%d mask=%d flags=%d "
               "flags-value=0x%x new-before=%s new-after=%s old-before=%s old-after=%s\n",
               result, new_guard.act.handler == 0, old_guard.act.handler == 0,
               handler_ok, mask_ok, flags_ok, old_guard.act.flags,
               verdict(new_bad == 0), verdict(new_bad == 0),
               verdict(old_bad == 0), verdict(old_bad == 0));
        if (result != 0 || new_bad != 0 || old_bad != 0 || !handler_ok || !mask_ok ||
            !flags_ok) {
            failures++;
        }
    }

    // Row 3: restore SIG_DFL from a guarded request, with non-null old capturing the caught action.
    guard_init(&new_guard);
    guard_init(&old_guard);
    result = raw_sigaction(SIGUSR1, &new_guard.act, &old_guard.act);
    {
        int new_bad = guard_check(&new_guard, "restore-new");
        int old_bad = guard_check(&old_guard, "restore-old");
        int old_handler_ok = old_guard.act.handler == handler_value;
        printf("sigact: restore ret=%ld old-handler=%d new-before=%s new-after=%s "
               "old-before=%s old-after=%s\n",
               result, old_handler_ok, verdict(new_bad == 0), verdict(new_bad == 0),
               verdict(old_bad == 0), verdict(old_bad == 0));
        if (result != 0 || new_bad != 0 || old_bad != 0 || !old_handler_ok) {
            failures++;
        }
    }

    // Row 4: the disposition is really SIG_DFL again; old is guarded and non-null.
    guard_init(&old_guard);
    result = raw_sigaction(SIGUSR1, NULL, &old_guard.act);
    {
        int old_bad = guard_check(&old_guard, "default");
        int handler_ok = old_guard.act.handler == SIG_DFL;
        printf("sigact: default ret=%ld handler=%d old-before=%s old-after=%s\n", result,
               handler_ok, verdict(old_bad == 0), verdict(old_bad == 0));
        if (result != 0 || old_bad != 0 || !handler_ok) {
            failures++;
        }
    }

    printf("sigact: done failures=%d\n", failures);
    if (failures != 0) {
        printf("sigact: FAIL failures=%d\n", failures);
        return 1;
    }
    printf("sigact: ok\n");
    return 0;
}
