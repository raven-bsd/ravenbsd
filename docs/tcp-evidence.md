<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP provenance and evidence (K264)

Status: **DONE.** The FreeBSD/DragonFly entries for the landed ports are validated and added; the
packet-level matrix ran all ten cases. One TCP finding remains outside K264: the Raven handshake
advertises a window-scale option to a peer that did not negotiate it. The incomplete ports stay
quarantined and contribute no entry.

## Results

| Gate | Result |
|---|---|
| audit | **pass**: 9 imports (7 FreeBSD, 1 DragonFly, 1 pre-existing smoltcp/0BSD), 27 claims (17 quarantined), 13 donor files, 0 violations, 0 donor-missing |
| matrix | **ran 10/10**: Linux 5/5 pass; Raven zero-window, window-update, retransmit, fin-rst pass; Raven handshake **finding** |
| unit | **891/891 passed**, 0 skipped (merged master) |
| checker | **pass**: 2 negative fixtures rejected, 1 valid fixture accepted, 2 mutations caught |
| provenance | 0 violations; generated evidence run directories are excluded, while scripts/README/port-claims/docs stay scanned |
| `make check-fast` | **`check-fast: PASSED`** |

## Landed provenance entries

The exact entries from the landed reports were added to `PROVENANCE.toml`:

| Ticket | File | Donor | Licence |
|---|---|---|---|
| K268 | `crates/raven-tcpip/src/sack.rs` | DragonFly `sys/netinet/tcp_sack.c` | BSD-3-Clause |
| K270 | `crates/raven-tcpip/src/ecn.rs` | FreeBSD `tcp_ecn.c`, `tcp_ecn.h`, `tcp.h` | BSD-3-Clause |
| K270 | `crates/raven-tcpip/src/pmtu.rs` | FreeBSD `tcp_input.c`, `tcp_subr.c`, `ip_icmp.c`, `tcp_var.h`, `tcp.h` | BSD-3-Clause |
| K271 | `crates/raven-tcpip/src/socket.rs` | FreeBSD `tcp_usrreq.c`, `tcp_subr.c`, `uipc_socket.c` | BSD-3-Clause |
| K271 | `crates/raven-tcpip/src/listen.rs` | FreeBSD `uipc_socket.c` | BSD-3-Clause |
| K272 | `crates/raven-tcpip/src/readiness.rs` | FreeBSD `sys/sys/socketvar.h` | BSD-3-Clause |
| K276 | `crates/raven-tcpip/src/syncache.rs` | FreeBSD `tcp_syncache.c`, `tcp_syncache.h`, `tcp_timer.c` | BSD-3-Clause |
| K313 | `crates/raven-tcpip/src/tcp_data.rs` | FreeBSD `tcp_output.c`, `tcp_timer.c`: `tcp_default_output`, `tcp_setpersist`, `tcp_timer_persist` | BSD-3-Clause |

K279 changes only the netd readiness test and is covered by K272's `readiness.rs` entry; it adds no
separate import. K313 (`tcp_data.rs`, merged master) is in the same claim file as a landed FreeBSD
port. The pre-existing smoltcp DHCP entry (`src/dhcp.rs`, 0BSD) is unrelated.

The pinned donors are FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb` and DragonFly
`94448f622a06cde90ba42d688e9cab2448ad9ab3`. The sparse worktrees are read through the pinned git
objects. The donor audit checks the files, the licence class and the named functions (for example
DragonFly `tcp_sack_update_lostseq`/`tcp_sack_islost`, FreeBSD `tcp_usr_connect`/`tcp_usr_close`,
`syncache_add`/`syncache_expand`, `soreadable`/`sowriteable`).

## Quarantine

No NetBSD TCP entry exists. The NRL 4-clause NetBSD files remain NO-COPY. The following tickets are
quarantined and contribute no entry: K254-K261 (superseded NetBSD-derived attempts), K265, K266,
K267, K269, K273, K274, K275, K277 and K278 (not landed/incomplete). K265's merge is reverted on
master; K266/K267/K278 are DONE reports but not in the landed list.

## Matrix evidence

The matrix builds the net-conformance initfs and image once, then runs the five scenarios against
Linux and Raven with `--no-build`:

```text
build initfs: 431.881 s, exit 0 (logs/build-initfs.log)
build image:   15.668 s, exit 0 (logs/build-image.log)
linux  handshake     pass (1.141 s)
linux  zero-window   pass (5.006 s)
linux  window-update pass (5.386 s)
linux  retransmit    pass (1.339 s)
linux  fin-rst       pass (4.128 s)
raven  handshake     FAIL (35.149 s): SYN-ACK mss=1460 ws=1 sack=True; plain-peer ws=1 win=65535
raven  zero-window   pass (27.493 s)
raven  window-update pass (17.924 s)
raven  retransmit    pass (27.258 s)
raven  fin-rst       pass (46.574 s)
```

The Linux reference for the same handshake is `SYN-ACK mss=1460 ws=7 sack=True; plain-peer
ws=absent win=64240`. The Raven stack offers a window-scale option to a peer that did not offer
one; this is a TCP port finding, not a K264 evidence blocker. The pcap is
`tests/evidence/tcp/run-20261121T003615Z/pcaps/raven-handshake.pcap`; the serial is
`tests/evidence/tcp/run-20261121T003615Z/serial/raven-handshake.log`.

The first matrix attempt timed out at 1200 s because the runner rebuilt inside every target
scenario after a 300 s timeout; the preserved log is
`/tmp/opencode/k264-matrix-timeout-001455Z`. The fix builds once and uses `--no-build` for all
target scenarios; the complete run took about 620 s inside the same 1200 s cap.

## Evidence packets

* `tests/evidence/tcp/run-20261121T003615Z/` - matrix packet: `matrix.json`, ten pcaps, five
  serial logs, build logs and `sha256sums.txt`. Manifest commit `c3a2d951`, `dirty: yes` because the
  entries/claims were committed in `3cda2923` immediately after the run; `manifest.json` records
  the input hashes.
* `tests/evidence/tcp/run-20261121T004851Z/` - clean audit/unit/checker packet: `audit.json`,
  `audit.txt`, `unit.json` (710/710), `logs/checker.log`, `summary.json`, `sha256sums.txt`.
  Manifest commit `3cda2923`, `dirty: no`.

Every packet has a SHA-256 manifest; `sha256sums.txt` covers the artifact set.

## Reproduce

```sh
timeout 60  sh scripts/tcp-evidence.sh --selftest
timeout 30  python3 scripts/tcp-evidence_ut.py
timeout 120 sh scripts/tcp-evidence.sh --audit \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --freebsd-root /home/bo/work/ravenos-workspace/ravenbsd-resources/freebsd \
    --dragonfly-root /home/bo/work/ravenos-workspace/ravenbsd-resources/dragonfly
timeout 1200 sh scripts/tcp-evidence.sh --matrix
timeout 240 sh scripts/tcp-evidence.sh --audit --unit --checker \
    --reports-root /home/bo/work/ravenos-workspace/ravenbsd-prjmgmt/tickets \
    --freebsd-root /home/bo/work/ravenos-workspace/ravenbsd-resources/freebsd \
    --dragonfly-root /home/bo/work/ravenos-workspace/ravenbsd-resources/dragonfly
```

The matrix cap is the measured cold run + margin; the build is cached after the first run. A
cap expiry is a failure with the preserved log, not a reason to raise it.

## Limits

* The handshake window-scale finding is a TCP port gap; it needs a TCP ticket and wire evidence.
* The quarantined ports must be reworked and resubmit their exact entries.
* K271's report notes name `solisten_proto`; the pinned FreeBSD tree has
  `solisten_clone`/`solisten_enqueue`, which are the verified symbols. Reviewer confirmation is
  still required for that naming divergence.
* No 24 h or live SSH/tap claim is made here; live SSH remains the integration gate.
* No product code is changed by K264; the only product-tree files touched are `PROVENANCE.toml`
  and the evidence-config `.provenance-ignore`. Generated `tests/evidence/tcp/run-*` directories
  are provenance-ignored; the scripts, README, port-claims and docs stay scanned.
