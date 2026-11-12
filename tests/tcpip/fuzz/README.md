<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Fuzz targets

Nightly-only `cargo-fuzz` targets for `raven-tcpip`. This directory is its own
Cargo workspace, so a normal stable `cargo build` and `make check` never need
nightly or `cargo-fuzz`.

The target bodies are in `raven-tcpip-fuzz-replay` (stable, workspace member)
so every corpus seed is also an ordinary `cargo test` case:

```sh
cargo test -p raven-tcpip-fuzz-replay
```

## Tooling

`cargo-fuzz` and libFuzzer are developer tools, not dependencies of a shipped
crate. Install once:

```sh
cargo install cargo-fuzz --locked
rustup toolchain install nightly
```

## Smoke mode (about a minute per target)

```sh
cd tests/tcpip/fuzz
rustup run nightly cargo fuzz build
for target in wire_parse tcp_options segment_sequence handshake_state reassembly; do
  rustup run nightly cargo fuzz run "$target" -- -max_total_time=60
done
```

## Long soak

```sh
cd tests/tcpip/fuzz
for target in wire_parse tcp_options segment_sequence handshake_state reassembly; do
  rustup run nightly cargo fuzz run "$target" -- -max_total_time=3600
done
```

## Corpus

`corpus/<target>/` holds small deterministic seeds, including C01-style header
vectors and C04-style TCP payload sequences. Keep files small and readable.
Crashing inputs found by a run belong in the same corpus directory and must
also become a stable regression test in `raven-tcpip-fuzz-replay` (or the
owning crate's `_ut.rs`).

## Replay

```sh
cd tests/tcpip/fuzz
rustup run nightly cargo fuzz run wire_parse corpus/wire_parse/<file>
```

The stable replay harness exposes the same functions:

```rust
raven_tcpip_fuzz_replay::replay("wire_parse", &bytes)?;
```
