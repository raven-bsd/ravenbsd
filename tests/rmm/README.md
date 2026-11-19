<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RM1 host model harness

This directory is the fixed-corpus driver for the `raven-rmm` adversarial model in
`crates/raven-rmm/tests/rmm_model.rs`. It is not a boot-time benchmark.

The model uses three fixed seeds (`0x123456789abcdef0`, `0x0badf00ddeadbeef`,
`0xa5a55a5a12344321`) over a low and a high virtual half, for both `X86_64Encoding` and
`Aarch64Encoding`. It mixes 4 KiB and 2 MiB map/unmap/protect/translate operations, double-map
and huge/4K conflicts, the deliberate 1 GiB rejection, table recycle, OOM rollback, direct-map
failure and an unreadable-table case. Every run unmaps all live mappings and asserts that only
the root frame remains allocated.

Run the bounded long corpus in the foreground:

```sh
timeout 40 sh tests/rmm/run-model.sh
```

`RMM_MODEL_OPS` sets the iterations per seed/base/encoding (default 10,000; the harness default
for the long run is 50,000). The test prints per-corpus `ops`, `p50_ns` and `p90_ns`; copy the
recorded output into the RM1 report. A cap expiry is a timeout and is reported as `BLOCKED`; do
not raise the cap without a measured runtime.
