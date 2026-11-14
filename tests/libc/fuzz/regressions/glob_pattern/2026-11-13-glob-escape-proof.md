<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# 2026-11-13 `glob_pattern`: escape-aware match proof

Target: `glob_pattern`
Owner: C46 (`raven-libc-glob`)
Input: `2026-11-13-glob-escape-proof.bin` (23 bytes)

`glob` interprets backslashes as escapes unless `GLOB_NOESCAPE` is set. The target's match
proof used the input's `fnmatch` flags directly, so a pattern with escapes produced a path
that `fnmatch` with `FNM_NOESCAPE` rejected. This is a property bug in C70, not a glob bug:
the proof now uses the same escape mode as `glob`.
