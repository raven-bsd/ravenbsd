<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->
# Shared conformance harness (S172)

The conformance tickets under `tests/conformance/<topic>/` share one guest row format and one
host runner. A topic supplies a probe and two plain-text tables; the harness supplies the
reporting, mutation sections, image assembly, QEMU invocation and the final exit mapping.

## Topic layout

```text
tests/conformance/<topic>/
  rows.tsv                 name <TAB> requirement <TAB> gate
  mutations.tsv            mutation <TAB> targeted-row <TAB> requirement
  build.sh                 builds build/conformance/<topic>/conformance-probe
  <service>.service.toml   one-shot service; exec is /bin/conformance-probe
  <probe>.rs               the topic rows
```

`rows.tsv` lists every normal row. `gate = 1` means the row must pass for the topic to pass.
`mutations.tsv` names a controlled mutation and the row it must turn red. A mutation encodes what
a broken implementation would believe (for example that `dup` copies the file offset); against
the correct product that row fails, which is the negative check.

## Guest row contract

The probe includes `conformance.rs` and prints one serial line per row:

```text
CONFORM <topic> begin
CONFORM <topic> row=<name> status=PASS|FAIL detail=<key=value ...>
CONFORM <topic> summary passed=<n> total=<n> failures=<n>
CONFORM <topic> done failures=<n>
CONFORM-MUTATION topic=<topic> begin=<mutation>
CONFORM-MUTATION topic=<topic> end=<mutation> failures=<n>
```

`detail=` carries the exact bytes, offsets and errno values that the row asserted, so a failure is
diagnosable from the serial log alone. The probe is one-shot and its service uses
`exit_when_exits`, so the process exit is the row result as well.

## Host runner

```sh
python3 tests/conformance/harness/runner.py --self-test
python3 tests/conformance/harness/runner.py --topic tests/conformance/descriptors
python3 tests/conformance/harness/runner.py --topic tests/conformance/descriptors \
    --phase mutation --mutation dup-copies-offset
```

The runner builds the probe, packs the selected profile (`--profile`, default `release`), overlays
`/bin/conformance-probe` and the topic service into the initfs, and runs the generated qtest case.
It then parses the row contract and owns the result:

* normal phase: 0 when every gate row passed and every controlled mutation turned its row red;
  1 when a gate row failed or a mutation was not caught; 2 on a build/image/QEMU failure.
* mutation phase: 1 when the targeted row turned red (the mutation was caught, the expected test
  result); 0 when it stayed green (the negative check failed); 2 on a harness failure.

qtest runs with `expect_exit = "any"`, because the probe intentionally exits non-zero when a row
fails or a mutation is caught. The runner uses the row lines, not the QEMU exit status, so a red
row is reported as a red row and never as a timeout or an unmatched `init exited (0)` line.

## Reuse by another conformance ticket

1. Create the ticket's own topic directory (its `Owns`) and put `rows.tsv`, `mutations.tsv`,
   `build.sh`, a service file and the probe there.
2. Include `tests/conformance/harness/conformance.rs` as the row module.
3. Emit one row per assertion with exact detail values.
4. Run the shared runner with `--topic <your-topic> --service <your-service-name>`.

No topic file is edited by another ticket; the harness is the shared, read-only dependency.
