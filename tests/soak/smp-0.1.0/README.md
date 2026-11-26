# S85h 0.1.0 SMP soak

Repeats one requested QEMU cell until `login:` appears, then stops QEMU and
starts the next cycle. Each cycle keeps the raw and timestamped serial, and the
runner fails on a missing CPU, a missing login marker, a panic/exception/TLB
shootdown timeout token, a supervisor timeout, or a non-zero exit.

```sh
sh tests/soak/smp-0.1.0/run.sh --arch x86_64 --accel kvm --smp 4 --cycles 100
sh tests/soak/smp-0.1.0/run.sh --arch x86_64 --accel tcg --smp 4 --cycles 100
sh tests/soak/smp-0.1.0/run.sh --arch aarch64 --accel tcg --smp 4 --cycles 100
```

The default images are `build/x86_64/ravenbsd-x86_64.img` and
`build/aarch64/login-cold-start-release.img`. Override them with `--image` or
`RAVEN_SOAK_IMAGE`. Firmware paths follow the existing `RAVEN_OVMF_*` and
`RAVEN_AAVMF_*` variables. Evidence is written below
`<prjmgmt>/tickets/evidence/artifacts/S85h/runs/`; set `RAVEN_PRJMGMT` when the
management checkout is not a sibling of this clone. `--out` selects another run
directory for scratch measurements.

Unit tests: `python3 tests/soak/smp-0.1.0/run_ut.py`.
