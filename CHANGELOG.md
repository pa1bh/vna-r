# Changelog

All notable changes per version. The version number lives once in `Cargo.toml`
(`[workspace.package] version`) and applies to the library, the CLI, the GUI, the server
and the Nix flake; the GUI shows it in the title bar and under **Help ▸ About**, the CLI with
`vna-cli --version`. A release is a git tag `vX.Y.Z` on `main` (see "Versions and
releases" in the README).

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow [SemVer](https://semver.org/): until 1.0 a minor version (0.x) may still change
things that are not backwards compatible, such as the format of the calibration files.

## [Unreleased]

### Fixed

- **A NanoVNA-F that sat idle after a sweep lost its reflection.** Measured on a
  NanoVNA-F 4.3 with firmware 1.0.4: left alone for some minutes after a single sweep
  (150 s went well, 300 s did not), every sweep after it showed S11 near −66 dB, an SWR
  of 1.00 on any antenna, on the device's own screen as well, until the device was
  switched off and on. A scan leaves the device waiting; sweeping on its own it stays
  right for hours. So 30 s after the last sweep vna-r now lets the device go on by
  itself, as **Stop** already did, and the next sweep takes it back. Nothing changes for
  a LiteVNA. Tried through `vna-server` on a Raspberry Pi 4 with pauses of 5, 5 and
  8 minutes: wrong every time before, right every time after. **Not tried**: the program
  with the device on its own USB port, which runs the same code.

## [0.4.1] - 2026-11-17

A clean-up round: the whole code base read through for bugs, for places that break on
bad input and for code and documentation that had outlived their use. No new features.
The fixes that matter most are in `vna-server`: a client could stop its device thread
with one command, a server whose device thread had stopped kept listening, and a server
that stopped did not say goodbye to its clients.

Tried: `vna-cli` with a LiteVNA 67 on macOS and with a NanoVNA-F on a Raspberry Pi 4;
`vna-server` on both with a client that speaks the protocol by hand (among it the
commands the device thread could not take, two viewers, stopping the server under a
running sweep); the page in a browser through both servers, with **Band** and with
Disconnect during a standard measurement. Built on Asahi Linux (Apple M1) with a
LiteVNA 64: S11, S21, TDR, and the cable pulled and put back. The program connected to
the server of 0.4.0 on the Pi. **Not tried**: **sweep from calibration** on the screen
with a calibration whose end is not a round number of kHz (the fix is covered by a test
of the text that goes into the fields, and **Band** was seen in the browser), saving a
calibration and the references from the program's window, the archives of this release,
whose workflow tests what it builds for the first time, and Windows with a device.
docs/08 has the log.

### Fixed

- `vna-server`: a sweep command a client sends is checked before it reaches the device
  thread. A command with an absurd number of passes stopped that thread, one without
  segments kept it spinning. And when that thread does stop, the server now says so and
  exits with an error (systemd then starts a new one) instead of listening on and
  ignoring every command.
- `vna-server` that stops (Ctrl-C, `systemctl stop`) says goodbye to its clients: they
  got neither the last message nor a close and saw the connection break. Also so in
  0.4.0; found with a client that kept sweeping while the server stopped.
- GUI: **sweep from calibration**, **Band**, the segment spread and **Set sweep for TDR**
  wrote frequencies rounded to the kHz into Start and Stop. A calibration of 100 kHz to
  30 MHz in 1601 points ends at 29.9992 MHz; "29.999MHz" is another grid, so the
  calibration was interpolated and the next standard started a new set. The fields now
  get as many decimals as the frequency needs.
- GUI: after **Disconnect** from a server during a standard measurement the buttons of
  the standards stayed disabled.
- Touchstone, CSV and reference files: a write that failed at the very end (a full disk)
  was reported as written.
- The warning about a jump in the calibration had three gaps of fourteen spaces in it.
- Sweep averaging with outliers dropped: a point that is not a number (a zero reference
  wave) was the one outlier it could not drop.
- A Touchstone file with a magnitude too large to be one (7000 dB) or a reflection that
  cannot be converted to 50 Ω is refused instead of read as infinity.
- `vna-cli sweep --calibrated` wrote "uncalibrated raw" in the comment of its files.
- `vna-cli cal clear` failed on a file that no longer reads, the case it is for.
- `vna-cli --json tdr --csv -` wrote JSON and CSV to stdout together; it is refused, as
  `sweep` already did. `cal … --json` printed a line of text before the JSON.
- `vna-cli cal`: kit options with another action than `kit` were ignored without a word,
  and a mistyped `--channel` left the average of the device changed.
- In the browser, a file with a step tiny against its top frequency could divide by zero
  in the time-domain view; `sim:` with a rate like `1e-30` stopped the device thread; a
  request to the server that starts with blank lines is answered 400 instead of 431.

### Changed

- Calibration files (`vna-cli cal`, **Save** in the GUI) and the GUI's reference files
  are written to a temporary file that then takes the place of the old one. A write
  that fails half-way leaves the file that was there, with the standards in it.
- `vna-cli`: `--gate`, `--vf` and `--port-extension` are checked before the sweep, not
  after it.
- `vna-server` stops listening before it lets go of the device, so that a client that
  connects during the shutdown is not left waiting; a connection that ends in a crash
  gives its place back.
- The drivers check a sweep configuration themselves (at least one point, a last
  frequency that fits), a calibration set refuses a sweep whose values do not match its
  grid in number, and the line check refuses a ripple band too small for the sweep.
- GUI: when the serial ports cannot be listed, the developer log says why.
- CI and releases: the oldest supported Rust (1.95) is checked, every build uses the
  committed `Cargo.lock` (`--locked`), the documentation must build without warnings and
  the browser page is built the way a release builds it. A release tests what it builds,
  and stops when its changelog section or one of its archives is missing instead of
  publishing without it. Dependabot proposes newer actions and crates once a month.
- Manual chapter 10 and the README, after the first day with analysers behind a server:
  the USB ports of a Raspberry Pi 4 can be too weak for a LiteVNA that charges (it then
  drops out every few seconds; a hub with its own power supply is the likely cure), what
  has actually been tried (a Raspberry Pi 4 on Debian 13 and a Mac), that analysers can be
  swapped without restarting the server, and that a vna-server built from source has the
  browser page built in only when `scripts/build-web.sh` ran first.

## [0.4.0] - 2026-11-16

Measuring over the network. A new program, `vna-server`, shares the analyser on its USB
port; you measure from a browser, with the page the server serves, or from the program
with **Device ▸ Add server…**. For someone who never uses a server the program gains that
one menu item and nothing else.

Tried: with the LiteVNA 67 on macOS, the program as it was (the device thread moved to a
crate of its own): sweeps, and the cable pulled during a sweep, which is reported and
picked up again. With the NanoVNA-F on macOS: S21, also above 280 MHz. `vna-server` on
macOS with the LiteVNA 67, and on a Raspberry Pi 4 (Raspberry Pi OS on Debian 13, built on
the Pi) as the systemd service with a NanoVNA-F; the program and a Chromium browser as
its clients, two viewers at once, a server that restarts under a running sweep. **Not
tried**: the archives of this release, whose workflow builds `vna-server` and the browser
page for the first time; the page that measures in Safari, in Firefox and on a phone; a
LiteVNA on a Raspberry Pi (a LiteVNA 64 kept dropping off the USB of a Pi 4, which reported
over-current; a keyboard and the NanoVNA-F on the same port did not, so it is likely what
the LiteVNA draws while it charges, and a powered hub its cure); pulling the analyser's
cable out of a server; Linux and Windows as server or client; the Nix package.

### Added

- Measuring over the network. `vna-server` runs on a computer with the analyser on its USB
  port, a Raspberry Pi for example, and shares it on the network. It connects to the
  analyser by itself, waits for it when it is unplugged, and lets go of it when it is
  stopped, so that a LiteVNA gets its screen back. Every release archive has it; the Linux
  archives also have a systemd unit (`vna-server.service`) to start it at boot. Manual
  chapter 10.
- Measuring in the browser. `vna-server` serves the vna-r page itself: open
  `http://<server>:8080/` in any browser and the page connects to its server and measures,
  calibration included. Files go through the browser's own dialogs.
- **Device ▸ Add server…** in the program: a server is then listed in the port list under the
  serial ports and connected to with **Connect**, also by **Connect at start**. The servers
  are remembered. The browser page on its own (a website) has the same menu entry.
- Several people on one analyser: everyone sees the sweeps and can steer, each calibrates
  in their own program or browser, a calibration standard and a screenshot go only to the
  one who asked, and sweeping stops when the last viewer leaves.
- When the network or the server drops, the program waits up to two minutes and carries on
  with Run once it is back; the top bar says whether it waits for the server, for an
  analyser on the server, or for one that dropped off the server's USB.
- A simulated analyser, for trying all this without one: `vna-server --simulate`, and in the
  program **Developer ▸ Connect to simulator** (with **View ▸ Developer tools** on).
- Every release also has the browser page on its own, `…-web.zip`, for putting it on a website.
- `docs/10-client-server.md` (Dutch): the design note the server was built from.

### Changed

- A calibration file now reads back exactly as it was written; before, a value could come
  back one unit in its last decimal place off.
- The log lines of the device thread (connecting, a lost link, reconnecting) now come from
  `vna_link` instead of `vna_gui`; the program still shows them by default. For more detail
  on the terminal use `RUST_LOG=vna_link=debug`.
- The side panel is drawn again as soon as its width changes, so that it is not left cut off
  at the right until the mouse moves (seen once on a browser page from the server).
- Amateur bands, Region 2: 60 m ends at 5406.4 kHz, the edge 47 CFR 97.303(h)(3) names for
  the band, not the 5406.3 kHz that 0.3.6 read off the ARRL chart. Steve Falco, AC2XM, dug
  up the statute.
- README and both manuals brought up to date with 0.3.0–0.3.6 after a check against the
  code: the README's feature list (bands, Band row, zoom, shared axis with reset view, sweep
  from calibration, log spacing, drop, S21 offset, Kaiser windows, reconnecting, the
  browser build, arm64 in CI); the manual gains the browser viewer (chapter 2), the clear
  buttons, the TDR marker keys, the full list of remembered settings, the line open/short
  warning, the shared TDR axis, the default file names and the calibration format number,
  the `jumps` in `cal show --json` and the standard aliases. Corrected: the Windows folder
  of the references is `%APPDATA%\vna-r\data`, the Touchstone comment does not mention
  averaging, the menu entry is **Help ▸ About vna-r**.

## [0.3.6] - 2026-11-15

Three Region 2 band edges set right after a check against the ARRL chart, and the arm64
Linux archive confirmed on real hardware.

Tried: the corrected table in the library tests only; the `aarch64-linux` archive of 0.3.5
on Fedora Asahi Remix 44 (Apple M1) with a LiteVNA 64 (connect, calibrate, save, S11, S21,
TDR, screenshot; the udev rule is needed). **Not tried**: the new edges on the screen,
Windows, the NanoVNA-F.

### Changed

- Amateur bands, Region 2: three edges corrected against the ARRL chart of January 2026:
  60 m ends at 5406.3 kHz, 9 cm is 3300–3450 MHz, and 1.25 m has its 219–220 MHz piece
  (fixed digital message forwarding only) next to 222–225 MHz. Reported by Steve Falco,
  AC2XM, on groups.io.
- README: the `aarch64-linux` archive of 0.3.5 has since been tried, on Fedora Asahi Remix 44
  (Apple M1) with a LiteVNA 64: connecting, calibrating, saving the calibration and measuring
  all worked. The 0.3.5 notes still say it was untried.

## [0.3.5] - 2026-11-14

A band picker for the sweep, asked for on groups.io, and a first archive for arm64
Linux. The CI's Nix job now caches the dependencies (12 minutes down to under 2).

Tried with a LiteVNA 67 on macOS, in the GUI: the Band row in both regions, the Region 2
backdrop, and the region surviving a restart. vna-r itself ran on Asahi Linux (M1) with a
LiteVNA 64, built from source. **Not tried**: the `aarch64-linux` archive itself, which
this release builds for the first time; Windows; the NanoVNA-F.

### Added

- Sweep panel: **Band** sets Start and Stop to an amateur band with 10 % of its width
  extra on each side and applies the sweep, for tuning a multiband antenna band by band.
  Under **View ▸ Amateur bands** you now choose IARU Region 1 or Region 2 (the Americas:
  80 m to 4.0 MHz, 40 m to 7.3 MHz, 2 m to 148 MHz, 70 cm 420–450 MHz, and the 1.25 m,
  33 cm, 5 cm bands); the backdrop and the Band list follow it. Suggested by Steve Falco
  on groups.io. (Work list 71.)
- Releases: an `aarch64-linux` archive, built on GitHub's arm64 runner, for Asahi Linux
  on Apple Silicon, Raspberry Pi and other arm64 boards; CI builds and tests on arm64 too.
  First tried on Asahi Linux (M1) with a LiteVNA 64, built from source.

## [0.3.4] - 2026-11-13

Eight small additions after a look at what NanoVNA-Saver does, two wishes from the first
round of use with them, and a browser build of the GUI for looking at Touchstone files
without a device.

Tried with a LiteVNA 67 on macOS, in the GUI, with and without the device: the marker
column, the bands, the trimmed average with a load on port 1, log spacing over
50 kHz–6.3 GHz, the cable list, the S21 offset, the Kaiser windows, the shared axes with
reset view, and sweep from calibration; the browser build in Chrome on macOS. **Not
tried**: Linux and Windows, the NanoVNA-F, and other browsers.

### Added

- Plots: the three frequency charts share their frequency axis and the two TDR charts
  their distance axis, so zooming or dragging one moves the others and you always look at
  the same part of the spectrum. A **reset view** button in the marker bar (and the TDR
  header) puts every chart back on its automatic scale; a double-click in a chart still
  does the same. (Work list 69.)
- Calibration panel: **sweep from calibration** sets Start, Stop and Points to the grid of
  the loaded or measured calibration and applies the sweep, so a loaded file is used
  without interpolation. (Work list 70.)
- Marker table: a column **L / C** with the reactance at the marker as the one coil or
  capacitor it is at that frequency (1 µH, 47 pF). Hovering over it gives Q (|X|/R) and
  the parallel form of the impedance, R ∥ L or C. For an unknown component on port 1 this
  saves the arithmetic from R + jX. (Work list 53, after NanoVNA-Saver.)
- **View ▸ Amateur bands** draws the IARU Region 1 amateur bands as a grey backdrop with
  their name in the three frequency charts, so that it is plain whether an antenna's dip
  lies in the band. Off by default; remembered. The table is fixed to Region 1 for now.
  (Work list 54.)
- **Sweep avg** can leave outliers out: from three sweeps on, a **drop** field next to it
  sets how many sweeps per point, farthest from the mean first, are left out of the
  average. One glitch (a USB hiccup, a hand near the antenna) then does not pull the
  trace. Default 0, the plain mean as before; remembered. (Work list 55.)
- Segments: a **log spacing** button moves the segment edges so that every segment spans
  the same ratio, from the lowest start to the highest stop. With the same points per
  segment the grid is then dense on HF and coarse on UHF: 50 kHz to 6 GHz in one picture
  without a hundred points wasted above 3 GHz. (Work list 56.)
- TDR: a **cable** list next to the velocity factor with the catalogue values of common
  cables (RG-58, RG-213, PTFE types, RG-8X, H155, Aircell, H2000, LMR, Aircom Plus,
  Ecoflex, window line). A starting point only: within one type the spread is a few
  percent, so a known length still decides. (Work list 57.)
- Traces: an **offset** next to S21, in dB, for an attenuator between the device under
  test and port 2 (to protect the port from an amplifier). The value is added back, so
  S21 shows the device alone in the charts, the markers and saved files, whose comment
  line names the offset. 0 is off; remembered. (Work list 58.)
- TDR and gate: two Kaiser windows next to rectangular, hann and blackman. **kaiser 6**
  has side lobes at about −44 dB for nearly the width of hann; **kaiser 13** puts them
  below −95 dB at 2.3 times the rectangular width, for a very weak reflection next to a
  short or open. Also `vna-cli tdr --window kaiser6|kaiser13`. (Work list 59.)
- A browser build of the GUI: a viewer for Touchstone files, without a device
  (WebAssembly, drawn with WebGPU). It opens with a built-in example measurement, an
  off-centre-fed wire antenna for HF, and leaves out what needs an analyser: the port,
  Connect, Run, the sweep settings and the calibration. Markers, references, the Smith
  chart, TDR and gating work on an opened `.s1p` or `.s2p`; the file stays on the
  visitor's computer. `scripts/build-web.sh` writes three static files into `target/web`
  that can be put on a web server, and CI builds the browser target so that it keeps
  compiling. `docs/09-wasm-proef.md` (Dutch) says how to host it and what device support
  in a browser would take. Tried in Chrome on macOS only; **not tried**: other browsers,
  a phone, a web server with https and compression.

  For the program itself nothing changes. The device part of its top bar and of its side
  panel moved into functions of their own, so that the browser build can leave them out;
  that has been built and tested but **not looked at again with a device attached**.

## [0.3.3] - 2026-11-12

One correction to the reconnecting that 0.3.2 introduced.

Tried with a LiteVNA 67 on macOS, in the GUI: the cable pulled and put back, after which
the connection came back; and the warning for a jump in the calibration, new in 0.3.2,
shows as intended. **Not tried**: a port that comes and goes in quick succession with the
new rule (with a good cable that no longer happened; the rule is covered by tests only),
and the reconnecting on Linux and Windows.

### Fixed

- GUI: reconnecting no longer gives up after four drops in a row ("the connection keeps
  dropping; not reconnecting again", new in 0.3.2). A contact that is being moved does
  not drop once: a LiteVNA 67 left the USB bus fourteen times in half a minute while a
  connector was being turned on its port, each time back after 1.4 s and gone again
  within a second. The program now waits until the port has stayed for a second before it
  opens it again, longer after every drop in a row (2, 4, at most 8 seconds), and keeps
  waiting for as long as the port keeps coming back. It still stops after 30 seconds
  without the device. The cause of the drops turned out to be the USB cable; with another
  one they are gone.

## [0.3.2] - 2026-11-12

Two things that came out of an evening of measuring with a LiteVNA 67 on macOS: a
sawtooth at 100 MHz and an occasional `I/O error: Broken pipe`.

Tried: the jump detection on six calibration files (two of them have the 100 MHz jump),
and the reconnecting with the device, by pulling the cable while the GUI's worker was
sweeping: twice lost, twice reconnected within half a second of the port coming back,
sweeping resumed. **Not seen**: the warning and the orange "connection dropped" bar in
the GUI window itself; the reconnecting on Linux and Windows. What was and was not tried
for 0.3.0 still holds.

### Added

- A warning when a calibration is interpolated across a jump: a sweep with points between
  two calibration points where a raw standard measurement steps at least four times more
  than between the points around them. A LiteVNA 67 switches its signal source near
  100 MHz and everything it measures raw steps there; a 1 MHz–1 GHz calibration in 201
  points used on a 50–150 MHz sweep then draws a sawtooth from 96 to 100 MHz. The GUI
  shows the warning under Calibration, the CLI prints it with `sweep --cal`, and
  `vna-cli cal show` lists the jumps in a file (also in `--json`, as `jumps`). The cure
  is to calibrate on the grid you measure on; the interpolation itself is unchanged.

### Changed

- GUI: a connection that drops is no longer the end of the session. When the device
  leaves the USB bus (a cable or connector that moves, a device that restarts) the top
  bar says so, the program waits up to 30 seconds for the port to come back, reconnects
  and carries on with Run or the single sweep that was under way. Before, the first sign
  was `I/O error: Broken pipe` at the next sweep and you had to connect again yourself,
  sometimes after restarting the program. The loss is now also noticed while nothing is
  being measured (the port is looked at twice a second, and a battery reading that fails
  counts). A calibration standard that was being measured is not repeated. Seen with a
  LiteVNA 67 on macOS, where the system log showed the device leaving the bus for about
  two seconds, three times in twenty seconds. The cause there was the USB-C plug: taking
  hold of it was enough for such a drop.

### Documentation

- README and manual chapter 2: what to do when cargo refuses to build because it picks up
  an older Rust that rustup installed earlier (`rustup update stable`). Reported by a user
  on Fedora.

## [0.3.1] - 2026-11-12

A small release after the first reaction from a user outside the circle of testers.
Nothing else changed since 0.3.0, so what was and was not tried there still holds; the
Zoom menu was tried on macOS only.

### Added

- GUI: **View ▸ Zoom** sets the size of the text and everything else in the window, from
  75% to 250%, and shows the size in use. Zooming itself was already there, on Ctrl (⌘ on
  a Mac) with +, − and 0, and the size was already remembered, but nothing in the program
  or the manual said so. Asked for by a user on Windows 11 who found the text too small.

## [0.3.0] - 2026-11-12

The result of a review of the whole project: bugs in the calibration and TDR, the
calibration flow in the GUI, a stricter CLI, new functions and a round of restructuring.
It went out as the release candidates 0.3.0-rc.1 and 0.3.0-rc.2 on 10 November; what they
and the testing after them changed is merged into this section.

Tried with a LiteVNA 67 on macOS (the whole CLI; in the GUI connecting, sweeping,
calibrating, interpolating a coarse calibration, the progress bar and losing the cable)
and with a NanoVNA-F 4.3 on NixOS and macOS (CLI sweeps, and in the GUI a through
calibration and measurements of a filter, attenuators and antennas). **Not tried with a
device**: Windows. **Not confirmed**: the file dialogs on NixOS after the fix below. With
the LiteVNA the references, TDR, gate and file functions of the GUI were not gone through
step by step. One observation is unexplained: with the NanoVNA-F on NixOS the S11 trace in
the GUI was scattered once while raw sweeps with the CLI were right, and it went away
without a known cause.

Settings and calibration files of 0.2.x are read; 0.2.x does not read the settings this
version writes, so keep a copy of `app.ron` if you want to go back.

### Added
- CLI: `vna-cli cal --help` lists the standards and actions with what each does, and a
  mistyped one is refused with the list; `vna-cli completions <shell>` prints a completion
  script. Options that an action would ignore (`--note` with `cal show`, `--apply` without
  `fit`, a grid with anything but a standard) are an error. Piping into `head` no longer
  ends with a panic or "Broken pipe", and with `--csv -` stdout carries only the CSV (the
  table used to be printed in front of it).
- GUI: references are kept between sessions (as `ref-N.s2p` files in a `references`
  folder next to the settings) and come back at the next start with their names,
  visibility and the Δ choice.
- Port extension: move the reference plane outward by a one-way delay in picoseconds
  (**Port extension** in the calibration panel, `--port-extension <ps>` with `sweep` and
  `tdr`), for an adapter or fixture behind the calibration plane. It applies to
  everything shown and is noted in exported files; the calibration is not touched.
- GUI: keyboard shortcuts (space Run/Stop, `S` single sweep, `1`–`4` active marker, `H`
  hold, `G` gate, `T` frequency/time domain; arrow keys now also move the TDR marker),
  the battery voltage is read again every minute, and **View ▸ Connect at start**
  connects by itself when a VNA is plugged in (off by default).
- One-port Touchstone export: `vna-cli sweep --s1p file.s1p`, and in the GUI a file name
  ending in `.s1p` when saving. The sweep CSV has four more columns behind the existing
  ten: `r_ohm`, `x_ohm`, `z_mag_ohm` and `s21_group_delay_ns`.
- CLI: `--json` makes `list`, `info`, `sweep`, `tdr` and `cal show` print one JSON document
  on stdout instead of a table, for scripts.
- GUI: **Stop**, **Disconnect** and **Apply sweep** no longer wait for a running sweep to
  finish; they take effect at the next block of data (255 points on a LiteVNA, 301 points
  or about three seconds on a NanoVNA-F, where a long sweep used to make Stop look dead
  for up to half a minute). A sweep that takes more than a moment shows a progress bar.
  **Stop** is also there while a calibration standard or a single sweep is being measured
  with Run off, and the space bar follows.
- Library: `sweep_with` on `Vna`, `LiteVna` and `NanoVnaShell` reports progress per block
  and lets the caller cancel (`Error::Cancelled`).

### Changed
- GUI: Return in **Start** or **Stop** applies the sweep.
- CLI: `sweep --segment` together with `--start/--stop/--points/--cal`, and `tdr --from`
  together with a grid or `--cal`, are now an error instead of silently ignoring the
  latter.
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
- The GUI's 2900-line `app.rs` is split by subject into `app/` (settings, sweep,
  calibration, plots, tdr, files, diagnostics); code moved unchanged.
- GUI: one copy of the shown sweep per frame instead of three, without the raw records,
  and the calibration status line no longer solves the calibration every frame.
- Rules that the CLI and the GUI each had a copy of now live in the library with tests:
  ordering and overlap check of segments (`plan_segments`, `SweepConfig::parse_segment`),
  the segment TDR works on (`SweepResult::widest_segment`), distance to round-trip time
  (`Gate::from_distance`), nearest frequency point, the complex average of sweeps.
- The drivers can be tested without hardware: `LiteVna::from_port` and
  `NanoVnaShell::from_port` take any serial port, and the tests drive them with a fake
  LiteVNA and a fake NanoVNA-F (sweep loop, checksum and duplicate accounting, chunked
  scans, echo and prompt framing, screenshots, a pulled cable, a silent device). The
  library went from 39 to 75 tests.
- The workspace declares its minimum Rust version (1.95, the floor of egui 0.37) and is
  clean under clippy; CI now runs clippy with warnings as errors, tests the library with
  overflow checks, and checks that the Dutch and English manuals have the same structure.
  The release workflow builds the Linux tarball on Ubuntu 22.04 (glibc 2.35, so Debian 12
  works), keeps macOS `._*` files out of the archives and no longer appends auto-generated
  notes to the changelog text.
- One definition each for the reference impedance (`litevna::Z0`), the dB floor
  (−200 dB) and the SWR ceiling; `db` and `swr` moved to `litevna::traces` (still
  re-exported from `touchstone`).
- Removed an unused register writer and the GUI's `anyhow` dependency; corrected comments
  that contradicted the measurements in docs/01 (FIFO checksum, points per second).

### Fixed
- NixOS (flake): **Save…**, **Load…** and the other file dialogs of the GUI did nothing;
  the log said "Can't connect to a portal: libdbus-1.so not found". The dialogs load
  `libdbus` at run time, and the flake did not put it on the library path. It has been
  that way since 0.2.0. The release archives for other distributions are not affected.
- NanoVNA-F: S21 could come back at about -65 dB with a cable between the ports, so a
  through calibration gave the warning "median raw |S21| is -65.8 dB" and every S21
  measurement was wrong, while S11 was right. It is a property of the firmware (1.0.4):
  after the device has swept on its own for some seconds, a scan that reaches above
  280 MHz loses the through channel, and so does every such scan after it, until one scan
  stays below 280 MHz. vna-r now makes that short scan (two points, 10 and 20 MHz) before
  the first sweep of a session. Found on Linux, but it does not depend on the platform:
  it showed whenever the first sweep after connecting went above 280 MHz.
- NanoVNA-F (text shell): a sweep could fail with `protocol error: unexpected sweep reply
  "\nch> sweep…"`, seen on Linux right after opening the port. A prompt that the device
  had sent earlier arrived after the input buffer was cleared and ended up in front of
  the echo of the next command. Opening the port now waits until the line is quiet, and
  every reply is read from the echo of its own command onward, for text commands, scans
  and screenshots alike.
- CLI: the comment line of a file written by `sweep --s2p` did not say that S11 was
  time-gated, while the GUI's did. It now states the gate and the port extension, and the
  comment of an `.s1p` says whether the data was calibrated.
- GUI: a sweep that TDR cannot work with (a file with unevenly spaced points, a span too
  narrow for its top frequency) put an error in the top bar that came back every frame
  and could not be dismissed, while the TDR view said "Connect and press Run". The TDR
  view now says why there is no TDR, and the top bar is left alone.
- GUI: a long error in the top bar is cut off with the full text in its hover instead of
  running off the window; "diagnostics copied" appears there for a few seconds instead of
  under **Files**.
- LiteVNA: after a sweep that failed (a timeout, a record with an impossible index) the
  next sweep on the same grid did not reset the protocol and could stay out of step with
  the device's replies; a failed sweep now forces a reset and a fresh configuration.
- GUI: a USB cable pulled during a sweep is recognised as a disconnect also when the
  failure shows up as a timeout: the worker checks whether the port still exists. Before,
  the GUI could stay "connected" to a device that was gone.
- A measurement that is not a finite number (NaN or infinity, from a damaged file or a
  record without a reference wave) is refused as a calibration standard, and a kit fit
  that produces no number is refused instead of written into the kit; such a value made
  the saved calibration file unreadable. A Touchstone file with `nan`/`inf` values or
  with the same frequency twice is refused with a message.
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
- README and both manuals checked against the program and against each other. Corrected:
  the TDR range of a 50 kHz–6.3 GHz sweep in 1001 points (8 m, not 17 m), an error message
  that no longer exists, the label of a sweep outside the calibrated span (RAW), the two
  thresholds for the load check, the dips in the antenna example and the arithmetic of
  the 25 Ω example. Added: Return applies the sweep, Stop during a standard, what is
  remembered, `--port-extension` with `tdr`, the short forms of the `cal` actions, and what
  to do when the connection drops.
- The screenshots in the manual date from 0.2.3 and 0.2.4; the manual now says so.
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

[Unreleased]: https://github.com/pa1bh/vna-r/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/pa1bh/vna-r/releases/tag/v0.4.1
[0.4.0]: https://github.com/pa1bh/vna-r/releases/tag/v0.4.0
[0.3.6]: https://github.com/pa1bh/vna-r/releases/tag/v0.3.6
[0.3.5]: https://github.com/pa1bh/vna-r/releases/tag/v0.3.5
[0.3.4]: https://github.com/pa1bh/vna-r/releases/tag/v0.3.4
[0.3.3]: https://github.com/pa1bh/vna-r/releases/tag/v0.3.3
[0.3.2]: https://github.com/pa1bh/vna-r/releases/tag/v0.3.2
[0.3.1]: https://github.com/pa1bh/vna-r/releases/tag/v0.3.1
[0.3.0]: https://github.com/pa1bh/vna-r/releases/tag/v0.3.0
[0.2.5]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.5
[0.2.4]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.4
[0.2.3]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.3
[0.2.2]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.2
[0.2.1]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.1
[0.2.0]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.0
[0.1.0]: https://github.com/pa1bh/vna-r/commit/1a78b67
