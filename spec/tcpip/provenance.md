# Provenance and clean-room statement

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: C01 spec wave, rev 1. This file is the provenance record for
`spec/tcpip/**`; it is not a specification.

## 1. What this is

`spec/tcpip/**` is fresh BSD-3-Clause prose, tables and test vectors for the
clean-room `raven-tcpip` stack (DR-28, DR-29). It contains no executable code
and no copied donor material.

* No `[[import]]` entry is added to `PROVENANCE.toml`. A clean-room
  implementation has none (DR-28; `architecture/12-licensing-and-provenance.md`
  §3b).
* No CDDL, GPL, AGPL or proprietary code, comment or table was copied,
  translated or closely paraphrased.
* The donor stacks were read as **behavioural references** by the C01
  spec-author session only. The C02-C27 implementer sessions must not read
  them; their tickets say so and their reports must repeat it.
* The RFCs are normative. Where this spec differs from an RFC, the difference
  is marked `DEVIATION` with a reason in the relevant file
  (`state-machine.md` §15, `rto.md` §3, `cwnd.md`).

## 2. The five DR-28 steps for this component

| Step | C01 status | Evidence |
|---|---|---|
| 1. Research the donor thoroughly | Done for illumos and NetBSD; FreeBSD TCP subtree was absent; smoltcp read for the comparison. | `research-log.md` |
| 2. Write the functional/architectural specification first | This tree: state machine, features, observability, oracle, vectors. | `README.md` §4 |
| 3. Independent implementation, fresh session | Not in this ticket. C02-C27 run in a fresh session that has not read the donor; they use RFCs + this spec only. | C1/C2 wave split in `process/CARL.md` |
| 4. Compatibility validation | C20 (packetdrill), C22 (interop), C24 (oracle) are the independent black-box tests. C01 defines the measurement plan in `oracle.md`. | `oracle.md` |
| 5. Provenance record | This file plus `research-log.md`. | this file |

## 3. Donor material and licence

| Donor | Revision | Licence | What was read | What was not copied |
|---|---|---|---|---|
| illumos-gate | `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` | CDDL-1.0 (tier B) | counter/MIB surface, property-table shape, TIME-WAIT structure, timer surface, SACK receiver shape, selected reset/sequence paths | no code, comments, identifiers-as-written or table layout |
| NetBSD src | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | BSD-2/3 (tier A) | counter list, sysctl surface, state-machine edges, SACK/time-wait/SYN-cache operational shape | no code or distinctive expression |
| smoltcp | `8b0e5c8598647fe92e2a1b87932d1932354fee0b` | 0BSD (tier A) | feature comparison only, with `file:line` in `features.md` | no code; the comparison is a measurement/read of public source, not a port |
| FreeBSD src | `133344405f532bd54ff3629a77d4c4e062f01fdb` | BSD-2 (tier A) | nothing: the sparse checkout has no `sys/netinet` TCP tree | n/a |

The illumos reading is the tier-B clean-room case of DR-28: read for
behaviour, write a functional specification, implement independently, validate
by black-box tests, keep the record. It is **not** a CDDL port. The names in
`observability.md` are ours (`tcp.active_opens`, `tcp.rto_min`, ...), not
illumos property names. The table shape (name/type/min/max/default/unit/
stability) is a generic typed-property idea required by C19; the rows and
values are our choices.

## 4. How the vectors were produced

* `headers.toml`: packet bytes constructed field by field from the RFC
  layouts; checksums computed with the 16-bit one's-complement algorithm and
  shown in `headers.md` §4. No packet was captured and no stack's parser
  produced the expected values.
* `rto.toml`: the RFC 6298 integer estimator in q8 fixed point; the formulas
  and worked steps are in `rto.md`. No implementation was run.
* `cwnd.toml`: the NewReno and CUBIC rules stated in `cwnd.md`, evaluated
  with integer arithmetic; the closed-form CUBIC `W_cubic`/`W_est` values are
  shown with intermediate steps.
* `sack.toml`: the RFC 6675 `IsLost`/`SetPipe`/`NextSeg` definitions applied
  to hand-written range sets; `sack.md` shows the counts and the selected
  segment for each case.

The arithmetic was checked with a small integer calculator (not a network
stack, not a parser implementation) and then written into the TOML by hand.
The `.md` companions show enough intermediate work for a reviewer to
recompute a sample. No vector was produced by running `raven-tcpip`, smoltcp,
illumos or NetBSD.

## 5. Clean-room boundary rules for the implementers

C02-C27 must follow these without exception:

1. Read the RFCs named in the ticket and `spec/tcpip/**`. Do not open
   `ravenbsd-resources/illumos-gate`, `.../netbsd`, `.../freebsd` or
   `.../smoltcp` TCP source.
2. Do not search the web for a TCP implementation to copy. RFC text and this
   spec are sufficient; if they are not, write a `QUESTION` report.
3. If a donor identifier or distinctive comment appears in your code, stop and
   rewrite it; the spec may have leaked it and the reviewer must know.
4. Keep the crate BSD-3-Clause and `#![forbid(unsafe_code)]` per C15.
5. Report exactly which sources you consulted; "the spec and the RFCs only"
   is the expected answer for a clean-room implementer.

## 6. Reviewer checklist

* [ ] `spec/tcpip/**` contains no code and no donor identifiers/comments.
* [ ] `research-log.md` names every donor file that was opened.
* [ ] `features.md` has a `file:line` for every smoltcp comparison; the
      `UNVERIFIED` rows are not used as requirements.
* [ ] Every state-machine row cites an RFC section; every `DEVIATION` has a
      reason and an observable effect.
* [ ] `observability.md` covers every counter and tunable required by C01 §4.4
      and every name is unique.
* [ ] `headers.toml` >= 60 cases, `rto.toml` >= 20, `cwnd.toml` >= 15,
      `sack.toml` >= 15; all parse as TOML; the `.md` arithmetic checks.
* [ ] No `[[import]]` entry for `raven-tcpip` in `PROVENANCE.toml`.
* [ ] The C02-C27 tickets and the C2 wave manifest carry the "no third-party
      stack" instruction.

Reviewer sign-off: pending (this is the C01 spec commit; the reviewer records
acceptance in the C1 checkpoint, not by editing this file).

## 7. Limitations recorded honestly

* FreeBSD TCP source was not available in the sparse checkout; no FreeBSD
  claim is made.
* Some smoltcp details are marked `UNVERIFIED` in `features.md` §12; they may
  not become requirements.
* No packet capture was possible on the authoring host (`tcpdump` lacks
  permission); `oracle.md` records what was measured with `ss`/`nstat` and
  marks the packet-level rows `NOT-MEASURED`. C24 must rerun them with
  `CAP_NET_RAW` or a TAP.
* The CUBIC vectors pin the RFC 8312 target at RTT boundaries, not every
  intermediate per-ACK value; `cwnd.md` §3.2 states the contract. If C24
  measures a material difference between our curve and the reference, the
  reviewer may ask for a tighter per-ACK vector in a follow-up spec change.
