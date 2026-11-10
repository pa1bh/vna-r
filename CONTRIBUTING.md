# Working on vna-r

A hobby project, so no ceremony; this file is what you need to find your way and to keep
the pieces consistent.

## Build, test, check

```bash
cargo build --workspace
```

```bash
cargo test --workspace
```

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && ./scripts/check-manuals.sh
```

The last line is what CI enforces besides the tests. Rust 1.95 or later (the floor of
egui 0.37). On Linux you also need `pkg-config` and `libudev-dev`; `nix develop` gives a
complete shell. No test needs a device.

## Layout

| Where | What |
|---|---|
| `crates/litevna` | The library. No UI, no `anyhow`; errors are `litevna::Error`. |
| `crates/vna-cli` | Command line tool; thin, argument parsing and printing. |
| `crates/vna-gui` | egui/eframe GUI. `app.rs` is the state and all panels, `worker.rs` the device thread, `smith.rs` the Smith chart. |
| `crates/build_git_rev.rs` | Build script shared by the two binaries (git revision for About and `--version`). |
| `docs/manual`, `docs/handleiding` | The user manual, English and Dutch, chapter for chapter the same. |
| `docs/01` … `docs/08` | Research notes, roadmap, analysis and QC procedure, in Dutch. |
| `linux/70-litevna.rules` | udev rule, shipped in the Linux tarball and by the flake. |

Inside the library, from the wire upwards:

- `protocol` (LiteVNA binary records, pure functions) and `shell` (NanoVNA text shell,
  pure parsers plus `NanoVnaShell`) know the two protocols; `device` holds `LiteVna`,
  `SweepConfig` and `SweepResult`.
- `vna` wraps both in the enum `Vna`; the CLI and GUI only talk to that. Protocol-specific
  extras stay on the concrete types (`Vna::require_lite`).
- `calibration` (SOLT with kit models, interpolation, kit fit), `timedomain` (TDR, gating),
  `traces` (dB, SWR, impedance, group delay), `touchstone` (files) and `freq` (parsing
  `14.2M`) are pure maths on `SweepResult` and know nothing about devices.

The GUI keeps all serial I/O on the worker thread. The UI sends `worker::Cmd` and gets
`worker::Event`; nothing in `app.rs` may block on the device.

## Conventions

- **A measurement tool must not guess.** Reject bad input with a message rather than
  clamping it silently; where a value is clamped for display, say so in a comment.
- **Maths goes in the library**, with a test, even when only the GUI uses it today. The
  GUI and the CLI should not each have their own copy of a rule.
- **Every user-visible change gets a line in `CHANGELOG.md`** under *Unreleased*, in
  English, written for a user. The release workflow turns the section into the release
  notes.
- **The two manuals change together.** `scripts/check-manuals.sh` checks headings, images
  and code blocks, not the wording. The Help menu links to the manual at the tag of the
  running version, so a manual fix reaches users with the next release.
- **File formats are a promise.** Calibration files carry `"format"`, the stored GUI
  settings a `version`; read old files, refuse newer ones with a message, and add a test
  with a literal old file when the format changes.
- **One commit per topic**, message in English, saying why.
- Comments explain what the code cannot: the measurement that led to a constant, the
  reason for an odd order of operations. `docs/01` and `docs/06` hold the measurements.

## Adding or changing device support

Start with `docs/01-onderzoek-protocol.md` (LiteVNA) or `docs/06-nanovna-shell-protocol.md`
(NanoVNA shell) and `cargo run -p litevna --example probe_shell` for a shell device.
Parsers are pure functions with tests on captured bytes; put a capture in the test, not a
description of it. `discovery` decides the protocol from the USB ids; `Vna::info` fills
`Limits`, which the GUI uses for its warnings and defaults.

## Releasing

See "Versions and releases" in the README: bump the version in `Cargo.toml`, move
*Unreleased* in the changelog under the new number, commit, tag `vX.Y.Z`, push both. The
QC procedure for the current work list is `docs/08-qc-procedure.md`.
