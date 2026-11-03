# Onderzoek: LiteVNA 67 over USB aansturen

*Status: 3 november 2026. Bronnen staan onderaan.*

## 1. Samenvatting

Het goede nieuws: het protocol is **openbaar en klein**. De LiteVNA (62/64/67) gebruikt
hetzelfde binaire USB-protocol als de NanoVNA V2 ("S-A-A-2"), en hugen heeft de volledige
register-tabel als appendix in de officiële LiteVNA handleiding gezet. We hebben dus geen
reverse engineering nodig; wel moeten we een paar zaken op de echte LiteVNA 67 verifiëren
(zie §7).

Kernpunten:

- Het apparaat is een **USB CDC-ACM virtuele seriële poort** (op macOS `/dev/cu.usbmodemXXXX`,
  USB VID `0x04B4` / PID `0x0008`, "Cypress"). Geen driver nodig op macOS en Linux.
- Het protocol is **register-gebaseerd**: de host schrijft/leest bytes op adressen (0x00..0xFF).
  Er zijn maar 11 opcodes. Alles is little-endian. Geen framing, geen separators.
- Meetdata komt uit een **FIFO** (adres 0x30): 32 bytes per frequentiepunt met ruwe
  I/Q-waarden van de referentie (fwd0), reflectie (rev0) en doorlaat (rev1).
  `S11 = rev0 / fwd0`, `S21 = rev1 / fwd0` (complexe deling).
- Over USB krijg je **altijd ongecalibreerde ruwe data**. Calibratie (SOLT) moet de PC-software
  zelf doen. (Firmware 1.4 kent een "calibrated data"-modus via register 0x26 = 3, maar dat is
  LiteVNA-specifiek en nog niet door ons getest.)
- Max 65535 punten per sweep over USB, >1600 punten/s. Dat is ruim voldoende voor een GUI.

## 2. Het landschap: welke NanoVNA's praten welk protocol?

| Familie | Firmware | Protocol | Voorbeelden |
|---|---|---|---|
| NanoVNA "v1" (edy555/hugen/DiSlord) | open source | **Tekst-shell** (`info`, `sweep`, `data 0`, …) | NanoVNA-H, NanoVNA-H4 |
| NanoVNA V2 / S-A-A-2 (OwOComm) | open source | **Binair register-protocol** | V2, V2 Plus, V2 Plus4 |
| **LiteVNA** (hugen, Zeenko) | **closed source** | zelfde binaire protocol als V2, met extra registers | LiteVNA 62, 64, **67** |
| NanoVNA-F (Sysjoint) | eigen | tekst-shell variant | F, F V2, F V3 |

Je LiteVNA 64 en 67 zitten dus in dezelfde protocolfamilie; je NanoVNA-H1 (als dat een
"v1"-type is) praat een compleet ander, tekstgebaseerd protocol. Dat is later goed als tweede
backend te doen, maar voor de POC richten we ons op het binaire protocol.

## 3. Het protocol in detail

### 3.1 Opcodes (host → device)

| Byte | Naam | Formaat | Antwoord |
|---|---|---|---|
| `00` | NOP | – | geen |
| `0D` | INDICATE | – | ASCII `'2'` (`0x32`) |
| `10` | READ | `10 AA` | 1 byte |
| `11` | READ2 | `11 AA` | 2 bytes |
| `12` | READ4 | `12 AA` | 4 bytes |
| `18` | READFIFO | `18 AA NN` | NN waarden (elk 32 bytes bij 0x30) |
| `20` | WRITE | `20 AA XX` | geen |
| `21` | WRITE2 | `21 AA X0 X1` | geen |
| `22` | WRITE4 | `22 AA X0..X3` | geen |
| `23` | WRITE8 | `23 AA X0..X7` (10 bytes) | geen |
| `28` | WRITEFIFO | `28 AA NN data…` | geen |

Truc om de parser van het apparaat in een bekende staat te krijgen: stuur **8 × `0x00`**
(NOP's). Een half ontvangen commando wordt zo altijd afgerond. Alle bestaande software doet dit
bij het openen van de poort.

### 3.2 Registers (normale modus)

| Adres | Naam | Type | Betekenis |
|---|---|---|---|
| `00..07` | sweepStartHz | u64 | startfrequentie in Hz |
| `10..17` | sweepStepHz | u64 | stapgrootte in Hz |
| `20..21` | sweepPoints | u16 | aantal punten |
| `22..23` | valuesPerFrequency | u16 | metingen per punt (normaal 1) |
| `26` | rawSamplesMode | u8 | `0` USB-modus, `1` raw samples (verlaat protocol!), `2` terug naar normale (scherm)modus, `3` gecalibreerde data (LiteVNA) |
| `30` | valuesFIFO | FIFO | meetdata; WRITE van willekeurige waarde **leegt** de FIFO |
| `40` | Average | u8 | FFT-vermenigvuldiger (1..80), beïnvloedt IFBW |
| `41` | LowFrequencyPower | u8 | MS5351 (LF-bron), 1..3, default 1 |
| `42` | HighFrequencyPower | u8 | MAX2871 (HF-bron), 1..3, default 3 |
| `44` | ChannelSelect | u8 | `1` alleen S11, `2` alleen S21, `0` beide (sneller met één kanaal) |
| `58` | UnixTime | u32 | klok zetten |
| `5C` | batteryValue | u16 | accuspanning in mV (LiteVNA) |
| `EE` | screenshot | u8 | WRITE → device stuurt header (w u16, h u16, bpp u8) + RGB565-pixels |
| `F0` | deviceVariant | u8 | altijd `0x02` |
| `F1` | protocolVersion | u8 | altijd `0x01` |
| `F2` | hardwareRevision | u8 | |
| `F3` | firmwareMajor | u8 | `0xFF` = DFU/bootloader-modus |
| `F4` | firmwareMinor | u8 | |

Zodra je naar `sweepStartHz`/`sweepStepHz`/`sweepPoints` schrijft, verlaat het apparaat zijn
eigen UI en gaat het in "USB data mode". Met `WRITE 26 02` geef je het scherm weer terug.
Je kunt de door de gebruiker ingestelde sweep **niet** teruglezen.

### 3.3 FIFO-record (32 bytes, little-endian)

| Offset | Veld | Type |
|---|---|---|
| `00` | fwd0Re | i32 |
| `04` | fwd0Im | i32 |
| `08` | rev0Re | i32 |
| `0C` | rev0Im | i32 |
| `10` | rev1Re | i32 |
| `14` | rev1Im | i32 |
| `18` | freqIndex | u16 (0..sweepPoints-1) |
| `1A..1E` | reserved | |
| `1F` | checksum | u8 (ongedocumenteerd; NanoVNA-App controleert hem) |

Checksum zoals NanoVNA-App (DiSlord) hem berekent over de eerste 31 bytes:

```
c = 0x46
for b in data[0..31]: c = (c ^ ((c << 1) | 1)) ^ b      (alles u8)
```

Of de LiteVNA 67 dit byte echt vult moeten we meten; de POC rapporteert alleen hoeveel records
kloppen en gooit niets weg.

### 3.4 Een sweep doen (zoals NanoVNA-Saver en NanoVNA-App het doen)

```
00×8                        reset parser
20 26 00                    USB-modus (of 03 voor gecalibreerde data)
23 00 <start u64>           sweepStartHz
23 10 <step u64>            sweepStepHz
21 20 <points u16>          sweepPoints
21 22 01 00                 valuesPerFrequency = 1
20 30 00                    FIFO leegmaken (verse sweep)
18 30 FF                    255 punten lezen  (herhalen tot alle punten binnen zijn)
18 30 NN                    rest
```

De LiteVNA accepteert ook `18 30 00`: NN = 0 betekent "stuur **alle** punten van de sweep"
(LiteVNA-uitbreiding, de V2 kan dat niet). De sweep loopt altijd door; de FIFO vult zich
continu, dus voor on-demand metingen eerst legen.

Timing: NanoVNA-Saver rekent 35 ms per punt als timeout; de LiteVNA haalt >1600 pt/s dus dat
is heel ruim. Een goede strategie is: een deadline per chunk en lezen tot het aantal bytes
compleet is.

### 3.5 DFU-modus

Zelfde opcodes, andere registers (`E0` flashWriteStart, `E4` flashFIFO, `EF` reboot met `0x5E`).
`F3 == 0xFF` herkent de bootloader. Voor de POC laten we flashen bewust buiten scope: een fout
hier kan het apparaat onbruikbaar maken. Mocht het toch nodig zijn: in `assets/` liggen de
officiële firmware-images v1.4.10 (build 13 juli 2026) voor de 62, 64 en 67
(`liteVNA67-260713.bin` enz.), zie `02-assets-inventaris.md`. Recovery via DFU is dus mogelijk.

## 4. Wat is bekend over de LiteVNA 67 specifiek

- 50 kHz – 6,3 GHz, N-connectors, 7" 1024×600 capacitief touchscreen, 7500/7800 mAh accu,
  ingebouwde bron-attenuator, dynamisch bereik >100 dB tot 3 GHz.
- De officiële "Product Manual, based on v1.4 firmware" dekt de 62/64/67 samen en heeft één
  protocol-appendix, dus **de 67 praat hetzelfde protocol**.
- Screenshot wordt 1024×600×16 bit = 1,2 MB over USB (CDC full-speed, ~1 MB/s): reken op een
  paar seconden.
- De actuele officiële firmware is **v1.4.10** (image in `assets/liteVNA67-260713.bin`). De image
  bevat de USB-id `04B4:0008`, wat bevestigt waar onze poortdetectie op filtert. Strings in de image
  wijzen op SD-kaartondersteuning, een USB mass-storage modus ("LiteVNA MSC"), FPU-detectie en
  "SERIAL SPEED"/"SET TIME"-menu's.
- WHY VNA (SM6WHY, Windows) noemt de LiteVNA 67 expliciet en noemt een experimentele
  CW-generator die een onofficieel gewijzigde firmware 1.4.10 vereist. Daar blijven we weg van.
- NanoVNA-Saver herkent alleen "LiteVNA64" (verwacht HW 2.2 en FW 2.2 uit de registers); een
  67 valt bij hen terug op generiek "V2". Welke waarden onze 67 in `F2..F4` rapporteert is
  dus een van de eerste dingen om te meten.

## 5. Wat ik op jouw Mac heb gecontroleerd

- Rust toolchain aanwezig: cargo 1.96, arm64.
- Op dit moment is er **geen LiteVNA aangesloten**: de enige `usbmodem` is de "LG Monitor
  Controls" van je monitor. Zodra de 67 via USB-C aan hangt verschijnt er een tweede
  `/dev/cu.usbmodem…` met vendor-id `0x04B4`. De POC heeft een `list`-commando dat dit toont.

## 6. Keuzes voor de Rust-implementatie

- **Seriële laag:** crate `serialport` 4.x (cross-platform: macOS, Linux, Windows). De poort
  moet in raw-modus (geen echo, geen line discipline); `serialport` doet dat standaard.
  Baudrate is irrelevant bij USB-CDC maar moet gezet worden (115200).
- **Architectuur:** een pure library-crate `litevna` (protocol-encoding, parsing, device-API,
  geen UI) en een CLI `vna-cli` voor de POC. De GUI (egui/eframe of Tauri) komt later bovenop
  dezelfde library. De protocol-laag is zo getest te maken zonder hardware.
- **Calibratie:** niet in de POC. Later: SOL(T)-calibratie met 12-term of 3-term errormodel in de
  library, opslaan als Touchstone/JSON.
- **Export:** Touchstone `.s2p` (S11, S21; S12/S22 op nul) en CSV, zodat je de data meteen in
  andere tools kunt bekijken.

## 7. Gemeten op de LiteVNA 67 (3 november 2026)

De POC is dezelfde avond tegen de echte LiteVNA 67 gedraaid (firmware v1.4.10, poort 1 en 2 open,
geen DUT). Antwoorden op de open vragen:

| Vraag | Antwoord |
|---|---|
| USB-identificatie | `/dev/cu.usbmodemv1_01`, `04B4:0008`, fabrikant "ZeenKo.tech", product "LiteVNA6", serienummer "v1.0". De productstring onderscheidt de 67 dus **niet** van een 64. |
| Registers `F0..F4` | variant `0x02`, protocol `0x01`, HW-rev `0x02`, FW `2.2`. Precies de waarden waarop NanoVNA-Saver "LiteVNA64" herkent. Via de registers is een 67 niet van een 64 te onderscheiden; de echte firmwareversie (1.4.10) staat er niet in. |
| Accu (`0x5C`) | werkt: 3,803 V. |
| Checksum byte `0x1F` | **ja**, de 67 vult hem; het NanoVNA-App-algoritme klopt op 100 % van de records (ruim 20.000 records getest, 0 fouten). Bytes `1A..1E` zijn altijd 0. |
| Referentiekanaal | `fwd0` is altijd exact `(2^30, 0)`: de firmware normaliseert al op de referentie. `rev0 / 2^30` is dus direct S11. Delen door `fwd0` blijft correct en kost niets. |
| `READFIFO 0x30 0` ("alles") | werkt, maar is niet sneller dan chunks van 255: de sweep zelf is de bottleneck. |
| Snelheid | 1001 punten, 50 kHz–6,3 GHz: 2,8 s met de scherm-instelling (BW 1,13 kHz); 1,6 s met `average = 1`; 0,9–1,1 s met alleen S11. 4001 punten in 6,4 s. 101 punten HF in 0,3 s. |
| `rawSamplesMode = 3` (gecalibreerd) | wordt geaccepteerd en geeft andere data dan modus 0 (fase van de open poort draait anders), dus er wordt een correctie toegepast. Wat precies (welke on-device calibratie actief is) is nog niet gecontroleerd. |
| `ChannelSelect` (`0x44`) | S11-only is ~40 % sneller; het S21-veld bevat dan nullen. |
| Screenshot (`0xEE`) | header `00 04 58 02 10` → 1024×600, 16 bpp; 1,2 MB in 1,3 s. Pixels zijn RGB565 **big-endian** (zoals NanoVNA-Saver aanneemt). Zie `docs/images/litevna67-screenshot-usb.png`. |

### Valkuilen die we tegenkwamen

1. **32 verouderde records na het legen van de FIFO.** Bij snelle sweeps (S11-only, of weinig
   punten) komen na `WRITE 0x30` nog precies 32 records van de vorige cyclus binnen; wie dan exact
   N records leest mist de laatste 32 indexen. Oplossing in de library: lezen tot elke `freqIndex`
   gevuld is, duplicaten tellen. Dit is waarschijnlijk een 1 kB USB-verzendbuffer (32 × 32 bytes).
2. **`Average` (0x40) en `ChannelSelect` (0x44) zijn persistent** in het apparaat, ook nadat je het
   scherm hebt teruggegeven. De CLI leest ze vooraf en schrijft ze na de sweep terug, maar voor
   `0x40` helpt dat niet echt: het register leest altijd `1`, terwijl het scherm vóór onze tests
   BW 1,13 kHz toonde en erna 2,25 kHz. De uitgelezen waarde weerspiegelt de UI-instelling dus
   niet (vergelijk de waarschuwing in de handleiding over de sweep-registers). Gevolg: `--average`
   gebruiken verandert de bandbreedte-instelling op het apparaat; via het menu zet je die terug.
   `0x44` leest wél betrouwbaar (0 = beide kanalen).
3. zsh splitst variabelen niet op spaties; dat kostte even een vals alarm in de testlus.

### Nog open

- Wat `rawSamplesMode = 3` precies toepast en hoe het zich verhoudt tot de calibratie op het scherm.
- Of de 32-record-buffer ook op de LiteVNA 64 bestaat (vermoedelijk wel).
- Gedrag bij `valuesPerFrequency > 1` (meerdere metingen per punt).

## 8. Bronnen

- Lokaal in `assets/`: officiële User Guide (identiek aan de nanovna.com-PDF), firmware-images v1.4.10,
  NanoVNA-App/QT/Saver binaries. Inventaris: `02-assets-inventaris.md`.
- Officiële LiteVNA Product Manual (v1.4 firmware, dekt 62/64/67), appendix "USB data interface":
  https://storage.eleshop.eu/files/p1207670-Sf10622ab93fb487388b5b395dce10d5cl.pdf
- Oudere LiteVNA User Guide (v1.4): https://nanovna.com/wp-content/uploads/2021/11/LiteVNA_User-Guide.pdf
- NanoVNA V2 user manual, protocolsectie: https://nanorfe.com/nanovna-v2-user-manual.html
- NanoVNA-Saver drivers (Python, GPL-3): `NanoVNA_V2.py`, `LiteVNA64.py`, `Hardware.py` in
  https://github.com/NanoVNA-Saver/nanovna-saver
- NanoVNA-App (DiSlord fork, C++): `NanoVNA_v2_comms.{h,cpp}` in https://github.com/DiSlord/NanoVNA-App
- NanoVNA-QT / libxavna (OwOComm, officiële V2-protocollib, C++): https://github.com/nanovna-v2/NanoVNA-QT
- Eleshop productpagina LiteVNA 67: https://eleshop.eu/litevna-67-vector-network-analyser.html
- WHY VNA (SM6WHY): https://sm6why.n.nu/why-vna
- EEVblog-draad "NanoVNA Custom Software" (achtergrond, V2 register map):
  https://www.eevblog.com/forum/rf-microwave/nanovna-custom-software/
