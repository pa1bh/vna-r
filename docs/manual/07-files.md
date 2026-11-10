# 7. Files

## Touchstone (.s1p, .s2p)

Touchstone is the interchange format for S-parameters that every RF program reads:
text, one line per frequency, with an option line that states the unit and the format.

**Writing.** In the GUI, **Save Touchstone…** under **Files**; with the CLI,
`vna-cli sweep --s2p name.s2p`. vna-r writes Hz, real/imaginary, 50 Ω, with S12 and
S22 as zero because the LiteVNA does not measure them. For a one-port device such as an
antenna, give the file the extension `.s1p` in the GUI or use `--s1p name.s1p`: that
writes S11 only. The comment lines state whether the
data was calibrated, gated or averaged. With the gate on, the gated S11 is written.
For segments that touch each other, the shared point is written once, because
Touchstone requires strictly increasing frequencies.

**Reading.** **Open as trace…** shows a file instead of the device: all views, markers,
TDR and gate work on it, the label becomes **FILE**, and the calibration is not applied
a second time. The next sweep replaces it. **Open as reference…** loads it as a
reference next to the live measurement. The reader understands s1p and s2p in the
RI, MA and DB formats with units from Hz to GHz, including the files the LiteVNA itself
writes to its SD card and the output of simulation programs. A file with a reference
impedance other than 50 Ω (`R 75` on the option line) has its S11 renormalised to 50 Ω,
so impedance and SWR come out right; S21 stays as in the file, and the message under
**Files** says so. With the CLI:
`vna-cli tdr --from name.s2p`.

## Calibration files (.cal.json)

A calibration is a JSON file with the frequency grid, the kit definitions and the raw
measurements of each standard. Storing the raw measurements rather than the error terms
is what makes interpolation and the kit fit possible afterwards. GUI and CLI use the same
format; `vna-cli cal show --file x.cal.json` prints the contents in readable form.

A calibration belongs to one device, one kit and one reference plane. Store them under
recognisable names, for example `n-hf.cal.json` and `n-6g3.cal.json`, and put in `--note`
what they belong to ("N kit, directly on port 1"). The `cal/` directory in the repository
is meant for this and is listed in `.gitignore`.

## CSV

`vna-cli sweep --csv name.csv` writes S11 and S21 per frequency as real/imaginary,
dB, degrees and SWR, followed by the impedance at port 1 (R, X and |Z| in ohm) and the
group delay of S21 in nanoseconds; `--csv -` writes to standard output. `vna-cli tdr --csv`
writes distance, time, impulse, step and impedance. For spreadsheets and your own scripts.

## References

References from **Hold** are kept between sessions: vna-r writes them as `ref-0.s2p` to
`ref-3.s2p` in the folder `references` next to its settings (on macOS
`~/Library/Application Support/vna-r`, on Linux `~/.local/share/vna-r`, on Windows
`%APPDATA%\vna-r`). To keep one for good or pass it on, copy that file, or **Save
Touchstone** while the measurement is live and later **Open as reference…**.

## Screenshots

**Screenshot** in the GUI and `vna-cli screenshot --out name.png` fetch the device's LCD
as a PNG: 1024×600 on the LiteVNA 67, 480×320 on the 64, 800×480 on the
NanoVNA-F. If the colours come out wrong (on a different device), `--swap-bytes`
reverses the byte order of the pixels.

## GUI settings

The GUI stores its settings in the operating system's usual directory for application
data (via eframe). Clearing them is rarely necessary; **Clear** in the calibration panel
and the defaults in the Sweep panel are usually enough.
