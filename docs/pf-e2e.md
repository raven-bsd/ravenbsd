<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# pf and tooling end-to-end conformance

Status: K297, 2026-11-21. Test-only: this tree drives the three existing product
libraries (K273 `raven-pf`, K274 `raven-pfctl`, K281 `raven-tcpdump`) through
one path; it changes no product code. The K277 fixture matrix is the oracle.

```sh
timeout 300 sh tests/pf-conformance/e2e/run.sh
```

## 1. The path

```text
tests/pf-conformance/rules/*.conf
        |  raven-pfctl::parse_config / print_config   (K274 pfctl front-end)
        |  raven-pf::parse -> Engine::evaluate        (K273 filter/state/NAT)
        |  packet rows -> Ethernet/IP/TCP/UDP/ICMP frames -> pcap
        v  raven-tcpdump::execute                     (K281 offline decoder)
   verdict + state + translation, tcpdump decode and filter counts
```

For every K277 scenario the harness (`e2e/harness`, a test-only crate outside the
product workspace) does all of the following and fails if any step disagrees:

1. **pfctl load** — parse the rules with `raven-pfctl`, print them, re-parse the
   print and require a byte-identical round trip.
2. **filter** — parse the same rules with `raven-pf`, drive every packet row
   through `Engine::evaluate`, and assert the verdict, state disposition and
   NAT/RDR translation.
3. **capture** — build the Ethernet/IPv4/IPv6/TCP/UDP/ICMP(6) frames for the same
   rows, write a classic pcap, decode it with `raven-tcpdump` and assert the
   packet count, the first decode line and a protocol-filter count.

`tests/pf-conformance/e2e/run.sh` also exercises the **binary** pfctl path
(`raven-pfctl -nf tests/fixtures/pfctl/pf.conf` exits 0; the invalid
`bad-address.conf` exits non-zero) and keeps every log/pcap/hash under
`build/pf-e2e/`.

## 2. Coverage

| Case | Source | Checks |
|---|---|---|
| stateful TCP/UDP/ICMP allow, deny, reverse, expiry | K277 `packets/stateful-*.tsv` | verdict, state, counters via K277; decode+filter here |
| default deny, quick/last-match, CIDR, ports | K277 `default-deny`, `quick-last-match` | verdict and decode |
| NAT source translation + reverse | K277 `nat` | `Decision::translation` both directions, decode of the wire tuple |
| redirect + reverse | K277 `rdr` | translation both directions, decode |
| IPv6 TCP/UDP state, allow/deny | K277 `ipv6` | verdict/state, IPv6 decode |
| table membership (`table <web> { ... }`) | K297-local (the K277 matrix has no table row) | pfctl table parse, engine membership, decode |
| malformed rules fail closed | K277 `rules/malformed-*.conf` | both parsers reject; the raven-pf line/substring is pinned |
| fragmentation with reassembly | K277 `fragment` | `SKIP`: `pf_reassemble` is outside 0.1.0 (`D-pf-frag-state`) |
| fragmentation without reassembly | K277 `fragment-no-scrub` | `KNOWN`: donor expects block, `raven-pf` 0.1.0 admits it |

The first run: **15 passed, 1 skipped, 1 known, 0 failed** (see
`build/pf-e2e/report.json`).

## 3. The pfctl adapter (recorded divergence)

The K277 rules use three Raven extensions that K274's pfctl grammar does not
implement: `set default pass|block`, the `pass quick in` word order (pfctl wants
`pass in quick`), `static-port`, and `proto any`. For the **pfctl load only**,
the harness drops the `set default` line, moves `quick` after the direction,
and drops `static-port`/`proto any`; every actual rule is still parsed and
printed by pfctl. The engine always parses the original file, so the filter
semantics are unchanged. This is a test adapter, not a product change; the
underlying pfctl subset divergence is owned by K274.

## 4. Mutations

`run.sh` proves the harness is not decorative:

* `--mutate-verdict` flips the first expected verdict; the engine assertion must
  fail (`mutation caught: Verdict`).
* `--mutate-pcap` flips the first decoded TCP destination port; the first
  tcpdump line assertion must fail (`mutation caught: Pcap`).

Both mutations run in memory/`build/pf-e2e/`; the fixture tree is not modified.

## 5. Evidence and artifacts

`run.sh` writes:

| Path | Content |
|---|---|
| `build/pf-e2e/log.txt` | per-scenario PASS/SKIP/KNOWN and the summary |
| `build/pf-e2e/report.json` | schema 1: summary + one row per scenario/status/detail |
| `build/pf-e2e/<scenario>.pcap` | the exact capture decoded in the run |
| `build/pf-e2e/log-mutate-{verdict,pcap}.txt` | mutation runs |
| `build/pf-e2e/hashes.txt` | sha256 of the K277 manifest/semantics, the report and every pcap |

## 6. Donor reconciliation

Test-only. The expected verdicts come from the K277 oracle (which records the
FreeBSD/OpenBSD pf functions) and the tools keep their own provenance:

| Component | Donor behaviour | Licence decision |
|---|---|---|
| `raven-pf` | FreeBSD `sys/netpfil/pf/pf.c`, `pf_lb.c`, `pf_table.c` `@13334440` (K273 report) | product code already ported/reviewed; this ticket only consumes it |
| `raven-pfctl` | FreeBSD `sbin/pfctl` `parse.y`, `pfctl_parser.c`, `pfctl_table.c` `@13334440` (K274) | product code already reviewed; the e2e uses the library |
| `raven-tcpdump` | NetBSD `external/bsd/tcpdump/dist/*` `@36eddeb`, 4-clause: read-only donor, clean-room implementation (K281) | no donor code copied here; the e2e only calls the library |

The harness, frame builders, pcap writer use and the shell runner are first-party
test tooling; no donor code is copied.

## 7. Known gaps

1. **Fragment port-rule divergence** — the donor `pf_match_rule()` rejects a rule
   with `port_op` for `PF_VPROTO_FRAGMENT` (`FreeBSD pf.c:5997-6002`); `raven-pf`
   0.1.0 admits the fragment. The e2e records it as `KNOWN` and reports it to the
   pf lane; the K277 fixture keeps the donor expectation.
2. **Reassembly** — `pf_reassemble` is outside the 0.1.0 scope; the scenario is
   `SKIP` with `D-pf-frag-state`.
3. No QEMU: the path is host-level (pfctl/engine/tcpdump libraries and the pfctl
   binary); the in-guest packet path is the separate `pf-core` qtest.
