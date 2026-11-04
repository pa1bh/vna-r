# vna-r

Rust-gereedschap voor de **LiteVNA 67** (en andere LiteVNA / NanoVNA V2 apparaten) op macOS en Linux.
Hobbyproject van PA1BH.

**Handleiding:** [docs/handleiding](docs/handleiding/README.md): wat een VNA meet, installeren, de GUI,
calibreren, TDR en gating, meetrecepten, bestanden, de CLI en problemen oplossen.

Status: werkend op LiteVNA 64 en 67. Zie [docs/01-onderzoek-protocol.md](docs/01-onderzoek-protocol.md) voor
het protocolonderzoek en [docs/05-roadmap.md](docs/05-roadmap.md) voor wat er nog komt.

## Opbouw

| Crate | Wat |
|---|---|
| `crates/litevna` | Library: protocol (opcodes, registers, FIFO-records), device-API, poortdetectie, SOLT-calibratie met kit-fit en interpolatie, TDR, Touchstone-export. Geen UI. |
| `crates/vna-cli` | Commandline-POC bovenop de library. |
| `crates/vna-gui` | Desktop-GUI (egui/eframe): live Smith-chart, logmag, SWR, fase, TDR-weergave, marker, calibratie, export. |

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

```bash
./target/release/vna-cli tdr --cal cal/n-6g3.cal.json --vf 0.70 --max-distance 3   # reflecties op afstand
```

Handig bij problemen: `--trace` toont elke verzonden/ontvangen byte, `peek F0` leest een register.
Na een sweep geeft de CLI het scherm terug aan het apparaat; met `release` kun je dat ook los doen.

### GUI

```bash
cargo run --release -p vna-gui
```

Kies de poort (★ = LiteVNA), **Connect**, **Run**. Links de sweep-instellingen en het calibratiepaneel:
meet open/short/load (en optioneel through/isolation) op het huidige raster, zet **apply** aan en sla
de calibratie op als JSON; dezelfde bestanden werken ook met `vna-cli sweep --cal`. Voor calibraties
boven ~1 GHz: onder **Kit fit** meet je een kabel met open en short aan het verre eind, **Fit** schat
de open/short-asymmetrie van de kit (als capaciteit of offset delay) en **Apply to kit** zet die in de
calibratie; een onwaarschijnlijke standaardmeting (losse open, vergeten load) geeft een waarschuwing.

Segmenten: onder **Segments** voeg je extra bereiken toe die na het eerste geveegd en in één trace
getoond worden, elk desgewenst met een eigen calibratiebestand, bijvoorbeeld 100 kHz–50 MHz fijn met
een HF-calibratie naast 50 MHz–6,3 GHz met de brede. Zo ontloop je de interpolatie van een brede
calibratie aan de onderkant. TDR en gate gebruiken het breedste segment. CLI:
`vna-cli sweep --segment 100k:50M:1001@cal/n-hf.cal.json --segment 50M:6.3G:1001@cal/n-6g3.cal.json`.

Bestanden: onder **Files** opent **Open as trace…** een .s1p/.s2p-bestand in plaats van het apparaat (label
FILE; alle weergaven, markers en TDR werken erop, de volgende sweep vervangt het) en **Open as reference…**
laadt het als referentie. Formaten RI/MA/DB, eenheden Hz–GHz, ook bestanden van de LiteVNA zelf. CLI:
`vna-cli tdr --from meting.s2p`.

Referenties: **Hold** in de markerbalk zet de huidige trace vast (maximaal vier, elk met naam en eigen
kleur, aan/uit onder **Traces**); de knop **Δ** bij een referentie tekent live min referentie in dB en
graden in de log mag- en fase-grafieken. Referenties worden niet bewaard; exporteer ze als s2p en open ze later als referentie.

Middelen: **Sweep avg** N middelt de laatste N sweeps complex in software (teller k/N, reset bij een
ander raster), naast de *Average* van het apparaat zelf. Complex middelen drukt ruis op lage niveaus
zonder de magnitude te vertekenen, wat middelen in dB wel doet.

Weergaven: de Smith-chart staat vast; de drie andere grafieken kiezen in hun kop wat ze tonen: log mag,
lin mag, fase, SWR, R + jX, |Z| of groepsvertraging (−dφ/dω over ±N punten, N instelbaar onder
**Traces**). De rekenkant zit in `litevna::traces`.

Markers: vier stuks (M1–M4). Kies de actieve bovenin, klik in een grafiek of op de Smith-chart om hem te
plaatsen, pijltjestoetsen verschuiven hem per punt (shift: ×10). De tabel toont frequentie, S11, Z, SWR
en S21 per marker en het verschil tussen de eerste twee; knoppen springen naar min SWR, max S21, min S21.

Instellingen (poort, sweep, average, calibratiebestand, traces, TDR, markers, venstergrootte) worden bij
afsluiten bewaard en bij de volgende start hersteld; een bestaand calibratiebestand wordt automatisch
geladen.

### TDR

Zie [docs/04-tdr.md](docs/04-tdr.md). In de GUI: kies **TDR** in het linkerpaneel. Met **Gate** aan
houd je alleen de reflecties tussen twee afstanden over (of haal je ze weg met *notch*); de oranje
lijnen in de TDR-plots zijn sleepbaar en het frequentiebeeld toont dan de gegate S11 (label GATED).
CLI: `vna-cli sweep --cal x.json --gate 0.9:1.2 --s2p uit.s2p`.

### Calibratie

Zie [docs/03-calibratie.md](docs/03-calibratie.md). Kort: `vna-cli cal open|short|load|through --file x.json`,
daarna `vna-cli sweep --cal x.json`. Een ander raster binnen het gecalibreerde bereik wordt geïnterpoleerd.
`vna-cli cal check --file x.json` laat zien wat de calibratie op een kabelmeting overlaat (ripple per band,
near-end term, consistentie van de kit-fit).

## Apparaten

Geverifieerd op de LiteVNA 67 en de LiteVNA 64 (zie docs/01 §7 en §8): zelfde protocol en registers,
verschillend scherm (1024×600 vs 480×320) en USB-naam. De LiteVNA-H1 (NanoVNA-H-firmware, tekstprotocol)
staat op de roadmap.

## Platform

- **macOS** (Apple Silicon en Intel): werkt zonder driver, poort heet `/dev/cu.usbmodem…` (USB id
  `04B4:0008`).
- **Linux**: poort `/dev/ttyACM*`. Bouwen vraagt `pkg-config`, `libudev-dev` (serialport) en
  `libgtk-3-dev` (bestandsdialogen), op Debian/Ubuntu:

  ```bash
  sudo apt-get install -y pkg-config libudev-dev libgtk-3-dev
  ```

  Toegang tot de poort zonder root: kopieer [linux/99-litevna.rules](linux/99-litevna.rules) naar
  `/etc/udev/rules.d/` en herlaad udev (instructies in het bestand), of zet jezelf in de groep
  `dialout`.
- **CI**: [.github/workflows/ci.yml](.github/workflows/ci.yml) bouwt en test op Ubuntu en macOS bij elke
  push; de GUI is op Linux gebouwd en getest in een Debian-container, maar nog niet op een echte
  Linux-desktop gedraaid.
