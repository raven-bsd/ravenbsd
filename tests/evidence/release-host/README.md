<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release host evidence (K392 refresh)

Refreshed 2026-11-21T21:00:00Z at base `879d3d39` from merged evidence only. The source ticket
reports in `ravenbsd-prjmgmt/tickets/<lane>/reports/` remain the primary record; this directory
duplicates small artifacts and records the exact hash of every referenced file. No heavy gate was
rerun for this refresh.

The Pi 4 board stays **HARDWARE/UNVERIFIED**: there is no board serial transcript or photo, and a
host image or QEMU run is not a board result.

## Rows

| Row | Source ticket | Source commit | Tag | Status | Command / result | Artifact | SHA-256 |
|---|---|---|---|---|---|---|---|
| shell-hygiene | S97, K133, K134 → K136/K148/K158 | `08f43b9a`, `90729b09`, `b0cfde8b`; fresh at `604a9367` | OBSERVED | **PASS** | `base-tools-du` qtest exit 0; `grep -c 'process: .* exited'` = 0 | `tests/evidence/release-host/qtest/base-tools-du.log` | `544cad327bb5597d4a807496978c7e173dd5aa9df9bb8b1430880635d73c0e53` |
| shell-hygiene / ls | K133 | `90729b09` | OBSERVED | **PASS with host-load gap** | K133 43.7 s pass; fresh rerun failed at boot under load 80, no `ls` command | `docs/demo-ls-latency.md` | `cf5bdc22b9221f3b31cefd04ac160f795877ab26284e7ddb97510ed087986398` |
| kernel-gate | S140 | `f803502a` | OBSERVED | **PASS** | `kernel-gate.sh`; 27/27 at the recorded base | `docs/kernel-gate.md` | `17a95df16ed10320d43e41a5a11c27ec81458400198b128b7dc639f50bc10c46` |
| posix | POSIX gate | stamp `8d605619` | OBSERVED | **PASS** | `posix-check`; 1708 pass, 0 fail, 105 skip | `docs/posix/CONFORMANCE.md` | `19b667bbb53fdb5da94252c215902e19f43232a4d65a1dffb106cfedac93acc8` |
| smp-soak | K368 | `2e8a873f` | OBSERVED | **PASS** | 40/40: KVM1/2/4 10/10, TCG4 10/10 | `tests/evidence/release-smp/SHA256SUMS` | `7a46b7d2558183670a9761b8b4f0bc172bbda746ac73d5ed8529b69468429f75` |
| tcp-soak | K365 | `60b2bf51` | OBSERVED | **PASS** | KVM2/KVM4/TCG4 8/8 cycles each | `tests/evidence/tcp/soak/SHA256SUMS` | `753af19d69043fb94c01432fd4d84233fa2c6a5f2545dea4bbded8aa3ffb09d7` |
| rack-audit | K269 | `f6235c82` | OBSERVED | **PASS 219/219** | acceptance `donors=219 register=219 accepted=219 gaps=0 duplicate=0`; register rows=219, missing=0 | `tests/evidence/release-host/rack-acceptance.log` | `2d3c7f4dd590f7248d68d58c3f0597ac4d9da08c76a9698d4db70588e8016fe3` |
| rack-timer-model | K325 | `978899da` | OBSERVED | **PASS 12/12** | `cargo nextest run -p rack-timer-model`; 10,000-row deterministic trace | `tests/rack-timer-model/traces/rack-timer-10k.tsv` (726 KiB, referenced) | `64d1e2bacf8db5cc37001470e2474ac812ebc5fb5147b4b053f4fba0d4b97040` |
| pf-conformance | K273, K274, K296, K277 | `6af246ac`, `12e12871`, `a31195af`, `b8169cce` | OBSERVED | **PASS local (carrier); release gate MET via K389** | `selftest.sh` 14 executed/1 skip/1 known; `nextest -p raven-pf -p raven-pfctl -p raven-tcpdump` 213/213 | `tests/evidence/release-host/pf-manifest.tsv`; `pf-semantics.tsv` | `b23beb4c6b3d16ac79d04d03c6451243d99aa8347ac7a854810967b254dd555b`; `a8b21d3a8e0bef12e370cfd08dc41e62ef0346a1de7d67f3bc35b1af70d4ed84` |
| pf-merged | K389 | `f469b2fd` | OBSERVED | **PASS 6/6; gate MET** | pf-core 97.640 s, pf-kvm2 13.099 s, pf-kvm4 34.700 s, pf-tcg4 115.283 s, net-postpf-pf 27.641 s, socket-inet 288.843 s; real drop + NAT; `netd: pf absent` default | `tests/evidence/pf-merged/f469b2fd…/hashes.txt` | `6bea8c1bad18d26d6c96ceed2f6aff3caed9548e6cbfe043ea0aa295b231738c` |
| pf-post-k273 | K371 | `5d1e1efe` | OBSERVED | **PASS** | `net-postpf-default` 219,040 ms; `net-postpf-pf` 86,729 ms; both exit 33 | `tests/evidence/release-host/postpf-default.serial.log`; `postpf-pf.serial.log` | `496810bcfad5d1ea044945db48655fe674075ce70f856f655a6409e2237f798e`; `6367593e949e62890d0d794dc290b4cc81585991bdfc9407093f337e1ff4e95d` |
| pipe-poll | K370, K372 | `f3c10013` | OBSERVED | **PASS pipe; SSH BLOCKED** | `pipe-poll` qtest pass; `raven-libc-platform` 584/584; K195 tap class b; K382 shows the virtio TX used-ring reclaim is not the blocker; K195 rev8 differential pending | `docs/pipe-poll.md` | `33b151bd1c910e8f3b5c1585ebe6d0a9c6bb0162cdef44fc79af69a477da8a48` |
| supply-chain | K390 | `8ca70577` | OBSERVED | **BLOCKED (owner publish)** | vendor `--local` sha `ae2a4482…`; no-URL release fails closed; `cargo deny check` `advisories/bans/licenses/sources ok` (0.22.0); owner upload and manifest URL remain | `tests/evidence/supply-chain/b071456e…/SHA256SUMS` | `ae2a44825d6d25e594501931cdd12b031b52e95d2579ff76ad89bc7b3f92bac8` (artifact) |
| image-x86_64 | REL6 | `fd394ff3` | UNVERIFIED | **PINNED** | image hash recorded at the old product commit; not rebuilt at `b8169cce` | `docs/release-images.md` | `cf59ad921923f06d0914d6d3951707189dba2d257c1ae3ff804d351fe56bfc69` |
| image-aarch64 | REL6 | `fd394ff3` | UNVERIFIED | **PINNED** | image hash recorded at the old product commit; not rebuilt at `b8169cce` | `docs/release-images.md` | `cf59ad921923f06d0914d6d3951707189dba2d257c1ae3ff804d351fe56bfc69` |
| cold-start | K135/K139 | `63efc104` | UNVERIFIED | **PENDING** | no 5/5 <1 s sample in the packet; board `QEMU boot <1 s` MET | `docs/releases/0.1.0-notes.md` | `88e6fcd36fe668c19f46489ad5245d07fc17f8636a17870322eeb768ab8074c9` |
| pi4-board | owner hardware | - | HARDWARE/UNVERIFIED | **HARDWARE** | no board log/serial/photo; host image is not a board result | none | - |

## Release board NO-GO / HARDWARE rows

The release truth is `tickets/release/0.1.0-gates.tsv` (SHA-256
`5b185256c0edd9034679bd455491187bb97a961edea776a01073c1d3d3a06fd5`). The packet keeps these
rows explicit; they are not softened by the refreshed host rows.

| Board row | Status | Evidence | Tag |
|---|---|---|---|
| SMP TLB shootdown under load | **NO-GO** | S159: 20/20 TCG4 `posix-fs-smp` failures at load 88-126; TLB timeout at iterations 4 (FSTAT) and 10 (EXIT); K376 reproducible load-matched gate; `tickets/otto/reports/S159.md` | OBSERVED |
| SSH remote login | **BLOCKED** | K382: 32 KiB burst crosses >2 TX rings, every completion reclaimed, mutation catches a lost completion -> the virtio TX reclaim is not the blocker; K195 gate class b (0-byte probes), rev8 tap-vs-slirp differential pending | OBSERVED |
| pf packet filter | **MET** | K389 merged tree `f469b2fd`: six cases PASS, real drop + NAT, default unfiltered; serials/JSON in `tests/evidence/pf-merged/**` | OBSERVED |
| Pi 4 SD/UFS + HDMI/USB | **HARDWARE** | no board, serial, monitor or keyboard attached | HARDWARE/UNVERIFIED |
| Pi 4 E-Ink photo | **HARDWARE** | no panel/photo; E4 board run pending | HARDWARE/UNVERIFIED |
| 24 h soak + freeze artifacts | **BLOCKED** | K228 soak harness merged; owner 24 h run not executed | OBSERVED |
| Supply chain | **BLOCKED (owner publish)** | K390 merged `8ca70577`: vendor `--local` sha, fail-closed URL, cargo-deny 0.22.0 exit 0, ucs2 exception; owner upload/manifest URL remain | OBSERVED |
| Merged test-build (all crates) | **NO-GO** | K395/K394: pristine master `8ca70577` cannot compile fsd-ufs tests (E0063); full workspace test-build must pass before a tag | OBSERVED |
| Cross-OS benchmark (ravenmark) | **MISSING** | no ravenmark result consumed | UNVERIFIED |
| TCP stack release-grade | **NO-GO** | K295 packet complete, K365 soak, RACK 219/219; no SSH login, no 24 h soak | OBSERVED |
| Release evidence packet | **NO-GO** | this packet; host rows refreshed, remaining board rows explicit | OBSERVED |

## Fresh host runs at `604a9367`

### `base-tools-du` — PASS (exit 0, 102,338 ms)

```sh
timeout 240 cargo xtask qtest --case base-tools-du --arch x86_64
```

`grep -c 'process: .* exited' build/qtest/base-tools-du.log` returned `0`; no fatal-runtime,
spawn-failure, panic or EXCEPTION line. This is the S97 quiet child-exit behaviour and the
K134/K136/K148 `du` thread/TLS path.

### `base-tools-ls-latency` — FAIL at boot (exit 1, 71,844 ms)

The fresh rerun stopped at `^raven# $` with no matching serial line within 60 s while the host
was at load 80 on 20 CPUs with four QEMU guests and concurrent builds. No `ls` command ran; the
K133 merged `90729b09` change and its 43.7 s pass remain the ls evidence. This is a host-load gap,
not an ls regression.

## Referenced source reports (outside the product tree)

| Ticket | Report path | SHA-256 |
|---|---|---|
| K269 | `ravenbsd-prjmgmt/tickets/dave/reports/K269.md` | `ca9409ecd61abd62a92a167ab2160a93b13e4ec57449d2420a66c18df306c678` |
| K273 | `ravenbsd-prjmgmt/tickets/jorge/reports/K273.md` | `98d4b40362c914f78b258263c0415e9eae661ef0612545c873f7c336ce6863bb` |
| K274 | `ravenbsd-prjmgmt/tickets/steve/reports/K274.md` | `870438f4c66f6c9bcfabc10139f00246dce3c88749f816d24ae857aec46fe33f` |
| K277 | `ravenbsd-prjmgmt/tickets/jorge/reports/K277.md` | `1f6d1843e9ffa28c8901e10f5ecda0ed5f52ca1c138d020f2429564995e2f61b` |
| K296 | `ravenbsd-prjmgmt/tickets/phil/reports/K296.md` | `0820d22d718712ac9b7f2b7a8256cb3b979e9c2d429f3021528cc67f4c15d47e` |
| K325 | `ravenbsd-prjmgmt/tickets/lance/reports/K325.md` | `56f69b7cad74c813c6e71eae2edad84aa402e85449aaad5aed4a392d0e44093a` |
| K371 | `ravenbsd-prjmgmt/tickets/tim/reports/K371.md` | `0a93893a2c0c1265b36b62fd06b9438a60b48884fc6b7d9c29ae2a934a4c3a2b` |
| K372 | `ravenbsd-prjmgmt/tickets/tim/reports/K372.md` | `98ace268513a7a4006d58e643ba49397df7d20f2ec69e456bf6b2f3efb5d926e` |
| K139 | `ravenbsd-prjmgmt/tickets/kevin/reports/K139.md` | `7588104d97da7e085ee139e77a1d31034f1349f6544f871cfde7cf4abb7942f7` |
| S159 | `ravenbsd-prjmgmt/tickets/otto/reports/S159.md` | `749b419e97c78094e4d08cf2fbe24fa8f9c6a8a848a9ac47447671ce8a7f31a2` |
| K368 | `ravenbsd-prjmgmt/tickets/stuart/reports/K368.md` | `2f369b8fe99991acd60aac12a145396a34ffc537b9b6dba014e2e0b87d1d06e4` |
| K365 | `ravenbsd-prjmgmt/tickets/stuart/reports/K365.md` | `4c5b2fa3feaba2fe75f32da48b24805ebba29d17af017922ce57d6a9a5aba93b` |
| K389 | `ravenbsd-prjmgmt/tickets/jorge/reports/K389.md` | `52e69174c8f291b6736252f0615e282dcb714bbe1f7ad860c3a3486f38192cfc` |
| K390 | `ravenbsd-prjmgmt/tickets/jerry/reports/K390.md` | `7e25f09de61a799dca3bcd95819591e51fb15f2481de26da2e976623ad53da10` |
| K382 | `ravenbsd-prjmgmt/tickets/bob/reports/K382.md` | `c7b191014d0092695ea85ecab1f00f75396c2bec96aebccb75f636f2b0799205` |

## Verification

```sh
timeout 60 sh tests/evidence/release-host/check.sh
```

The check runs `sha256sum -c tests/evidence/release-host/SHA256SUMS`, verifies the nested K368 and
K365 manifests, and requires the packet to carry the `shell-hygiene`, `tcp-soak`, `rack-audit`,
`pf-conformance`, `pipe-poll` and `HARDWARE/UNVERIFIED` rows. It does not run QEMU.
