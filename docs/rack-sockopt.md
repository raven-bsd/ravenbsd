<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK sockopt/query conformance matrix (K340)

Status: host-only matrix, 2026-11-21. **OBSERVED** against the merged K334
`raven_tcpip::RackMisc` surface; no product source is edited here.

## Purpose

K334 ported the RACK setsockopt/getsockopt/query functions. `tests/rack-sockopt/` exercises every
K334 option for defaults, valid set/get, invalid values, read-only behavior, numeric clamps,
deferred options, inheritance and the three query requests. Mismatches are recorded as findings
for K334.

## Option matrix

| Option | Default | Valid set -> get | Invalid / notes |
|---|---:|---|---|
| `TCP_DEFER_OPTIONS` | 0 | 1 -> 1 | boolean |
| `TCP_RACK_DSACK_OPT` | 0 | 3 -> 3 | bits 1 (std) and 2 (use) |
| `TCP_RACK_HI_BETA` | 0 | 1 -> 1 | boolean |
| `TCP_RACK_TIMER_SLOP` | 0 | 123 -> 123 | u32 |
| `TCP_RACK_MEASURE_CNT` | 1 | 5 -> 5; 0 -> 1 | clamps to at least 1 |
| `TCP_NO_PRR` | 0 | 1 -> 1 | boolean |
| `TCP_USE_CMP_ACKS` | 0 | 1 -> 1 | boolean |
| `TCP_RACK_MBUF_QUEUE` | 0 | 1 -> 1 | boolean |
| `TCP_RACK_PACE_TO_FILL` | 0 | 1 -> 1 | boolean |
| `TCP_RACK_FORCE_MSEG` | 0 | 1 -> 1 | boolean |
| `TCP_RACK_ABC_VAL` | 0 | 7 -> 7; 300 -> 255 | u8 clamp |
| `TCP_RACK_PACING_DIVISOR` | 250 | 300 -> 300; 0 -> 250; 10 -> 50 | default/min clamp |
| `TCP_RACK_PACE_MIN_SEG` | 0 | 1234 -> 1234; 70000 -> 65535 | u16 clamp |
| `TCP_RACK_PACE_MAX_SEG` | 0 | 7 -> 7; 100 -> 63 | capped at `MAX_USER_SET_SEG` |
| `TCP_PACING_DND` | 0 | 1 -> 1 | boolean |
| `TCP_RACK_PROFILE` | write-only | 0/1/6 select modes; 9 -> `EINVAL` | get -> `EINVAL` |
| `TCP_RACK_SET_RXT_OPTIONS` | write-only | 0/1/2 select modes; 3 -> `EINVAL` | get -> `EOPNOTSUPP` |
| `TCP_RACK_PACING_BETA` | 0 (50 after profile 1) | set -> `EOPNOTSUPP` | **K334 finding** |
| `TCP_INFO` | unsupported | get -> `EOPNOTSUPP` | **K334 finding** |

Unknown options return `EOPNOTSUPP`; a wrong socket level or direction also returns
`EOPNOTSUPP`.

## Query matrix

| Request | Expected behavior |
|---|---|
| `TCP_QUERY_SENDMAP` | returns the matching sendmap range/flags/timestamps; 0 for unknown or empty flight |
| `TCP_QUERY_TIMERS_UP` | 0 without a pacing method; 1 with the HPTS flags and times when armed |
| `TCP_QUERY_RACK_TIMES` | fills the RTT/reorder state; 1 |
| any other request | `-22` (`-EINVAL`) |

## Findings for K334

1. `TCP_RACK_PACING_BETA` is readable but not writable: `rack_set_sockopt` returns
   `EOPNOTSUPP`, while the donor routes it through the NewReno beta and `rack_get_sockopt` reports
   0/50 from the local state. The setter is not wired to congestion-control state.
2. `TCP_INFO` returns `EOPNOTSUPP`; the donor calls `rack_fill_info`/`sooptcopyout`. K334 has
   `rack_fill_info` but `rack_get_sockopt` does not use it.
3. `TCP_RACK_ABC_VAL` values above 255 are clamped to 255; the donor consumes the option as a
   byte, so this is recorded as a port decision rather than a wire difference.
4. `TCP_RACK_PACE_MIN_SEG` values above 65535 are clamped; the donor stores a `u16`-sized option.

No product source was edited; the matrix is evidence for K334/K269.

## Donor and references

* FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`,
  `sys/netinet/tcp_stacks/rack.c`: `rack_set_sockopt`, `rack_get_sockopt`,
  `rack_process_option`, `rack_chg_query`, `rack_fill_info`, `rack_add_deferred_option`,
  `rack_apply_deferred_options`, `rack_inherit`, `rack_set_profile`.
  BSD-2-Clause; behaviour only, no code copied.
* Illumos `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` and Linux
  `adc218676eef25575469234709c2d87185ca223a` for outcome/behaviour comparison only.

## Run

```sh
timeout 180 cargo nextest run -p rack-sockopt
timeout 420 make check-fast
```
