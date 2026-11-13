# raven-libc-elf

Pure logic for RavenBSD's dynamic loader: the ELF64 object view, the SysV and GNU hash tables,
symbol-version matching, scope walking, the per-architecture relocation decision tables,
`DT_RELR`, segment plans, init/fini order and the `LD_*` environment.

The crate is `no_std` + `alloc`, `#![forbid(unsafe_code)]`, and reads an image only through
[`MemView`], a bounds-checked byte view. The loader binary (`raven-libc-rtld`) supplies the
`unsafe` side of `RelocApplier`; none of the code here touches a pointer, a file or a syscall.
Design and behaviour: `architecture/raven-libc/donor-map/13-dynamic-loader.md`.

Tests run on the host:

```sh
cargo test -p raven-libc-elf
```

The two integration tests need a host C compiler and `readelf`; without them they print
`SKIP: <tool> not found`. Set `RL_ELF_REQUIRED=1` to turn a missing tool into a failure.
