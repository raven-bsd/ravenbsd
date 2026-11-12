# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S20 fuzz fixtures.
#
# This directory is reserved for crash reproducers found by the `raven-fuzz` soak: when a target
# violates an invariant, the failing input is committed here as `<target>-<n>.hex` (the same hex
# format as `tests/fuzz/corpus`), a `tests/fuzz/replay/tests/corpus.rs` entry replays it, and the
# report names the seed and iteration that produced it. There are no reproducers yet: the smoke
# and the deterministic replay found no invariant violation.
#
# The regular corpus lives in `tests/fuzz/corpus/<target>/`; see `docs/design/fuzzing.md`.
