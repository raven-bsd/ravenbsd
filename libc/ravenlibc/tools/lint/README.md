<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc lint scopes

`rules.toml` is read by `cargo xtask libc-lint`; change a scope there, not in
the Rust code. S-libc covers shipped libc sources, S-host covers the host and
oracle test trees, S-crate is the manifest-driven crate set.

`no-alloc` rejects `alloc::` paths and `extern crate alloc` in a crate whose
`std = "core"` row has no `alloc_feature`; `[no-alloc] allow-files` is the file
exemption list. The test files and `examples/` are skipped, as for `no-std`.

`-C relocation-model=pic` is a rustc flag passed by `libc-link` through
RUSTFLAGS; it is not a Cargo profile key.
