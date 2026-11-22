<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP boot soak (fixture, ambiguous)

The campaign stopped before the TCG column was recorded.

| Mode | Requested | Passed | Failure | p50 | p90 | worst |
|---|---:|---:|---|---:|---:|---:|
| KVM `-smp 1` | 20 | 20 | none | 3.402 s | 3.753 s | 3.935 s |
| KVM `-smp 2` | 20 | 20 | none | 3.429 s | 3.906 s | 3.950 s |
