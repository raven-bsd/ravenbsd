# raven-libc-time

Pure-logic `<time.h>`/`<sys/time.h>` and POSIX-timer crate: NetBSD tzcode (`localtime`, `mktime`,
`timegm`, `strftime`, `strptime`), TZif v1-v4, the POSIX `TZ` parser, and the host-testable
`TimeSys` seam. See `architecture/raven-libc/donor-map/06-time-tz.md`.

Run `cargo test -p raven-libc-time`; the real-zone differential runs with
`RAVEN_ZONES_DIR=<zic-output> cargo test -p raven-libc-time --test zones_real`. The generated
`zones` directory must contain `slim`, `fat`, `right` and `zone1970.tab` (zic recipe in the ticket).
The test data pin is `tzdata-2026fgtz` (NetBSD `external/public-domain/tz/dist/TZDATA_VERSION`).
