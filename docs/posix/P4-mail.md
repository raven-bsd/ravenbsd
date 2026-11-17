<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P47 local mail and mailx

Status: P47 implementation. `mail` and `mailx` are first-party Rust host utilities over one
bounded local core. They append to an mbox file or a Maildir, list headers, read one message,
delete one message and answer the `-e` existence check. There is no SMTP, network MTA, daemon or
process server.

## Evidence

```sh
sh tests/posix/utility/run-mail.sh
cargo nextest run -p mail -p mailx
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

OBSERVED (host, x86_64): `run-mail.sh` prints `2 pass, 0 fail, 0 defect`; the mbox case sends
from stdin and from a file, lists, reads, deletes both messages and checks `-e`; the Maildir case
does the same through `new/` files and rejects forward/reply options with exit 2.

## Local mail matrix

| Operation | `mail` | `mailx` | Notes |
|---|---|---|---|
| send with `-s SUBJECT` | present | present | local recipient names recorded in `To:` |
| body from stdin | present | present | default |
| body from file with `-b FILE` | present | present | `-` means stdin |
| select store with `-f PATH` | present | present | directory is Maildir, file is mbox |
| default store | present | present | `MAIL`, then `$HOME/Maildir`, `$HOME/.maildir`, `$HOME/mbox` |
| list headers | present | present | default with no action, or `-H` |
| read message `-r N` | present | present | one-based index |
| delete message `-d N` | present | present | mbox rewrite is atomic; Maildir removes the file |
| existence `-e` | present | present | exit 0 with mail, 1 without |
| reply `-R`, forward `-F` | unsupported | unsupported | usage error, exit 2 |
| SMTP/network, cc/bcc, attachments | unsupported | unsupported | no network or MTA is claimed |
| interactive prompt | unsupported | unsupported | the bounded core is command-line only |

Messages use `From:`, `To:`, `Subject:` and `Date:` headers. mbox stores prepend a `From `
separator and quote body lines beginning with `From `; Maildir stores write one file under `new/`
(the filename is unique per process/time). The date is formatted as RFC 2822 UTC from the host
clock.

## Exit status

* `0`: success, or `-e` with at least one message.
* `1`: operational failure, missing message, or `-e` with an empty store.
* `2`: usage error, missing argument, invalid message number or unsupported option.

## Tests

`tests/posix/utility/mail/mail/test.sh` covers mbox stdin/file delivery, header listing, reading,
two deletions, `-e` and the `-R` rejection. `tests/posix/utility/mail/mailx/test.sh` covers
Maildir file delivery, the `new/` file, listing, reading, deletion, `-e` and the `-F` rejection.
`run-mail.sh` runs both after one host build.

## UNVERIFIED

No Pi 4/Pi 5 board run. The tools are host utilities in the same evidence model as the other P4
utility rows; no target image profile installs them.
