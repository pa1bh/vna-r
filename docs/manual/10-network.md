# 10. Measuring over the network

## What it is for

The analyser does not have to sit next to the computer you look at it with. Put it on a
small computer that runs **vna-server**, a Raspberry Pi for example, and measure from a
laptop, a tablet or a phone on the same network:

- at the feed point of the antenna, on the mast or in the garden, with a Pi and a power
  bank, while you adjust and watch the SWR on a tablet;
- in the shack or the attic, while you sit somewhere else;
- with several people looking at one measurement, at a club evening or a course.

The server drives the analyser exactly as the program does over USB; the measurement
itself does not change. You look at it with the vna-r program on a computer, or with a
browser, in which case nothing needs installing.

## What you need

- A LiteVNA or a NanoVNA-F, as for everything else.
- A computer for the server, with a USB port: a Raspberry Pi 3, 4, 5 or Zero 2 W with a
  64-bit Raspberry Pi OS (Bookworm or later), or any Linux, macOS or Windows computer.
  What has been tried is a Raspberry Pi 4 with Raspberry Pi OS on Debian 13, and a Mac;
  the other models run the same archive but have not been tried.
- For a LiteVNA on a Raspberry Pi: count on a USB hub with its own power supply, see
  "Power" below.
- A network both are on: the wifi at home, the hotspot of a phone.
- Something to look with: the vna-r program on a computer, or a browser.

## vna-server on a Raspberry Pi

From the Releases page take the archive `…-aarch64-linux.tar.gz`. It holds `vna-server`,
the udev rule and `vna-server.service`, which starts the server whenever the Pi boots. On
the Pi, in the folder where you saved it:

```bash
tar xzf vna-r-v*-aarch64-linux.tar.gz && cd vna-r-v*-aarch64-linux
```

```bash
sudo install -m 755 vna-server /usr/local/bin/
sudo cp 70-litevna.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger
sudo cp vna-server.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now vna-server
```

The server now runs, and starts again after every boot and after a failure. It looks for
the analyser by itself every two seconds, so it does not matter whether you plug it in
before or after. You can also swap analysers without restarting the server: it connects
to whatever is plugged in then, a LiteVNA or a NanoVNA-F. What it does is in the log:

```bash
journalctl -u vna-server -f
```

There you see the address it listens on, when the analyser is connected or lost, and who
connects. The service runs as an ordinary user in the group `dialout`, which may open the
serial port; the udev rule gives the analyser's port to that group. `sudo systemctl stop
vna-server` stops it, and the analyser then gets its own screen back.

The archive is built for 64-bit ARM. It does not run on a 32-bit Raspberry Pi OS, on the
first Pi Zero, or on a Raspberry Pi OS older than Bookworm.

**Power.** The USB ports of a Raspberry Pi 4 supply about 1.2 A between them. A LiteVNA
that is switched on and charging its battery at the same time can ask for more. The Pi
then switches the ports off for a moment, the analyser drops off the USB and comes back,
and that repeats every few seconds. The page keeps saying the server has no analyser, the
server's log reports `connecting to /dev/ttyACM0: I/O error: Broken pipe`, and `sudo dmesg
| grep over-current` on the Pi shows the cause. Seen this way with a LiteVNA 64 on a Pi 4
with the official 3 A supply; a NanoVNA-F on the same port had no trouble. A USB hub with
its own power supply between the Pi and the analyser is the obvious cure; charging the
analyser fully first may help too. Neither has been tried here yet. A heavier supply for
the Pi itself does not help: the limit is in the ports.

## Starting it by hand

The server is also in the archives for the other platforms, and runs on any computer the
analyser is plugged into (on Windows the program is `vna-server.exe`):

```bash
./vna-server
```

It listens on port 8080 on every network connection of the computer and connects to the
first analyser it finds. A few options change that:

- `--port /dev/ttyACM0` (or `COM3`): this serial port, when there are several;
- `--listen 192.168.1.5:9000`: another address or port; `127.0.0.1:8080` keeps it to this
  computer;
- `--name shack`: the name the viewers see (by default the name of the computer);
- `--simulate`: a simulated analyser, see below;
- `--web <folder>`: the page from that folder instead of the copy inside the program.

Ctrl-C stops it; it lets go of the analyser first. A firewall (Windows, or macOS with its
firewall switched on) may ask on the first start whether the program may accept
connections; allow it.

## In the browser

Open `http://<name of the Pi>:8080/` in a browser, for example
`http://raspberrypi.local:8080/`. If the name does not work, use the Pi's IP address
(`hostname -I` on the Pi shows it, and so does the list of devices in your router).

The page is the vna-r interface. It connects to its server by itself, and the top bar shows
the server where the port list would be. From there everything works as in chapter 3:
Run, Single, the sweep panel, the calibration, the markers, TDR. The green line shows the
analyser and, after `via`, the name of the server; hovering over it shows the server's
version and the analyser's port on the server.

## In the program: Device ▸ Add server…

In the vna-r program on a computer, choose **Device ▸ Add server…**, type the address of the
server (`raspberrypi.local`, `192.168.1.5`, or with a port, `192.168.1.5:9000`; without a
port 8080 is meant) and click **Add**. The server then appears in the port list under the
serial ports, as `server raspberrypi.local:8080`, and is selected: **Connect** connects.
The same window lists the servers you know, each with **remove**.

The servers are remembered. With **View ▸ Connect at start** the program connects to a
selected server at start, as it does to an analyser on USB.

The page on a website (rather than the page from the server) has the same **Device ▸ Add
server…**; under "What this version does not have" you can read when that works.

## Several people on one analyser

Any number of viewers can connect to one server, programs and browsers at the same time.
They all see the same analyser:

- Everyone can steer. Press **Run** in one browser, and the others see their **Stop**
  button appear and the same sweep come in; anyone can press Stop.
- Everyone has their own sweep settings, and the analyser does what was asked last. So the
  trace you see can be of someone else's sweep: it then runs over their frequencies and
  points, not over what your own Start and Stop fields say. Press **Run** or **Single**
  yourself and the analyser measures with your settings again, for everyone.
- Everyone has their own calibration. The server passes on raw measurements only; the
  calibration is applied in each program or browser. When you measure a standard, only you
  get the result.
- A screenshot of the device goes only to the one who asked for it.
- When the last viewer closes the page or disconnects, the server stops sweeping and the
  analyser gets its own screen back.

## When the line turns orange

The top bar says what the program is waiting for:

- **"no analyser on the server yet, waiting for one…"**: the server runs, but no analyser is
  plugged in. Plug it in and the program connects by itself. This is not an error; you see
  it right after Connect when the analyser is not there yet.
- **"the analyser dropped off the server, waiting…"**: the analyser left the server's USB
  (its cable or connector moved, or it restarted). The server waits for it, as the program
  does over USB (chapter 9), and a sweep that was running carries on afterwards.
- **"the server lost its analyser, waiting for one…"**: the analyser did not come back
  within 30 seconds. Plug it in again.
- **"connection to the server dropped, waiting for …"**: the network connection went: the
  wifi out of reach, or the server restarted. The program tries again by itself for two
  minutes, with growing pauses, and carries on with Run once the server is back. A
  Raspberry Pi that reboots is usually back within that time.

**Cancel** stops the waiting, whatever it is for.

## What is different in the browser

- Files go through the browser: **Load…** opens the browser's file dialog, **Save…**
  (calibration, Touchstone, screenshot) downloads the file.
- A segment has no calibration file of its own; every segment uses the calibration of the
  calibration panel, interpolated where needed.
- Held references stay while the page is open, not after a reload.
- The settings (sweep, views, the servers you added) are kept in the browser, per address.

## Trying it without an analyser

`vna-server --simulate` starts a server with a simulated analyser: a multiband antenna on
port 1 and a band-pass filter on port 2, with a little noise, at the speed of a LiteVNA.
Calibration standards measure as ideal ones. That way you can try the server, the browser
page and several viewers without an analyser. In the program itself, **View ▸ Developer
tools** and then **Developer ▸ Connect to simulator** connects to the same simulated
analyser inside the program.

## What this version does not have

- **No password.** Everyone on the network can open the page, see the measurement and
  steer the analyser. Use it on a network you trust: at home, at the club, the hotspot of
  your phone. `--listen 127.0.0.1:8080` keeps it to one computer.
- **The page on a website.** A page that comes over https, as a website does, may open an
  unencrypted connection to a device on the local network only in Chrome and Edge, and
  only after you allow it; Safari and Firefox refuse. Normally you therefore open the page
  the server itself serves (`http://…:8080/`), which works in every browser.
- **A phone held upright.** The layout is not yet made for a narrow screen: turn the phone
  sideways, or use a tablet.
- **The Nix package.** Its server has no browser page built in; the page says so. The
  program on a computer connects to it all the same.
