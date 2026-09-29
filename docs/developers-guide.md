# Developers' guide

This guide records conventions for people working on Cupcake's Rust workspace.
User-facing documentation lives in the site under `docs/docs`.

## The build standard

Development, test, lint and typecheck builds link with the `mold` linker on
Linux. This is a default in `.cargo/config.toml`, which Cargo discovers on its
own, so a bare `cargo build` gets it. `mold` ships for Linux only, so the flag
lives in a Linux-only target table, and macOS and Windows keep their platform
linker. Cargo selects one `rustflags` source rather than merging them, so a
`[build] rustflags` entry would have to repeat every flag the Linux table
names; the file has none.

The repository pins the stable toolchain `1.91.0` in `rust-toolchain.toml`, so
the nightly-only parallel frontend flag (`-Zthreads=8`) is not used.

Release builds are excluded on purpose. The `build` job in
`.github/workflows/release.yml` sets `RUSTFLAGS` to an empty value, which
displaces the mold table: a shipped artefact stays on the platform linker, and
the `cross` containers do not carry `mold`.

### Cranelift

Exception: Cranelift is not the development-profile backend. Cranelift
requires a nightly toolchain, and this repository pins the stable `1.91.0`
(recorded 2026-09-29, with no measurement taken on it). Revisit if the
repository moves to a nightly pin; leynos/cupcake#1 asks for that decision.

### Installing mold in CI

Exception: the estate installs `mold` through the `install-mold` input of
shared-actions' `setup-rust`, but these workflows use `dtolnay/rust-toolchain`
instead. Every Linux job that compiles Rust therefore runs an "Install mold"
step that installs the Ubuntu 24.04 package at the pinned version
`mold=2.30.0+dfsg-1build1`. Move the workflows onto `setup-rust` with
`install-mold: 'true'`, and drop these steps, when leynos/cupcake#1 settles the
toolchain.
