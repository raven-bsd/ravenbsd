<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# AT_PHDR and the uutils link script

Status: **OBSERVED.** The uutils `du` binary now carries `PT_PHDR` and a first `PT_LOAD` at file
offset 0, so the kernel's `AT_PHDR` names a mapped program-header table and libc can find
`PT_TLS`. Ticket K136; the remaining `du` traversal/join hang is K134.

## Before

`userland/uutils/x86_64.ld` declared the load segments without `FILEHDR PHDRS`:

```ld
PHDRS {
    text PT_LOAD FLAGS(5);
    ...
}
SECTIONS {
    . = 0x00400000;
```

`readelf -lW build/x86_64/uutils/du` on the pre-fix build:

```text
Entry point 0x5282e8
  LOAD           0x001000 0x0000000000400000 0x0000000000400000 0x13c000 0x13c000 R E 0x1000
  LOAD           0x13d000 0x000000000053c000 0x000000000053c000 0x074000 0x074000 R   0x1000
  LOAD           0x1b1000 0x00000000005b0000 0x00000000005b0000 0x005000 0x005000 RW  0x1000
  LOAD           0x1b6000 0x00000000005b5000 0x00000000005b5000 0x000000 0x003b10 RW  0x1000
  TLS            0x1b1000 0x00000000005b0000 0x00000000005b0000 0x000280 0x0002d8 RW  0x8
```

There is no `PT_PHDR`, and the first `LOAD` starts at file offset `0x1000`; the ELF header at
offset 0 and the program headers at `0x40` are unmapped. `raven-elf::phdr_vaddr()` returns `None`,
the kernel sets `AT_PHDR=0`, and `__raven_init_main_thread` cannot find the `PT_TLS` module. The
first new thread then reads an uninitialised TLS handle and std aborts:

```text
raven# du /tmp/du
process: child exited (134)
fatal runtime error: current thread handle already set during thread spawn, aborting
```

## Fix

`userland/uutils/x86_64.ld` now declares the program header and maps the ELF/program headers in
the text segment:

```ld
PHDRS {
    phdr PT_PHDR PHDRS;
    text PT_LOAD FILEHDR PHDRS FLAGS(5);
    ...
}
SECTIONS {
    . = 0x00400000 + SIZEOF_HEADERS;
```

The image base stays `0x00400000`; `.text` remains page-aligned and starts on the next page after
the headers. The first `LOAD` file part is `0x13d000` bytes, still a page multiple, and `.tdata`
stays on its own page (`0x5b1000`).

`readelf -lW build/x86_64/uutils/du` after the fix:

```text
Entry point 0x5292e8
  PHDR           0x000040 0x0000000000400040 0x0000000000400040 0x000150 0x000150 R   0x8
  LOAD           0x000000 0x0000000000400000 0x0000000000400000 0x13d000 0x13d000 R E 0x1000
  LOAD           0x13d000 0x000000000053d000 0x000000000053d000 0x074000 0x074000 R   0x1000
  LOAD           0x1b1000 0x00000000005b1000 0x00000000005b1000 0x005000 0x005000 RW  0x1000
  LOAD           0x1b6000 0x00000000005b6000 0x00000000005b6000 0x000000 0x003b10 RW  0x1000
  TLS            0x1b1000 0x00000000005b1000 0x00000000005b1000 0x000280 0x0002d8 RW  0x8
```

The thread-spawn abort is gone. `base-tools-du` no longer prints the fatal runtime error; it
reaches `du /tmp/du` and then produces no size line, so the case fails at its 30 s step timeout.
That is the K134 traversal/join hang, not the K136 header mapping; `du` output and completion stay
with K134.

## Other architectures

The uutils builder uses `userland/uutils/<arch>.ld` when it exists, otherwise
`userland/linker/<arch>.ld`. Only `userland/uutils/x86_64.ld` exists, so aarch64 uses
`userland/linker/aarch64.ld`, which already has:

```ld
PHDRS {
    phdr PT_PHDR PHDRS;
    text PT_LOAD FILEHDR PHDRS FLAGS(5);
    ...
}
SECTIONS {
    . = 0x00400000 + SIZEOF_HEADERS;
```

No aarch64 uutils script change is needed. The generic `userland/linker/x86_64.ld` has the same
omission, but it is outside K136's Owns and is not used by uutils; it is the remaining handoff for
any TLS-bearing generic program that uses it.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | RavenBSD decision |
|---|---|---|---|---|
| Program headers in the first load | Uses the runtime linker's mapped headers; behaviour only, no code copied. | NetBSD/FreeBSD linkers map the ELF/program headers in the first load segment; behaviour only, no code copied. | Linux maps the headers and sets `AT_PHDR` to the mapped table; idea only. | First-party ld script change: `FILEHDR PHDRS` + explicit `PT_PHDR`; no donor code copied or translated. |
