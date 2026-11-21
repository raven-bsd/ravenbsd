<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release host evidence (K412 refresh)

Refreshed 2026-11-21T22:36:00Z at base `8bbdc97fd50a2c4997a8086c51f3d4f184a91ef2`.
S116b's cold-start artifacts were imported from `origin/dave/S116b` (`fe46e124`); the x86_64 and
aarch64 release images were built in this lane at `8bbdc97f` under the measured 1200 s caps. The
source ticket reports in `ravenbsd-prjmgmt/tickets/<lane>/reports/` remain the primary record;
this directory duplicates small artifacts and records the exact hash of every referenced file.
No other heavy gate was rerun for this refresh.

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
| pf-conformance | K273, K274, K296, K277 | `6af246ac`, `12e12871`, `a31195af`, `b8169cce` | OBSERVED | **PASS local; gate NO-GO** | `selftest.sh` 14 executed/1 skip/1 known; `nextest -p raven-pf -p raven-pfctl -p raven-tcpdump` 213/213 | `tests/evidence/release-host/pf-manifest.tsv`; `pf-semantics.tsv` | `b23beb4c6b3d16ac79d04d03c6451243d99aa8347ac7a854810967b254dd555b`; `a8b21d3a8e0bef12e370cfd08dc41e62ef0346a1de7d67f3bc35b1af70d4ed84` |
| pf-post-k273 | K371 | `5d1e1efe` | OBSERVED | **PASS** | `net-postpf-default` 219,040 ms; `net-postpf-pf` 86,729 ms; both exit 33 | `tests/evidence/release-host/postpf-default.serial.log`; `postpf-pf.serial.log` | `496810bcfad5d1ea044945db48655fe674075ce70f856f655a6409e2237f798e`; `6367593e949e62890d0d794dc290b4cc81585991bdfc9407093f337e1ff4e95d` |
| pipe-poll | K370, K372 | `f3c10013` | OBSERVED | **PASS pipe; SSH BLOCKED** | `pipe-poll` qtest pass; `raven-libc-platform` 584/584; K195 tap 0/10 (netd queued-TX) | `docs/pipe-poll.md` | `33b151bd1c910e8f3b5c1585ebe6d0a9c6bb0162cdef44fc79af69a477da8a48` |
| image-x86_64 | REL6 | `8bbdc97f` | OBSERVED | **PINNED** | 335,544,320 B; sha256=`22477a19411b7ba7ee0317415e7c6f1c336abced94797e21f27d323dfc65b586`; `timeout 1200 cargo xtask image --arch x86_64 --profile release --output build/release/packet-x86_64.img`; 248 s | `tests/evidence/release-host/image-pins.txt`; `image-x86_64-build.log` | `fc3302e17b112eae39b4568ce7956b724219478f9000696212a23a6cc7638c25`; `501a9c3254d9a60a3d170326d87a1f40147ada559bcf34d37a70a8d9f50f25f2` |
| image-aarch64 | REL6 | `8bbdc97f` | OBSERVED | **PINNED** | 335,544,320 B; sha256=`60cb51563aef74c04e27d3cdfd686b393cd52665e525011b43f03bd4948538ab`; `timeout 1200 cargo xtask image --arch aarch64 --profile release --output build/release/packet-aarch64.img`; 228 s | `tests/evidence/release-host/image-pins.txt`; `image-aarch64-build.log` | `fc3302e17b112eae39b4568ce7956b724219478f9000696212a23a6cc7638c25`; `4b60cdcc601f08605cd4c4f2e407b1647cecd3790aac106f14c572390686fbde` |
| cold-start | S116b | `b58c5ac97` (artifacts `fe46e124`) | OBSERVED | **PASS 5/5** | KVM4 kernel→login 250/558/830/710/428 ms; median 558, p90 830, max 830; image sha256=`24c42352f5c40f09b853e2f82c52a77acd218df7bfc7e9fedd6968ad14af966e`; `timeout 1200 sh scripts/boot-time.sh --smp 4 --runs 5 --out /tmp/opencode/s116b-boot-time` | `tests/evidence/release-host/cold-start-boot/README.md`; `run.log` | `1c52a3c071744132f2b702bace5212e3d7c5284f300e16727368ca8ab4d7404b`; `b0f832278464b3006a3083e1c225a27bc9b3e817d8270dbe434800c9da78894d` |
| cold-start-steps | S116b | `b04e71ad` | OBSERVED | **PASS 5/5** | KVM4 `login:`→`Password:` median 57.2 ms, p90 61.1; `Password:`→`raven#` median 105.8 ms, p90 112.9; both 5/5 <1000 ms | `tests/evidence/release-host/cold-start/README.md`; `run.log` | `addec918bebf02044464e2c2458e8f7f3e4b3fa8f561daf0bde52982bec92c67`; `bbdeb13d289a9a98c0a2a2dcbb08957c6bfd4698a90f08c0f265099e441d0b1f` |
| pi4-board | owner hardware | - | HARDWARE/UNVERIFIED | **HARDWARE** | no board log/serial/photo; host image is not a board result | none | - |

## Release board NO-GO / HARDWARE rows

The release truth is `tickets/release/0.1.0-gates.tsv` (SHA-256
`b460db58717a643a0a46b9045c5c3218af48b6cf2b634fe5c40a9479de2b079f`). The packet keeps these
rows explicit; they are not softened by the refreshed host rows.

| Board row | Status | Evidence | Tag |
|---|---|---|---|
| SMP TLB shootdown under load | **NO-GO** | S159: 20/20 TCG4 `posix-fs-smp` failures at load 88-126; TLB timeout at iterations 4 (FSTAT) and 10 (EXIT); K376 reproducible load-matched gate; `tickets/otto/reports/S159.md` | OBSERVED |
| SSH remote login | **BLOCKED** | K195 rev5 + K382 negative: virtio-tx pass/used-ring reclaim; stall above TX; K195 differential pending; no login claim | OBSERVED |
| pf packet filter | **MET** | K389 tree `f469b2fd`/merge `962603b1`: pf-core/kvm2/kvm4/tcg4/net-postpf-pf/socket-inet all pass; real drop + default `netd: pf absent` | OBSERVED |
| Pi 4 SD/UFS + HDMI/USB | **HARDWARE** | no board, serial, monitor or keyboard attached | HARDWARE/UNVERIFIED |
| Pi 4 E-Ink photo | **HARDWARE** | no panel/photo; E4 board run pending | HARDWARE/UNVERIFIED |
| 24 h soak + freeze artifacts | **BLOCKED** | K228 soak harness merged; owner 24 h run not executed | OBSERVED |
| Supply chain | **BLOCKED (publish only)** | K390 merge `8ca70577`: 771 crates, local tarball sha `ae2a4482...`, `cargo-deny 0.22.0` pass, exceptions recorded; `artifact_url=UNPUBLISHED`, no upload | OBSERVED |
| Cross-OS benchmark (ravenmark) | **MISSING** | no ravenmark result consumed | UNVERIFIED |
| TCP stack release-grade | **NO-GO** | K295 packet complete, K365 soak, RACK 219/219; no SSH login, no 24 h soak | OBSERVED |
| Release evidence packet | **NO-GO** | this packet; K412 refresh at `8bbdc97f`; S116b cold-start and both image pins closed; remaining board rows explicit | OBSERVED |

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

## Verification

```sh
timeout 60 sh tests/evidence/release-host/check.sh
```

The check runs `sha256sum -c tests/evidence/release-host/SHA256SUMS`, verifies the nested K368 and
K365 manifests, and requires the packet to carry the `shell-hygiene`, `tcp-soak`, `rack-audit`,
`pf-conformance`, `pipe-poll`, cold-start and image-pin rows plus `HARDWARE/UNVERIFIED`. It does
not run QEMU.
