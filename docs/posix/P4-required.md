<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P4 required utility acceptance

Status: OBSERVED (P4a8, base `origin/master` `60ec32aa`). P4a7 leaves exactly seven required
utilities. This note is the single acceptance surface for them; the utilities are implemented by
P4a3-P4a6, not here.

## Runner

```sh
sh tests/posix/utility/required/runner.sh
sh tests/posix/utility/required/test.sh
```

The runner executes each tool's own per-tool host test and prints one line per row:

* `[PASS] <tool>: <test>` - the test exists and exited 0.
* `[FAIL] <tool>: exit <n>` - the test exists and failed; the log is indented under the line.
* `[DEFECT] <tool>: missing ...` - the tool source or its per-tool test is absent.

It prints `required-runner: <N> pass, <N> fail, <N> defect` and exits 0 only when all seven rows
pass. A missing or failing row is never turned into a pass.

## Seven required rows

| Utility | Owner | Exact per-tool command | Current state at `60ec32aa` |
|---|---|---|---|
| `localedef` | otto / P4a4 | `sh tests/posix/utility/simple3/localedef/test.sh` | DEFECT: tool and test absent |
| `lp` | tim / P4a6 | `sh tests/posix/utility/simple5/lp/test.sh` | DEFECT: tool and test absent |
| `newgrp` | mel / P4a5 | `sh tests/posix/utility/simple4/newgrp/test.sh` | DEFECT: tool and test absent |
| `time` | bob / P4a3 | `sh tests/posix/utility/simple2/time/test.sh` | DEFECT: tool and test absent |
| `times` | bob / P4a3 | `sh tests/posix/utility/simple2/times/test.sh` | DEFECT: tool and test absent |
| `tput` | bob / P4a3 | `sh tests/posix/utility/simple2/tput/test.sh` | DEFECT: tool and test absent |
| `ulimit` | bob / P4a3 | `sh tests/posix/utility/simple2/ulimit/test.sh` | DEFECT: tool and test absent; also blocked on the missing `getrlimit`/`setrlimit` ABI (P4a7 review) |

Current observed transcript:

```
$ sh tests/posix/utility/required/runner.sh
[DEFECT] localedef: missing userland/bin/localedef tests/posix/utility/simple3/localedef/test.sh
[DEFECT] lp: missing userland/bin/lp tests/posix/utility/simple5/lp/test.sh
[DEFECT] newgrp: missing userland/bin/newgrp tests/posix/utility/simple4/newgrp/test.sh
[DEFECT] time: missing userland/bin/time tests/posix/utility/simple2/time/test.sh
[DEFECT] times: missing userland/bin/times tests/posix/utility/simple2/times/test.sh
[DEFECT] tput: missing userland/bin/tput tests/posix/utility/simple2/tput/test.sh
[DEFECT] ulimit: missing userland/bin/ulimit tests/posix/utility/simple2/ulimit/test.sh
required-runner: 0 pass, 0 fail, 7 defect
exit 1
```

A green run is `required-runner: 7 pass, 0 fail, 0 defect` and exit 0. The per-tool tests are the
inner commands of the owner runners (`run-simple2.sh` through `run-simple5.sh`); if an owner lands
a different per-tool path, the runner reports DEFECT until the path is aligned, never PASS.

## Self-test

`tests/posix/utility/required/test.sh` exercises the runner with a controlled temporary tree:
`time` passes, `times` fails and the remaining five are missing. It asserts the runner reports
`1 pass, 1 fail, 5 defect`, exits non-zero, and never passes a missing row. It also checks the real
tree has exactly seven row lines and an exit status consistent with its summary, so the test stays
valid before and after the owner tickets merge.

No utility, profile, evidence or generated POSIX document is changed by P4a8.
