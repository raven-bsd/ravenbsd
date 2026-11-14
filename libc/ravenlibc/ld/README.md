<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-rtld

`/libexec/ld-elf.so.1`, the RavenBSD dynamic loader.

The crate is `no_std` on the target, allocation-free in its own objects (the C52 views use the
loader's pool), and host-testable behind the `mapper::Platform` seam. The mapping, loading,
relocation, TLS and `dl*` logic lives in safe Rust; `src/raw/` is the only `unsafe` module
(DR-39 class M) and contains the syscalls, the entry assembly and the C exports. The
`selfreloc` stage is C61's `crt/src/selfreloc.rs`, included by `#[path]`, never copied.

Exports (17): `dlopen`, `dlclose`, `dlsym`, `dlvsym`, `dlerror`, `dladdr`, `dlinfo`,
`dl_iterate_phdr`, `__tls_get_addr`, `_rtld_error_str`, `_rtld_allocate_tls`, `_rtld_free_tls`,
`_rtld_get_stack_prot`, `_rtld_set_dlerror_slot`, `r_debug_state`, `_r_debug_postinit`,
`_rtld_version`. The `RtldTable` in `src/table.rs` is the C64 contract; the loader installs it
through `__rl_dl_install` when the shell provides it.

Build the shared objects:

```sh
cargo xtask libc-pack --arch x86_64 --check
cargo xtask libc-link --arch x86_64 --check
```

Host tests load real x86_64 shared objects through `HostMapper`; the real `libc.so.1` test is
`cargo test -p raven-libc-rtld --test host_libc` and is skipped when the file is absent. Set
`RL_RTLD_REQUIRED=1` to turn a missing tool or file into a failure.

On-target work waits for PG-1 (thread pointer), PG-13 (`MAP`/`PROTECT`), PG-36 (file memory
objects), PG-49 (spawn with `PT_INTERP`), PG-50 (auxv/`rsp`), PG-51 (`AT_SECURE`/`AT_HWCAP`) and
PG-53 (4 KiB ELF shape gate); see `architecture/raven-libc/donor-map/13-dynamic-loader.md`.
