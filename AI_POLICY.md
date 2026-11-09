# RavenBSD AI policy

RavenBSD welcomes AI-assisted contributions. This is a design goal of the project, not a
tolerance. The rules below keep that honest. A copy of this file ships in the source repo.

1. **AI contributions are welcome.** Code, docs, tests, and reviews may be produced by LLMs.
2. **Disclosure.** Every commit that contains AI-produced work carries a trailer:
   `Assisted-by: <tool or model>` (for example `Assisted-by: DeepSeek`). The ticket ID goes in the
   subject: `S04: parse BootInfo memory map`.
3. **A human or the lead is accountable.** Nothing merges without review by the lead (or the
   project owner). The author of a patch is never its only reviewer.
4. **Same bar for everyone.** Tests, `make check`, provenance headers, review. AI-written code is not
   trusted more or less than human-written code. Security-critical crates (listed in
   `process/REVIEW-CHECKLIST.md`) need line-by-line review of every `unsafe` block.
5. **Licence hygiene is mandatory.**
   - Copy only from donors whose tier allows it (`architecture/12-licensing-and-provenance.md`).
   - Never reproduce GPL/AGPL/proprietary code from memory or from training data. If a behaviour must
     match a tier-C project, the clean-room split applies (a spec author reads it, an implementer does not).
   - Every copied file keeps its origin: SPDX identifier, upstream path and revision in
     `PROVENANCE.toml`.
   - If you suspect text you produced is a verbatim copy of non-permissive code, stop and say so in
     the report. Honest reporting is never punished; hiding it is.
6. **No secrets, no telemetry, no network access from code under test** unless the ticket says so.
7. **Reports are evidence, not prose.** A report lists the commands run and their output. A claim
   without a command is rejected.
8. **Copyright.** Contributors keep their copyright; the project licence is BSD-3-Clause
   (`DECISIONS.md` DR-07). The legal status of AI-generated material is unsettled in many
   jurisdictions (UNVERIFIED); the project owner should obtain legal advice before commercial use.
