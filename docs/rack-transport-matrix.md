# RACK transport/HPTS matrix

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only evidence matrix over the merged K335 `rack_transport`/K313 HPTS paths. The package is
`tests/rack-transport-matrix` (`cargo nextest run -p rack-transport-matrix`); it never edits product
source.

## Matrix

| Path | Called with | Compared with |
|---|---|---|
| `rack_start_hpts_timer` | TLP timer 142 us | K320 `RackModel::pto` and K313 slot rounding (150 us) |
| `rack_process_timers` | TLP fired at 200 us | `rack_timeout_action(PACE_TMR_TLP)` -> `RackTimeout::Tlp` |
| `rack_stop_all_timers` | persist timer armed | `TimerStopOutcome.persist_pending`, `in_persist` |
| `tcp_addrack` | load/quiesce/unload | K327 `VtableModel::init`/`fini`, K314 UMA/sysctl state |
| `rack_ctloutput` | set/get `TCP_RACK_PACE_MIN_SEG` 800 | K327 `VtableModel::ctloutput` |
| `rack_hw_tls_change` | enable true | K327 `VtableModel::hwtls_change` |
| `rack_pru_options` | 0 and `PRUS_OOB` | K327 `VtableModel::pru_options` (`EOPNOTSUPP`) |
| `rack_wake_check` | pacing due/not due | K327 `VtableModel::early_wake_check` |
| `rack_switch_failed` | MBUFQ/ACKCMP caps | K327 `VtableModel::switch_failed` flags |

The K320 TLP model is used for the TLP PTO; K327 is used for the vtable/transport glue. K319/K324
loss/SACK and K326 BBR are not applicable to this glue path and are not duplicated here.

## K315 union row

The package embeds the nine-row matrix and validates it with the K315 checker
(`rack_coverage::check`). A complete row passes; deleting a row reports it missing, duplicating a
row reports the duplicate, a blank test reports the empty field, and a non-PASS status is rejected.
The same matrix is in the K344 report for the real-donor CLI run.

## Running

```text
cargo nextest run -p rack-transport-matrix
```
