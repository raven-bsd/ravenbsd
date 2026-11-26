<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP boot soak (fixture, TCG4 failure)

## Round 5 matrix (fixture)

| Mode | Requested | Passed | Failure | p50 | p90 | worst |
|---|---:|---:|---|---:|---:|---:|
| KVM `-smp 1` | 20 | 20 | none | 3.402 s | 3.753 s | 3.935 s |
| KVM `-smp 2` | 20 | 20 | none | 3.429 s | 3.906 s | 3.950 s |
| KVM `-smp 4` | 50 | 50 | none | 3.512 s | 3.817 s | 3.977 s |
| TCG `-smp 4` | 10 | 4 | **run 5/10 login timeout** | 8.530 s* | 10.262 s* | 10.262 s* |

TCG4 run 5 reached `svcd: all services up` but printed no login banner before the 30 s timeout.
