# NanoVNA-F: het tekstprotocol gemeten

*Status: 4 november 2026, gemeten op een NanoVNA-F 4.3 met firmware 1.0.4 (BH5HNU).
Dit was roadmapstap 10; de LiteVNA-H1 waar die stap voor bedoeld was is niet getest.*

## 1. Samenvatting

De NanoVNA-F spreekt niet het binaire registerprotocol van de LiteVNA
(`docs/01`), maar de **tekst-shell** van de NanoVNA-familie: een commandoregel over
USB CDC met prompt `ch> `. Voor vna-r is het belangrijkste dat er een binair
scan-commando is, `scan_bin`, dat **ruwe** S11 en S21 geeft (voor de calibratie op de
host) met maximaal 301 punten per aanroep. Daarmee past het apparaat achter dezelfde
`Vna`-handle als de LiteVNA: calibratie, TDR, gating, segmenten en alles daarboven
werken ongewijzigd. Het is wel traag: ~115 punten/s met S11+S21.

## 2. USB-identificatie

| | |
|---|---|
| VID:PID | `0483:5740` (STMicroelectronics virtual COM port) |
| fabrikant / product | `DEEPELEC.COM` / `NanoVNA-F Virtual COMPort` |
| poort op macOS | `/dev/cu.usbmodem<serienummer>1`, hier `/dev/cu.usbmodem5CF9774D36361` |

`0483:5740` is de standaard-VCP van elk STM32-bordje, dus vna-r kijkt ook naar de
productnaam: bevat die "nanovna" of "vna", of ontbreekt hij, dan wordt de shell
geprobeerd; een ander product op dat id wordt overgeslagen.

## 3. De shell

Elke regel eindigt op `\r`; het apparaat echoot de regel, antwoordt, en sluit af met
`\r\nch> `. Een lege regel geeft alleen een prompt, wat vna-r gebruikt om te
synchroniseren. `help` op deze firmware:

```
scan scan_bin data frequencies freq sweep power offset bandwidth saveconfig
clearconfig port gain test task touchcal touchtest pause resume cal save recall
trace marker edelay capture vbat reset vbat_offset transform threshold help info version
```

Gebruikt door vna-r:

| commando | antwoord | opmerking |
|---|---|---|
| `info` | `Model: NanoVNA-F 4.3`, `Version: Firmware 1.0.4 by BH5HNU`, `Build time: …` | identificatie |
| `version` | `1.0.4` | |
| `vbat` | `4209 mV` | |
| `sweep` | `1000000 30000000 301` | de eigen sweep van het scherm: start stop punten |
| `sweep S E N` | – | zet die |
| `pause` / `resume` | – | eigen sweep stoppen/hervatten |
| `bandwidth [n]` | `bandwidth 0 (1000Hz)` | 0 = 1 kHz, 1 = 500 Hz, …; blijft staan |
| `scan_bin S E N M` | binair, zie §4 | de meting |
| `capture` | 768000 bytes | zie §6 |

Overige antwoorden die we zagen: `cal` → `load isoln Es Er Et cal'ed` (statusvlaggen),
`threshold` → `current: 280000000` (boven 280 MHz werkt het apparaat op harmonischen),
`power` → `power: 3`, `edelay` → `0.000000000`, `data 0` → 301 regels `re im` van de
schermsweep (S11), `data 1` → allemaal nullen terwijl `scan` wel S21 gaf.

## 4. `scan` en `scan_bin`

```
scan     {start(Hz)} {stop(Hz)} [points] [outmask]      tekst, één regel per punt
scan_bin {start(Hz)} {stop(Hz)} [points] [outmask]      binair
```

Maskerbits: `1` frequentie, `2` S11, `4` S21, `8` **ongecalibreerd** (data vóór de
calibratie van het apparaat zelf). Bit 16 doet in `scan` niets; het binaire formaat
is een apart commando.

Het binaire antwoord, alles little-endian:

```
<echo van de commandoregel>\r\n
u16 mask        0x80 | gevraagd masker (0x83, 0x87, 0x8b, 0x8f gezien)
u16 points
per punt:       [u32 freq_hz] [f32 s11_re, f32 s11_im] [f32 s21_re, f32 s21_im]
ch>
```

Gemeten eigenschappen:

- **Maximaal 301 punten**: `scan 1000000 30000000 401 3` geeft
  `sweep points exceeds range 301`. vna-r knipt langere sweeps in stukken op
  hetzelfde raster (1001 punten = 301+301+301+98) en plakt ze aaneen.
- De frequenties komen exact uit op `start + i·(stop−start)/(points−1)` in
  integer-rekenwerk; met `stop = start + stap·(points−1)` is het raster dus identiek aan
  dat van vna-r. 1 punt geeft alleen `start`; `start == stop` herhaalt het punt;
  `stop < start` geeft `frequency range is invalid`.
- **Geen bereikcontrole**: 5 kHz en 2 GHz worden gewoon gemeten (en zijn buiten
  10 kHz–1,5 GHz betekenisloos). vna-r waarschuwt in de GUI.
- **Bijwerking**: een scan zet het puntenaantal van de schermsweep op dat van de scan
  en `frequencies` op het scanraster; `sweep` blijft wel de oude start/stop melden.
  vna-r onthoudt `sweep` voor de eerste scan en zet die bij het loslaten van het
  apparaat terug, gevolgd door `resume`.
- Het apparaat stuurt **niets** tot alle punten gemeten zijn; de lezer moet dus op
  byte-aantal lezen, niet op stilte.
- De ongecalibreerde open poort meet `0,678 ∠ −0,3°` bij 1 MHz (|Γ| −3,4 dB), de
  gecalibreerde `1,001`. Bit 8 heeft op de kalibratie van `scan` zelf geen blijvend
  effect: een latere `scan` zonder bit 8 geeft weer 1,00.

## 5. Snelheid

Bij bandbreedte 0 (1 kHz):

| meting | tijd | tempo |
|---|---|---|
| 301 punten S11 | 1,3 s | ~230 pts/s |
| 301 punten S11 + S21 | 2,6 s | ~115 pts/s |
| 1001 punten S11 + S21 (4 stukken) | 8,7 s | ~115 pts/s |

Ter vergelijking: de LiteVNA 67 doet 340–600 punten/s, de 64 ~300. Tekst (`scan`) is
even snel als binair; de tijd zit in het meten, niet in de USB.

## 6. `capture`

Na de echo volgen 800 × 480 pixels RGB565 **little-endian** (768000 bytes), dan de
prompt; ongeveer 3 s. Geen header, dus vna-r kent de maat aan het model toe
(NanoVNA-F 800×480, NanoVNA-H 320×240, H4 480×320) en valt voor een onbekend model
terug op het aantal bytes voor de prompt. De LiteVNA stuurt zijn pixels big-endian;
`Screenshot` draagt daarom zijn eigen byte-volgorde mee.

## 7. Implementatie in vna-r

- `crates/litevna/src/shell.rs`: `NanoVnaShell` (open en synchroniseren, `info`,
  `vbat`, `sweep`/`set_sweep`, `pause`/`resume`, `bandwidth`, `scan_bin` in stukken,
  `capture`, herstel bij `Drop`) plus pure functies met tests: `parse_info`,
  `scan_chunks`, `parse_scan_header`, `parse_scan_points`.
- `crates/litevna/src/vna.rs`: `enum Vna { Lite, Shell }` met `open` (kiest op USB-id,
  anders eerst LiteVNA dan shell), `info` → `VnaInfo` (model, firmware, `Limits`),
  `sweep`, `screenshot`, `battery_millivolts`, `release`.
- `crates/litevna/src/discovery.rs`: `Kind` per poort.
- CLI en GUI gebruiken `Vna`; LiteVNA-specifieke dingen (`peek`, `--average`,
  `--channel`, Average in de GUI) melden dat ze een LiteVNA nodig hebben of staan uit.

Verkennen van een andere firmware kan met de probe die hiervoor gebruikt is:

```bash
cargo run --release -p litevna --example probe_shell -- /dev/cu.usbmodemXXXX help info "scan 1000000 30000000 11 3"
```

(`raw:` voor een hexdump, `bin:` voor een `scan_bin` op byte-aantal.)

## 8. Niet getest of open

- NanoVNA-H, H4 en de LiteVNA-H1: zelfde shell-familie maar andere firmware (edy555 /
  hugen / DiSlord). `scan_bin` bestaat in DiSlord-firmware ≥ 1.0.x met hetzelfde
  masker; oudere firmware heeft alleen tekst-`scan` of zelfs alleen `data`. Als de
  H1 geen `scan_bin` blijkt te hebben, is een tekst-fallback de volgende stap.
- Calibratie op de NanoVNA-F met de SMA-kit (het apparaat zelf is nu geverifieerd,
  de meting met kit nog niet).
- Het scherm toonde na onze scans SWR ≈ 5 voor de open poort terwijl `scan` zonder bit 8
  nog steeds 1,00 gaf; SWR 5,2 is precies de ruwe |Γ| 0,678. De `cal'ed`-vlag staat na
  een sessie met ruwe scans echter nog gewoon aan (gecontroleerd), dus het is niet de
  apply-vlag; vermoedelijk passen de calibratietabellen van het apparaat (per puntindex)
  niet meer bij het door de scan gewijzigde raster tot `sweep S E N` ze opnieuw laat
  interpoleren. vna-r zet daarom bij het loslaten eerst de sweep terug en stuurt dan
  `cal on` (bestaat, is onschuldig) en `resume`.
- De ruwe open van −3,4 dB is normaal voor dit apparaat; de plausibiliteitscheck van de
  calibratie (die op de LiteVNA ≈ 0 dB verwacht) waarschuwt daarom pas onder −8 dB.
