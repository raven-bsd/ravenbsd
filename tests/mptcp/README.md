<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# MPTCP response/vector tests

`raven-mptcp-vectors` is a workspace member containing integration tests and the deterministic
replay of the cargo-fuzz target. It depends only on `crates/raven-mptcp`.

## Byte-exact vectors

| Name | Hex |
|---|---|
| MP_CAPABLE SYN | `1e 04 01 01` |
| MP_CAPABLE SYN/ACK | `1e 0c 01 01 01 02 03 04 05 06 07 08` |
| MP_CAPABLE ACK | `1e 14 01 01 11 12 13 14 15 16 17 18 01 02 03 04 05 06 07 08` |
| MP_CAPABLE ACK+data | `1e 16 ... 00 2c` (22 bytes) |
| MP_JOIN SYN | `1e 0c 10 03 11 22 33 44 55 66 77 88` |
| MP_JOIN SYN/ACK | `1e 10 10 02 00 01 02 03 04 05 06 07 99 aa bb cc` |
| MP_JOIN ACK | `1e 18 10 00` + 20 HMAC bytes |
| DSS ACK | `1e 08 20 01 01 02 03 04` |
| DSS ACK+mapping+FIN+checksum | `1e 1c 20 1f ... 00 2c 33 44` (28 bytes) |
| ADD_ADDR IPv4 | `1e 10 30 05 c0 00 02 01` + 8 HMAC |
| ADD_ADDR IPv4 echo | `1e 08 31 05 c0 00 02 01` |
| ADD_ADDR IPv6 | `1e 1c 30 06` + 16 address + 8 HMAC |
| REMOVE_ADDR | `1e 06 40 03 07 09` |
| MP_PRIO | `1e 03 50` / `1e 03 51` |
| MP_FAIL | `1e 0c 60 00` + 8 DSN |
| MP_FASTCLOSE | `1e 0c 70 00` + 8 key |
| MP_TCPRST | `1e 04 80 01` (permanent) / `1e 04 81 02` (transient) |

Every vector is round-tripped through `MptcpOption::parse`/`to_vec`, every strict prefix must be
rejected, and the option list test covers NOP/EOL and foreign TCP options.

## Fuzz target

```sh
cd tests/mptcp/fuzz
RUSTUP_TOOLCHAIN=nightly cargo fuzz run mptcp_options -- -runs=10000
```

The workspace pins stable Rust, so the nightly toolchain is selected explicitly for cargo-fuzz
only. The target calls `option::parse_options` and asserts that any accepted option re-encodes to
the consumed bytes. A three-byte DSS header (`01 1e 03 20 ff ff 59`) crashed the first run; the fix
is the `length < 4` guard in `parse_dss` and the regression test in `option_ut.rs`.

`tests/fuzz_smoke.rs` runs the same invariant on 10,000 deterministic pseudo-random blocks under
stable nextest, so CI does not need nightly.
