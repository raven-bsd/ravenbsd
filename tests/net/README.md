# packetdrill-compatible harness for `raven-netcore`

This directory contains a host-side harness that runs a small, hand-written subset of
[packetdrill](https://github.com/google/packetdrill) scripts directly against `raven-netcore`
over its in-memory `PairDevice`. packetdrill itself is GPL-2.0 (tier C in
`architecture/12-licensing-and-provenance.md`): it is **not** linked, vendored or copied here. It
may only be run as a separate external tool for syntax validation.

## Why a self-contained runner

The ticket allows either driving the real packetdrill tool over pcap/TUN or implementing the
minimal parsing here. The host CI has no guaranteed `libpcap-dev`, root, or TUN device, so the
harness parses the required subset itself and keeps packetdrill as an optional oracle:

```sh
PACKETDRILL_BIN=/path/to/packetdrill sh tests/net/check-syntax.sh
```

`check-syntax.sh` skips cleanly when no packetdrill binary is available; it never imports its
code or scripts.

## Usage

```sh
sh tests/net/run.sh                 # all scripts, 10 s timeout each
sh tests/net/run.sh --timeout-ms 5000 --scripts tests/net/scripts --results tests/net/results
```

The runner executes every `tests/net/scripts/*.pkt` in a child process. The child drives the
stack in-process; the parent enforces a hard kill after the timeout plus one second. Exit status
is 0 only when every script passes or is listed in `known-failures.txt`.

## Topology

| Side | IPv4 | MAC | TCP port |
|---|---|---|---|
| SUT (`raven-netcore`) | `10.0.0.1/24` | `02:00:00:00:00:01` | `8080` (listening) |
| Peer (script) | `10.0.0.2` | `02:00:00:00:00:02` | `12345`, ISN 1000 |

The peer endpoint answers the SUT's ARP requests. Script sequence numbers are relative to the
peer ISN; the SUT's ISN is learned from its SYN-ACK and used to translate ACK numbers. Link
behaviour is deterministic (`PairConfig::reliable()`).

## Supported script subset

* timestamps: absolute since script start (`0.000`, `0.300`) and relative (`+0.100`), converted
  to virtual milliseconds; a backwards absolute time is rejected;
* inbound packets `<` and expected outbound packets `>`:
  `S`, `.`, `P`, `F`, `R` flags, `seq:end(len)`, optional `ack`, `win`, and ignored TCP options;
* syscalls: `socket`, `bind`, `listen`, `accept`, `connect`, `read`, `write`, `close`,
  `setsockopt`; `...` arguments are accepted; `= -1 ERRNO (description)` is supported.
  `socket()` and `bind()` only assert their scripted results; `listen()` creates the real
  `raven-netcore` listener on the fixed port and `accept()`/`connect()` create the socket.

Unsupported packetdrill features are rejected rather than silently ignored. All six scripts in
`scripts/` pass packetdrill's own `--dry_run` parser.

## Results

One JSON file per script, named after the script, with exactly:

```json
{ "script": "syn-handshake.pkt", "result": "pass", "duration_ms": 0, "detail": "ok" }
```

`result` is `pass`, `fail` or `known-fail`. `results/conform.json` is the same data in
`raven-conform`'s `Results` schema, and `results/summary.md` is the human-readable table.

## Fuzz-lite

`tests/fuzz_lite.rs` feeds 2,000 deterministic random Ethernet frames, every truncation of a
valid TCP frame, and malformed IPv4 headers through the same harness link. It asserts only that
ingestion never panics or errors; it is not a substitute for `raven-netcore`'s own fuzz tests.

## Known failures

`known-failures.txt` has one `<script> <reason>` entry per line. A listed script that starts
passing fails the run, so the list can only shrink.
