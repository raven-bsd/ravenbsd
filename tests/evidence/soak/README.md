# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# Soak evidence schema

Status: **no 24 h run exists.** The 24 h soak gate is BLOCKED on a quiet host
(the runbook: [`../../docs/releases/0.1.0-soak-runbook.md`](../../docs/releases/0.1.0-soak-runbook.md)).
This directory currently contains only the schema and the syntax-checked command
file; the first completed run creates its `<image-sha256>/` directory.

## Layout

```
tests/evidence/soak/
├── README.md                 # this schema
├── runbook-commands.sh       # extracted, `sh -n`-verified commands (K388)
└── <image-sha256>/           # one directory per release soak image
    ├── hour-00.log           # checkpoint before/at start
    ├── hour-01.log
    ├── ...
    ├── hour-24.log           # final checkpoint after the workload
    ├── summary.tsv           # one row per hour + the closing row
    └── SHA256SUMS            # every file in the directory, at publish time
```

`<image-sha256>` is the SHA-256 of `build/soak/image.img` recorded before the
run. Two different images never share a directory.

## `hour-NN.log` fields

One UTF-8 log per checkpoint (`NN` = `00`..`24`), with these lines:

```
timestamp_utc=<YYYY-MM-DDTHH:MM:SSZ>
commit=<git commit>
image_sha256=<sha256>
load1=<float> load5=<float> load15=<float> nproc=<int>
qemu_pid=<pid|none> qemu_elapsed=<hh:mm:ss> qemu_cpu_pct=<float> qemu_rss_mib=<int>
run_dir=<build/soak/run-... directory or none>
soak_jsonl_bytes=<int> serial_bytes=<int> serial_stamped_bytes=<int>
summary_present=<yes|no>
serial_scan_hits=<int or the matched keys>
host_scan_hits=<int or the matched keys>
checkpoint_result=<ok|suspect|abort>
note=<free text>
```

A checkpoint is `suspect` when every sampled value is unchanged (no artifact
growth) or a scan key matched; `abort` when an abort criterion fired.

## `summary.tsv` columns

Header (tab-separated, exactly this order):

```
hour	start_utc	end_utc	load1	load5	load15	qemu_alive	run_dir	soak_jsonl_bytes	serial_bytes	cycles_done	result	note
```

* `hour` = `00`..`24`;
* `result` = `running` during the run, `ok` for a clean checkpoint, `abort` for
  an aborted checkpoint whose criteria fired, `partial` for the closing row of
  an aborted run, `pass` **only** for the closing row of a completed 24 h run
  with every checkpoint `ok` and a zero exit from the harness.
* The closing row (`hour` = `24` or the abort hour) is the only row allowed to
  carry `pass`/`partial`.

## Partial runs

A run that stops early is published as `result=partial` with the abort reason and
the last checkpoint. It is never written as `pass`, never retried to hide a
sample, and the gate remains BLOCKED. A `partial` row requires the preserved
`build/soak/run-<UTC>/` directory and its serial log.

## Integrity

`SHA256SUMS` lists every file in the directory (`sha256sum` lines, relative
paths) and is regenerated last. Any edit after publishing invalidates the sums;
re-run `sha256sum` and record the reason in the closing note.

## Related

* Harness: `scripts/soak.sh`, `tests/fixtures/soak/soak_driver.py` (K228).
* Runbook: `docs/releases/0.1.0-soak-runbook.md`.
* The TCP soak evidence runner (K300) is a separate gate with its own schema.
