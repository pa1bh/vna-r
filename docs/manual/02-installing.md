# 2. Installing and connecting

## Building

vna-r is written in Rust. You need a stable Rust toolchain, 1.95 or later
([rustup](https://rustup.rs)). Fetch the source code and build the three parts:

```bash
git clone https://github.com/pa1bh/vna-r.git
```

```bash
cd vna-r && cargo build --release
```

The programs are then in `target/release/`: `vna-cli` and `vna-gui`. You can also start them
directly through cargo:

```bash
cargo run --release -p vna-gui
```

```bash
cargo run --release -p vna-cli -- info
```

### macOS

Works without extra drivers, on Apple Silicon and Intel. LiteVNA and NanoVNA-F
appear as `/dev/cu.usbmodem…`.

### Linux

Building needs two system packages: `pkg-config` and `libudev-dev` (for the serial
port). On Debian and Ubuntu:

```bash
sudo apt-get install -y pkg-config libudev-dev
```

The file dialogs of the GUI use the desktop portal (`xdg-desktop-portal` with the
backend of your desktop environment). On a bare window manager without a portal they
do not appear; install a backend such as `xdg-desktop-portal-gtk` then. The GUI reaches
the portal through `libdbus-1.so.3`, loaded when the first dialog opens; every desktop
distribution has it.

The port is called `/dev/ttyACM0` or higher. To be allowed to open it without root, install
the udev rule from the repository:

```bash
sudo cp linux/70-litevna.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger
```

Then unplug the USB cable briefly and plug it back in. An alternative is to add yourself to the group
`dialout` (Debian/Ubuntu) or `uucp` (Arch) and log in again.

### NixOS

The repository has a `flake.nix`. Flakes are off on a default NixOS; put
`nix.settings.experimental-features = [ "nix-command" "flakes" ];` in `configuration.nix`
or pass it with each invocation. Without installing anything:

```bash
nix --extra-experimental-features 'nix-command flakes' run github:pa1bh/vna-r
```

(Append `#vna-cli` for the command line.) To build and develop yourself,
`nix develop` gives a shell with Rust, pkg-config, udev and the graphics libraries that
the GUI loads at run time; in it `cargo run --release -p vna-gui` simply works.
The package includes the udev rule, so in the system configuration

```nix
services.udev.packages = [ vna-r ];
```

suffices, with `vna-r` the package from the flake, or more simply `users.users.<name>.extraGroups = [ "dialout" ];`.
The flake has been built in CI on x86_64-linux and run on a NixOS laptop, CLI and GUI.

## Ready-made binaries

On every tag `v*`, GitHub Actions builds one archive per platform and attaches them to a
release under *Releases* on GitHub, with `vna-cli`, `vna-gui`, the manual (English and
Dutch) and on Linux the udev rule: `aarch64-macos` (Apple Silicon), `x86_64-macos` (Intel
Mac), `x86_64-linux` and `x86_64-windows` (zip). They are ordinary dynamically linked
binaries: on Linux the distribution's `libudev` and a desktop portal must be present, and on NixOS
they do not work (use the flake there). Between tags there is only the CI build, which
publishes nothing; build from source yourself in that case.

Nothing is code-signed. **macOS** therefore refuses a binary that was downloaded with a
browser and unpacked with the Finder ("cannot be opened because the developer cannot be
verified"). Unpack in the Terminal, which does not mark the files:

```bash
tar xzf vna-r-v*-aarch64-macos.tar.gz
```

or remove the mark afterwards:

```bash
xattr -dr com.apple.quarantine vna-r-v*-aarch64-macos
```

**Windows** shows a SmartScreen warning for the same reason; *More info ▸ Run anyway*
starts the program. The Windows build compiles and passes CI but has not yet been tried
with a device; reports are welcome. Ports are named `COM3` and the like; Windows 10 and
later include the USB serial driver that the LiteVNA and the NanoVNA-F use. What has changed per version is listed in
`CHANGELOG.md`; which version you are running is shown under **Help ▸ About** or with
`vna-cli --version`.

## Connecting

Switch the device on and connect it with a USB cable that carries data; some charging cables
have no data lines and then no port appears. Check:

```bash
cargo run --release -p vna-cli -- list
```

The line that starts with `LiteVNA` or `NanoVNA` is the device: a LiteVNA has USB id
`04B4:0008`, a NanoVNA-F or -H `0483:5740`. On macOS the LiteVNA 67 is called
`/dev/cu.usbmodemv1_01`, the LiteVNA 64 `/dev/cu.usbmodemDEMO1` and the NanoVNA-F
`/dev/cu.usbmodem` followed by its serial number; the name comes from the firmware and says
nothing about the device. vna-r chooses the protocol itself based on the USB id. Then:

```bash
cargo run --release -p vna-cli -- info
```

That shows the model, the firmware version and the battery voltage (and for a NanoVNA-F the
range and the number of points per scan). If this works, everything works.

## What the device's screen does

As soon as vna-r requests a measurement, the device switches its screen to **USB MODE** and stops
sweeping on its own; the buttons do not respond. That is normal. After a measurement with the CLI, and
after **Disconnect** or closing the GUI, the device gets its screen back. If it
stays stuck in USB MODE, for instance after the computer has crashed,
`vna-cli release` or switching it off and on again helps.

Two settings of the device are touched by vna-r and stay that way afterwards,
because they cannot be read back reliably: the *average* (bandwidth) and the measured
channel. After a session the LiteVNA's menu may therefore show a different bandwidth than
before; set it back if you wish.

A **NanoVNA-F** has no USB MODE: its screen simply keeps working. During a
measurement by vna-r its own sweep is temporarily set to the grid of the last chunk that
vna-r measured (at most 301 points per chunk; longer sweeps are done in chunks), and
after **Disconnect**, the end of a CLI command or `vna-cli release`, vna-r restores the
original sweep. Its bandwidth is set on the device itself; the
**Average** box in the GUI is disabled for this device.

## One program at a time

The serial port can only be open in one program at a time. If the GUI is connected, the CLI
gives an error saying the port is busy; click **Disconnect** first. The same
applies to NanoVNA-Saver or any other program that talks to the device.
