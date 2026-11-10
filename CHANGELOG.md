# Changelog

All notable changes per version. The version number lives once in `Cargo.toml`
(`[workspace.package] version`) and applies to the library, the CLI, the GUI and the
Nix flake; the GUI shows it in the title bar and under **Help ▸ About**, the CLI with
`vna-cli --version`. A release is a git tag `vX.Y.Z` on `main` (see "Versions and
releases" in the README).

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow [SemVer](https://semver.org/): until 1.0 a minor version (0.x) may still change
things that are not backwards compatible, such as the format of the calibration files.

## [Unreleased]

### Changed
- The udev rule for Linux is now `linux/70-litevna.rules` and gives the port to the
  logged-in user (`uaccess`) and the group `dialout` (`0660`) instead of to everyone
  (`0666`). In the old 99- file the `uaccess` tag did nothing because it came after
  `73-seat-late.rules`. Remove `/etc/udev/rules.d/99-litevna.rules` if you installed it.
- This changelog is in English from here on, like the README and the release notes on
  GitHub that are made from it.
- The manufacturer's firmware images and PDFs in `assets/` are no longer tracked in git;
  an MIT repository should not redistribute them. `docs/02-assets-inventaris.md` says
  where they come from.
- GUI: the default file names when saving are the frequency range (`1MHz-30MHz.cal.json`,
  `sweep-1MHz-30MHz.s2p`) instead of `litevna.cal.json` and `sweep.s2p`; two hints in the
  Traces panel pointed at a button and a roadmap step that no longer exist.
- Calibration files carry a format number (`"format": 1`) and the stored GUI settings a
  version. Files and settings from 0.2.x load unchanged; a calibration file written by a
  newer vna-r with a higher format is refused with a message instead of being misread.
  The settings keep their marker and plot lists as lists, so a future change in the number
  of markers or plots no longer resets every setting.

### Development
- The workspace declares its minimum Rust version (1.95, the floor of egui 0.37) and is
  clean under clippy; CI now runs clippy with warnings as errors, tests the library with
  overflow checks, and checks that the Dutch and English manuals have the same structure.
  The release workflow builds the Linux tarball on Ubuntu 22.04 (glibc 2.35, so Debian 12
  works), keeps macOS `._*` files out of the archives and no longer appends auto-generated
  notes to the changelog text.
- Removed an unused register writer and the GUI's `anyhow` dependency; corrected comments
  that contradicted the measurements in docs/01 (FIFO checksum, points per second).

### Fixed
- GUI: a USB cable pulled during a sweep is recognised as a disconnect also when the
  failure shows up as a timeout: the worker checks whether the port still exists. Before,
  the GUI could stay "connected" to a device that was gone.
- A Touchstone file that cannot be read now reports the line ("line 12: not a number:
  …") as its own kind of error instead of "invalid argument".
- Library: a sweep configuration with zero points, a non-increasing grid in the TDR and
  a zero or negative band in `check_lines` caused an arithmetic overflow or an endless
  loop; they now return an error.
- GUI: a calibration standard is averaged over as many sweeps as **Sweep avg** says;
  before, every standard was one raw sweep under an averaged trace.
- GUI: the comment in a saved .s2p and the name of a held reference said "host
  calibrated" or "raw" about an opened file, going by the last device sweep; for a file
  they now say "from file …".
- GUI: velocity factor, maximum TDR distance and gate edges from a damaged settings file
  are clamped; 0 or NaN led to a division by zero and an error that came back every
  frame.
- A sweep from a Touchstone file could not serve as a calibration standard ("n points
  missing in sweep"): files and NanoVNA shell sweeps encoded "no raw records"
  differently. One convention now: no records is an empty list, and that does not count
  as missing points.
- GUI: **Set sweep for TDR** and a new segment always went up to 6.3 GHz, also on a
  NanoVNA-F (1.5 GHz); they now take the upper limit of the connected device.
- A nonsensical screenshot header (from a device that is not a LiteVNA, or a disturbed
  link) could reserve up to 17 GB before anything was read; the dimensions are now
  limited to 4096×4096 and only 16 bits per pixel is accepted, which the conversion
  assumed anyway.
- CLI: `vna-cli tdr --csv -` writes to stdout as the manual says (it created a file
  named `-`); the comment in an .s2p from `sweep` names the real model instead of always
  "LiteVNA" and says "host calibration per segment" for `--segment …@cal.json` instead
  of "uncalibrated raw"; the line "first raw record: …" only appears with `--trace`.
- GUI: the arrow keys also moved the active marker while you typed in Start, Stop or a
  reference name.
- GUI: markers stay at their frequency when the grid changes (another range, another
  number of points, an opened file); before, they stayed at the same point number and so
  pointed at another frequency. A marker outside the new range disappears.
- GUI: a failed screenshot or a failed **set** of the averaging stopped the running
  sweep; only a sweep or connection error does that now.
- GUI: the open/short/load buttons measured on the grid typed into Start/Stop/Points,
  even when it had not been applied with **Apply sweep**; the display then ran on the
  old grid and the calibration was silently interpolated. Measuring a standard now
  applies the typed sweep first, and the panel warns as long as the fields differ from
  the running sweep.
- GUI: the status line of the calibration panel ("measured open", "loaded", "kit updated
  … save to keep it") was overwritten right after every action by the re-solve of the
  calibration, so with **apply** on it was never visible. The reason a calibration
  cannot be applied is now shown separately ("not applied: …").
- Calibration interpolation to another grid is now polar (magnitude and phase
  separately) instead of linear in real and imaginary part. The latter cuts the corner
  between two phasors and shrank every standard by cos(θ/2) per phase step θ, so every
  corrected |Γ| came out too high: negligible at 1001 points, but +1.3 dB at 11 points
  with a long cable. The limit on the phase step goes from 90° to 135° and now also
  applies to the short, the through, line open and line short, not only to the open.
- The Δ trace against a reference on another grid uses the same polar interpolation.
- TDR: the reported unambiguous range ("alias-free range") was 1/Δf, twice what the
  chart shows; the arrays and the gate already ran to 1/(2·Δf). The GUI label, the CLI
  line and the axis limit now use the right value, and the gate error names it directly.
- The git revision in **About** and `vna-cli --version` was only refreshed on a branch
  switch, so a build after a new commit on the same branch kept showing the old one.
- A port without a recognisable USB id that turns out not to be a NanoVNA shell cost
  twice the command timeout when connecting (first waiting for the prompt, then sending
  a `resume` while cleaning up and waiting again); now once.

### Documentation
- GTK was never a dependency: the file dialogs go through the xdg desktop portal. The
  README, the manuals, CI and the flake mentioned `libgtk-3-dev`/`gtk3` anyway; those
  are gone and the real requirement (a running portal) is stated. The README still said
  version 0.2.0 and used archive names of 0.2.3; docs/04 described the DC term from
  before 0.2.5.
- The TDR screenshot in chapter 5 now shows a short instead of an open: with the dip to
  0 Ω the 50 Ω line stays readable, with the open it sat at the bottom of an axis up to
  500 Ω.
- Screenshot of a 25 Ω resistor behind 5 m of coax with the cable recipe in chapter 6:
  the impedance chart steps from 50 to 26 Ω.

## [0.2.5] - 2026-11-05

TDR impedance on long cables, and two new screenshots in the manuals.

### Fixed
- TDR: the impedance along the line was too low and tilted on cables of more than a few
  metres (25 Ω for a 50 Ω cable of 4 m). The DC point of the step response was
  extrapolated from the lowest measured points, which fails as soon as S11 turns a full
  circle within them; it now follows from the requirement that nothing reflects before
  the reference plane. When resampling, S11 is also interpolated in magnitude and phase,
  so the end peak of a long cable no longer comes out too low.

### Documentation
- Screenshot of an S21 measurement (FM notch filter FLT201A/N) with the filter recipe in
  both manuals, and a TDR of a 5 m cable with an open end in chapter 5.

## [0.2.4] - 2026-11-05

Downloads for more platforms.

### Added
- Releases for Intel Mac (`x86_64-macos`) and Windows (`x86_64-windows`, zip) next to
  Apple Silicon and Linux; CI now also builds and tests on Windows. The Windows build
  has not been tried with a device yet. On Windows the GUI no longer opens a console
  window.
- The README and the manual explain how to get past Gatekeeper on macOS (unpack in the
  Terminal or `xattr -dr com.apple.quarantine`) and past SmartScreen on Windows.
- Issue template on GitHub that asks for the output of **Copy diagnostics**.

## [0.2.3] - 2026-11-05

Six points from an external code review, all in the measurement chain and the GUI's
state handling.

### Fixed
- A LiteVNA record with a bad checksum is no longer used as a measurement point; the
  point stays empty and comes again on the next pass of the FIFO.
- Crash with a NanoVNA-F with several segments and calibration on (the shell delivers no
  raw records, and splitting per segment assumed it did).
- After **Clear** in the calibration panel the CAL label stayed while the data were raw.
- The software average (**Sweep avg**) is cleared on connect and disconnect, so sweeps
  of a previous device do not count.
- Calibration files are checked when loaded (grid increasing, at least two points, as
  many values per standard as grid points, finite numbers); a broken file gives a
  message instead of a crash. Frequencies such as `nan` or `inf` are refused.
- Touchstone files with a reference impedance other than 50 Ω (`R 75`) are converted for
  S11; before, the value was ignored.
- TDR refuses a sweep that is too narrow for its top frequency (FFT larger than 4 Mi
  points) with a message, instead of allocating gigabytes on the GUI thread.

## [0.2.2] - 2026-11-05

First feedback from a tester on Fedora.

### Added
- **About**, **Copy diagnostics** and the log show the rendering layer (Vulkan/Metal/
  OpenGL, adapter, driver, Wayland or X11); `RUST_LOG=info` shows wgpu's choice on the
  terminal. Manual: what to do about a stuttering GUI on Linux.

### Fixed
- Stuttering menus on a Fedora workstation with an older NVIDIA card (nouveau): wgpu
  picked the experimental NVK Vulkan driver there. The GUI now chooses the adapter itself
  and avoids NVK and software rendering as soon as there is an alternative;
  `WGPU_BACKEND` still forces a backend.

## [0.2.1] - 2026-11-05

For testers outside the Netherlands: English README and manual.

### Added
- English manual in `docs/manual`, next to the Dutch one in `docs/handleiding`. The
  release tarball now contains both, with the images.
- **Help** opens the English or the Dutch manual on GitHub at the tag of its own
  version, so the text matches the buttons of the running program.

### Changed
- README in English, with download instructions, quick start and device table.
- Manual: the threshold for a loose open or short is −8 dB (the text said −3 dB); the
  OCF paragraph no longer ties the sign of X to the length when you measure through the
  feed line.

## [0.2.0] - 2026-11-05

First version given to others to test.

### Added
- Menu bar in the GUI (File, Device, View, Help) with **About vna-r**: version, build
  revision, author and the link to GitHub.
- **View ▸ Developer tools** turns on a Developer menu with a log window (connections,
  errors, file operations, sweeps) and **Copy diagnostics**, which puts version, device,
  sweep, calibration and the log on the clipboard for a problem report. Off by default.
- One version number for the whole project; `vna-cli --version` and About also show the
  git revision. The release workflow refuses a tag that does not match the version in
  `Cargo.toml` and uses the matching section of this file as release notes.
- NanoVNA-F (text shell, USB 0483:5740) next to the LiteVNA 64/67: sweeps through
  `scan_bin`, screenshot, info and battery; CLI and GUI choose the protocol by USB id.
- Δ trace (live minus reference) in every view: dB, degrees, SWR, ohm, nanoseconds.
- Ports refresh automatically while there is no connection.
- Nix flake (`nix run github:pa1bh/vna-r`) with the udev rule, release workflow for
  x86_64 Linux and Apple Silicon, CI that also builds the flake.
- Dutch manual in `docs/handleiding` with screenshots.

### Changed
- Buttons use text instead of symbols that Linux drew as boxes.
- The message after saving a Touchstone file is under **Files**.
- The plausibility check of open/short takes the −3 dB raw reflection of a NanoVNA-F
  into account.

## [0.1.0] - 2026-11-03

Internal version: LiteVNA 64/67 through the binary register protocol, SOLT calibration
with kit fit and interpolation, Smith chart and seven views, markers, references,
segments, TDR with gating, Touchstone export and import, CLI with the same functions.

[Unreleased]: https://github.com/pa1bh/vna-r/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.5
[0.2.4]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.4
[0.2.3]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.3
[0.2.2]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.2
[0.2.1]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.1
[0.2.0]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.0
[0.1.0]: https://github.com/pa1bh/vna-r/commit/1a78b67
