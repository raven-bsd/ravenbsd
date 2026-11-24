# S199 SIGACTION guarded-buffer conformance (x86_64)

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

This is the guarded-buffer regression for the kernel `SIGACTION` (raw `0x87`) seam. The kernel
copied a 152-byte C `struct sigaction` from a 40-byte libc object before `aef6f7f58`; this suite
proves the seam now copies exactly the 40-byte S160 request and touches no adjacent user byte.

The C 152-byte `struct sigaction` stays in libc (`raven-libc-signal/src/sigaction_abi.rs`,
`from_c`/`to_c`). The kernel form is `kernel/src/syscall/thread.rs` `UserSigAction`: handler at 0,
`mask[2]` at 8, flags at 24, `_pad` at 28, restorer at 32 (40 bytes). libc's `sigaction(3)` builds
that request on its own stack, where this test cannot guard it, so the probe issues the raw
`syscall` instruction itself.

## Rows

The probe prints `sigact:` rows and exits 0 only when every row checks.

- `sigact: layout size=40 handler=0 mask=8 flags=24 restorer=32` - the probe's request matches the
  kernel ABI.
- `sigact: install ...` - install a caught `SIGUSR1` action from a 40-byte request with canary
  words before and after it; the canaries are unchanged.
- `sigact: query ...` - pass a non-null `act` and a non-null `old`; the previous handler, mask and
  `SA_RESTART` flag round-trip, and every canary around both 40-byte objects is unchanged.
- `sigact: restore ...` - install `SIG_DFL` with a non-null `old`; the caught action is captured and
  every canary is unchanged.
- `sigact: default ...` - a following non-null `old` reads `SIG_DFL` back with intact canaries.
- `sigact: done failures=0`, `sigact: ok`.

## Run

```sh
timeout 420 /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/scripts/build-slot.sh s199-image -- \
    sh tests/conformance/signals/build.sh
timeout 60 python3 tests/conformance/signals/run.py --selftest
timeout 60 python3 tests/conformance/signals/run.py --image build/conformance/signals/signals.img
```

The build produces `build/conformance/signals/signals.img`; the run boots it under OVMF/QEMU and
reads the serial rows. `--accel kvm` selects KVM.

## Mutation (negative check)

The test-first defect is the pre-`aef6f7f58` kernel mirror. Revert only the local working-tree
kernel mirror to the 152-byte layout:

```c
struct UserSigAction {
    handler: u64,
    mask: [u64; 16],
    flags: i32,
    _pad: i32,
    restorer: u64,
};
```

Rebuild the image and run the same suite with `--mutation`. The 152-byte `copy_to_user` overwrites
the canary words after the 40-byte `old`, and the flags round-trip reads offset 136 instead of 24;
the `query` and `restore` rows turn red and the harness prints `MUTATION RED`. Restore the kernel
tree afterwards and commit no mutation.

## Donor map

- FreeBSD `sys/kern/kern_sig.c` `sys_sigaction()` (`copyin`/`copyout` `sizeof(act)`) and
  `kern_sigaction()`; BSD-2-Clause. `sys/sys/signal.h` `struct sigaction`.
- NetBSD `sys/kern/sys_sig.c` `sys___sigaction_sigtramp()` (`copyin`/`copyout` `sizeof(nsa)`) and
  `sigaction1()`; BSD-2/3-Clause.
- Illumos `usr/src/uts/common/syscall/sigaction.c` `sigaction()` and
  `usr/src/uts/common/os/sig.c` `setsigact()`; CDDL, behaviour only (clean-room).
- Linux `kernel/signal.c` `do_sigaction()` / `SYSCALL_DEFINE4(rt_sigaction)`; GPL, read-only. The
  local sparse donor checkout has no `kernel/` tree; the outcome used is the fixed-size copy already
  established by the BSD donors.
