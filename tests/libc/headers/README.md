# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# raven-libc-hdrcheck

Host-only test matrix for the ravenlibc C headers. It compiles every owned
header standalone for the three architectures in C11/C17/C23/GNU17 and
C++17/C++20, in the five visibility modes, checks struct layouts and constants
against doc 14, and exercises the feature-test engine.

No dependency: the TOML subset is parsed by `src/toml_lite.rs`; `clang` is
invoked through `std::process::Command` (no `cc`, `bindgen` or `build.rs`).

```sh
cargo test -p raven-libc-hdrcheck --lib
HDRCHECK_REQUIRE_OWNERS=C37 cargo test -p raven-libc-hdrcheck --release
```

Environment: `RAVEN_CLANG` (default `clang` from PATH), `HDRCHECK_JOBS`,
`HDRCHECK_QUICK=1`, `HDRCHECK_KEEP=<dir>`, `HDRCHECK_REQUIRE_OWNERS=C37,C38`,
`HDRCHECK_REQUIRE_COMPLETE=1`.
