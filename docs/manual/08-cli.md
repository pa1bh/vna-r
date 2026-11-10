# 8. Command line: vna-cli

All commands accept `-p, --port <PORT>` (otherwise the LiteVNA is found automatically)
and `--trace` (prints every byte that goes over USB, for troubleshooting). `--help` on
any command gives the full list.

Frequencies: `50k`, `1M`, `14.2M`, `1.3G`, `6.3G` (a decimal comma is accepted too). Points: 2 to 65535.

With `--json`, the commands `list`, `info`, `sweep`, `tdr` and `cal show` print one JSON
document on stdout instead of a table, for use in scripts; progress and warnings stay on
stderr. A sweep gives every point as `{"freq_hz": …, "s11": [re, im], "s21": [re, im]}`,
`tdr` the listed reflections under `peaks` and the whole curve under `trace`.

## list

Shows the serial ports and marks the VNA.

```bash
vna-cli list
```

## info

Identity and battery voltage: on a LiteVNA the identity registers (variant,
protocol version, hardware revision, firmware), on a NanoVNA-F the model, firmware, build,
range and points per scan.

```bash
vna-cli info
```

## peek

Reads a register of a LiteVNA, for those who want to explore the protocol (`docs/01`,
Dutch). A NanoVNA-F has no registers; you explore it with the probe from `docs/06`.

```bash
vna-cli peek F0
```

```bash
vna-cli peek --size 2 5C
```

## sweep

One sweep, printed as a table and exported if desired.

| option | meaning |
|---|---|
| `--start`, `--stop`, `--points` | the grid; default 1–30 MHz, 101 points, or the grid of `--cal` |
| `--cal <file>` | apply a calibration; a different grid within the span is interpolated |
| `--segment START:STOP:POINTS[@cal]` | several segments in one trace, repeatable; replaces start/stop/points/cal |
| `--s2p <file>`, `--s1p <file>`, `--csv <file>` | export (`--s1p` is S11 only); `--csv -` to stdout |
| `--rows <n>` | number of table rows, 0 is all |
| `--average <1..80>` | bandwidth of the device (stays set); LiteVNA only |
| `--channel s11\|s21` | measure one channel; LiteVNA only |
| `--gate START:STOP`, `--gate-notch`, `--vf` | time gate on S11, in metres one way |
| `--calibrated` | let the device apply its own calibration (LiteVNA mode 3, NanoVNA-F without mask bit 8) |
| `--fifo-all` | use the LiteVNA variant of READFIFO |
| `--keep-usb-mode` | do not release the device after the measurement (LiteVNA: screen stays frozen; NanoVNA-F: its own sweep is not restored) |
| `--keep-settings` | do not restore the device's average and channel |

On a NanoVNA-F, sweeps of more than 301 points are automatically split into chunks;
expect about 115 points per second.

Examples:

```bash
vna-cli sweep --start 100M --stop 1G --points 1001 --cal cal/n-6g3.cal.json --s2p antenna.s2p
```

```bash
vna-cli sweep --segment 100k:50M:1001@cal/n-hf.cal.json --segment 50M:6.3G:1001@cal/n-6g3.cal.json --rows 40
```

```bash
vna-cli sweep --cal cal/n-6g3.cal.json --gate 0.3:0.8 --vf 0.70 --s2p gated.s2p
```

## cal

Measure calibration standards into a file, or inspect and edit the file.
`--file` is required.

| what | does |
|---|---|
| `open`, `short`, `load` | the three reflection standards; the first one fixes the grid (`--start/--stop/--points`) |
| `through`, `isolation` | for S21; isolation is optional |
| `line-open`, `line-short` | a cable with an open or a short at the end respectively, for `fit` |
| `show` | print contents and status |
| `kit` | set kit definitions: `--open-c` fF, `--open-delay` ps, `--short-l` pH, `--short-delay` ps, `--load-r` Ω, `--through-delay` ps, `--kit-name` |
| `fit` | estimate the open/short asymmetry from line-open/line-short; `--f-min` (default 300M), `--apply`, `--as-delay` |
| `check` | what the calibration leaves behind on the line measurements: ripple per band, after gating, near-end term |
| `clear` | delete the file |

`--note "text"` stores a note in the file. An implausible standard measurement gives a
WARNING on stderr.

```bash
vna-cli cal open --file cal/n-hf.cal.json --start 100k --stop 50M --points 1001
```

```bash
vna-cli cal fit --file cal/n-6g3.cal.json --f-min 300M --apply
```

```bash
vna-cli cal check --file cal/n-6g3.cal.json
```

## tdr

Sweeps (or reads a file), transforms S11 to the time domain and lists the strongest
reflections with distance, |Γ| and the impedance behind them.

| option | meaning |
|---|---|
| `--cal <file>` | strongly recommended |
| `--vf <0.1..1>` | velocity factor, default 0.70 |
| `--window rectangular\|hann\|blackman` | default hann |
| `--peaks <n>` | number of reflections, default 5 |
| `--max-distance <m>` | do not look further than this |
| `--csv <file>` | distance/time/impulse/step/impedance |
| `--from <file>` | S11 from an .s1p/.s2p instead of sweeping |

```bash
vna-cli tdr --cal cal/n-6g3.cal.json --start 50k --stop 6.3G --points 1001 --vf 0.70
```

## screenshot

The device's LCD as a PNG. `--swap-bytes` reverses the byte order of the pixels
if the colours are wrong.

```bash
vna-cli screenshot --out screen.png
```

## release

Gives the screen back to the device: a LiteVNA that has got stuck in USB MODE
leaves it, a NanoVNA-F gets its own sweep back and starts sweeping again.

```bash
vna-cli release
```

## Exit codes and output

Tables and data go to stdout, messages and warnings to stderr, so that
`--csv -` can be used cleanly in a pipe. An error gives exit code 1 with a
readable message.
