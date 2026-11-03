# vna-r

Rust-gereedschap voor de **LiteVNA 67** (en andere LiteVNA / NanoVNA V2 apparaten) op macOS en Linux.
Hobbyproject van PA1BH.

Status: proof of concept. Zie [docs/01-onderzoek-protocol.md](docs/01-onderzoek-protocol.md) voor
het protocolonderzoek.

## Opbouw

| Crate | Wat |
|---|---|
| `crates/litevna` | Library: protocol (opcodes, registers, FIFO-records), device-API, poortdetectie, Touchstone-export. Geen UI. |
| `crates/vna-cli` | Commandline-POC bovenop de library. |
| `crates/vna-gui` | Desktop-GUI (egui/eframe): live Smith-chart, logmag, SWR, fase, marker, calibratie, export. |

## Gebruik

```bash
cargo build --release
```

```bash
./target/release/vna-cli list                       # welke seriële poorten zijn er, welke is de VNA
```

```bash
./target/release/vna-cli info                       # variant, protocol, hardware, firmware, accu
```

```bash
./target/release/vna-cli sweep --start 1M --stop 30M --points 201 --s2p hf.s2p
```

```bash
./target/release/vna-cli screenshot --out scherm.png
```

Handig bij problemen: `--trace` toont elke verzonden/ontvangen byte, `peek F0` leest een register.
Na een sweep geeft de CLI het scherm terug aan het apparaat; met `release` kun je dat ook los doen.

### GUI

```bash
cargo run --release -p vna-gui
```

Kies de poort (★ = LiteVNA), **Connect**, **Run**. Links de sweep-instellingen en het calibratiepaneel:
meet open/short/load (en optioneel through/isolation) op het huidige raster, zet **apply** aan en sla
de calibratie op als JSON; dezelfde bestanden werken ook met `vna-cli sweep --cal`. Klik in een grafiek
of op de Smith-chart voor een marker met frequentie, S11, impedantie, SWR en S21.

### Calibratie

Zie [docs/03-calibratie.md](docs/03-calibratie.md). Kort: `vna-cli cal open|short|load|through --file x.json`,
daarna `vna-cli sweep --cal x.json`. Een ander raster binnen het gecalibreerde bereik wordt geïnterpoleerd.

## Platform

- macOS: werkt zonder driver, poort heet `/dev/cu.usbmodem…` (USB id `04B4:0008`).
- Linux: `/dev/ttyACM*`; de `serialport`-crate heeft `libudev-dev` nodig om te bouwen.
