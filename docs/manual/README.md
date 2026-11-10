# vna-r manual

*Nederlandse versie: [docs/handleiding](../handleiding/README.md).*

A vector network analyser (VNA) measures how an antenna, cable, filter or other
RF component behaves as a function of frequency. vna-r is a program for the macOS, Linux
and Windows desktop that drives a LiteVNA 64 or 67 or a NanoVNA-F over USB, calibrates the measurements itself and
displays them, with markers, time domain (TDR), gating, references and export. This manual
explains what you are measuring, how to operate the program and where the pitfalls are.

The chapters build on one another, but can also be read on their own:

1. [Introduction: what a VNA measures](01-introduction.md). S11, S21, reflection coefficient, SWR,
   impedance, dB and phase, the Smith chart, and how vna-r is put together.
2. [Installing and connecting](02-installing.md). Building on macOS and Linux, the USB port,
   first contact, what the device's screen does during a measurement.
3. [The GUI](03-gui.md). Menu bar, sweep, segments, views, markers, averaging,
   settings that are remembered.
4. [Calibrating](04-calibrating.md). Why it is necessary, open/short/load/through step by step,
   files, interpolation to another grid, the kit fit above 1 GHz, checks and
   pitfalls.
5. [Time domain: TDR and gating](05-time-domain.md). From frequency to distance, the two
   graphs, windows, resolution and range, measuring cables, cutting out a reflection.
6. [Measurement recipes](06-measurement-recipes.md). Tuning an antenna, measuring a cable (length,
   velocity factor, loss), measuring a filter and an attenuator, before/after comparison.
7. [Files](07-files.md). Exporting and reading Touchstone files, calibration files,
   references, screenshots.
8. [Command line: vna-cli](08-cli.md). All commands and options, with examples.
9. [Devices, platform and troubleshooting](09-devices-and-troubleshooting.md). LiteVNA 64
   and 67 and the NanoVNA-F side by side, Linux permissions, common error messages,
   reporting a problem, what you can trust.

Those who want the background will find the research and development notes in `docs/01`
to `docs/08` (Dutch): the USB protocols, the calibration model with measurements, the TDR
mathematics, the roadmap, and the review and test procedure of version 0.3.

## Conventions

- Frequencies are typed as `14.2M`, `50k`, `1.3G` or `6,3G`; a comma is accepted as decimal separator.
- dB values of S11 and S21 are negative or zero: 0 dB is everything reflected or everything passed,
  −20 dB is 1% of the power.
- Distances in TDR are one way (the length of the cable), times are round trip.
- The screenshots were made with versions 0.2.3 and 0.2.4. The panels have gained a few
  fields since, and the alias-free range in the TDR screenshots is twice what the current
  version reports for the same sweep (chapter 5 has the current definition).
- "Reference plane" is the point at which the calibration places the measurement: with standards
  directly on port 1 that is the N connector of the device.
