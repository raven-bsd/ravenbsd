<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP boot soak (fixture, clean)

Campaign at `1111111111111111111111111111111111111111`, run `20260101T000000Z`:

| Mode | Requested | Passed | Failure | p50 | p90 | worst |
|---|---:|---:|---|---:|---:|---:|
| KVM `-smp 1` | 20 | 20 | none | 3.402 s | 3.753 s | 3.935 s |
| KVM `-smp 2` | 20 | 20 | none | 3.429 s | 3.906 s | 3.950 s |
| KVM `-smp 4` | 50 | 50 | none | 3.512 s | 3.817 s | 3.977 s |
| TCG `-smp 4` | 10 | 10 | none | 9.617 s | 11.466 s | 12.387 s |
