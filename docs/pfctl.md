# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# pfctl (K274): the pf.conf front-end

`raven-pfctl` is the front-end of the FreeBSD `pfctl` port: it parses pf.conf,
keeps a faithful AST, prints the compact normalized form `pfctl -nv` produces,
and parses table files. It is `userland/bin/pfctl`, crate `raven-pfctl`, and it
never talks to the kernel pf device.

Status: **front-end only**. The pf ABI (K273) is not merged yet, so every
mutating action (`-f` load, `-T add`) is refused; the supported invocation is
the dry run `-nv -f pf.conf`. The status/show path and the netd wiring wait for
K273 and are out of this ticket's scope.

## Usage

```sh
raven-pfctl -nv -f /etc/pf.conf            # parse and print the normalized config
raven-pfctl -n -f /etc/pf.conf             # parse only; exit code is the verdict
raven-pfctl -nv -D ext_if=em0 -f pf.conf   # -D macros (name=value, repeatable)
raven-pfctl -nv -t web -T add -f web.txt   # parse and print a table file
raven-pfctl -h                             # usage
```

Options: `-n` dry run (required until the ABI exists), `-v` print, `-f file`,
`-D name=value`, `-t table`, `-T add|delete|replace|test` (`show`/`flush` need
the ABI). Exit codes: `0` success, `1` any parse, validation or usage error.
Parse errors are line-numbered: `pfctl: pf.conf:12: invalid address '…'`.

## Supported pf.conf subset

* macros: `name = "value"` / `name = value`, referenced as `$name` (expanded
  textually in definition order, `-D` seeds, cycle/depth bounded)
* tables: `table <name> [const] [persist] [counters] { addr, !addr }` and
  `table <name> file "path"`
* options: `set block-policy`, `set fail-policy`, `set require-order`,
  `set state-policy`, `set skip on`, `set optimization`, `set loginterface`,
  `set hostid`, `set timeout`, `set limit`
* filter rules: `pass`, `block [drop|return]`, `match`, `scrub`, `no scrub`,
  with `in|out`, `log`, `quick`, `on [! ]if`, `inet|inet6`, `proto`,
  `from`/`to` (addresses, `<tables>`, `{ lists }`, `!` negation, `any`,
  ports `= != < <= > >=` and `a:b` ranges and `{ port lists }`),
  `flags set/mask`, `keep|modulate|synproxy state`, `no state`
* NAT: `nat`, `rdr`, `binat` and their `no` forms, with `pass`, `log`, `on`,
  `inet|inet6`, `proto`, `from`/`to`, `-> address [port p] [pool]`
  (`round-robin`, `source-hash`, `bitmask`)
* `anchor "name"` (plain named anchor; nested bodies are not supported)
* `include "path"` (expanded in place when a loader is present, bounded depth 8)
* `#` comments, blank lines, multi-line brace lists

## Output format

The printer follows the donor's `print_rule` clause order: action (block rules
always print their explicit `drop`/`return`), direction, `log`, `quick`,
`on` interface, family, `proto`, source/destination, `flags`, state. When both
source and destination are unspecified the rule prints the ` all` shorthand;
`pass`/`match` rules with state tracking and no explicit flags print
`flags any`; `pass` rules keep state by default (`no state` opts out). Table
bodies print space-separated (`table <web> { 192.0.2.10 192.0.2.11 }`), the
donor's `print_tabledef` form.

The output is idempotent: re-parsing and re-printing the normalized config
produces the same text (asserted by the fixture tests).

## Donor map

FreeBSD `sbin/pfctl` at `133344405f532bd54ff3629a77d4c4e062f01fdb`,
BSD-2-Clause. The Rust implementation is first-party; no donor code is copied.

| Topic | FreeBSD donor file/function | Raven file |
| --- | --- | --- |
| CLI, options, dry run, exit path | `sbin/pfctl/pfctl.c` (`main`, getopt, `PF_OPT_NOACTION`, `usage`) | `src/cli.rs` |
| pf.conf grammar | `sbin/pfctl/parse.y` (`ruleset`, `include`, `option`, `varset`, `varstring`, `tabledef`, `pfrule`, `natrule`, `binatrule`, `rdrrule`, `fromto`, `ipspec`, `xhost`, `host`, `portspec`, `action`, `dir`, `logquick`, `interface`, `af`, `proto`) | `src/parser.rs`, `src/lexer.rs` |
| rule printer | `sbin/pfctl/pfctl_parser.c` (`print_rule`, `print_fromto`, `print_port`, `print_op`, `print_flags`, `print_tabledef`) | `src/print.rs` |
| table file input | `sbin/pfctl/pfctl_table.c` (`load_addr`, `pfctl_table`) | `src/table.rs` |
| syntax reference | `sbin/pfctl/pf.conf.5` | this document |

Illumos and Linux were checked for outcome only; neither has a permissive
pf.conf front-end to port, so the FreeBSD permissive file is the donor.

## Divergences (documented, bounded)

* Hostnames are not resolved (no DNS/`getaddrinfo` in the front-end): address
  literals only (`a.b.c.d`, IPv6, `/prefix`). `no-route`/`urpf-failed` and
  address ranges (`a-b`) are rejected with a line-numbered error.
* Ports are numeric only; service names (`ssh`, `http`) are not looked up.
* Only one transport protocol per rule; `proto { tcp, udp }` lists are not
  supported.
* Address lists stay lists (`{ a, b }`); the donor expands them into separate
  rules at load time, which belongs with the ABI/loader ticket.
* `anchor` supports the plain named form; nested anchor bodies and
  `nat-anchor`/`rdr-anchor`/`binat-anchor` are not supported.
* `flags any` is accepted as the implicit form (the donor's parse.y sets
  flags/flagset to zero); the printer then emits `flags any` only for
  pass/match state-tracked rules, exactly like the donor's `print_rule`.
* `set timeout`/`set limit` braced lists normalize to one
  `set <kind> <name> <value>` line per pair.
* Includes inherit the parent macro table; macros defined inside an include are
  visible in that include only (the donor's symbol table is global).
* Mutating actions require `-n` until K273 provides the pf ABI.

## Tests and fixtures

`tests/fixtures/pfctl/`: `pf.conf` + `pf.conf.stdout` (the exact normalized
output), `includes/` (include expansion), `invalid/*.conf` + `.err` (expected
line number and message). `userland/bin/pfctl/tests/fixtures.rs` asserts the
oracle, the round trip, include expansion, every invalid fixture's line/error,
and that every prefix of a valid config parses without a panic.

```sh
timeout 180 cargo nextest run -p raven-pfctl
timeout 420 make check-fast
```

## Backlog

* Map the AST onto `crates/raven-pf` and the netd status path once K273 lands
  (this ticket must not edit `crates/raven-pf` or netd).
* Stage `raven-pfctl` in an image profile and add the `pfctl-config` qtest
  (profile fragments are outside this ticket's Owns scope).
* Hostname resolution, service-name ports, protocol lists, address ranges and
  nested anchors, if the release needs them.
