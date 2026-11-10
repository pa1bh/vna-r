# Analyse en aanbevelingen, november 2026

> **Vervolg.** Dit rapport beschrijft de toestand van 0.2.5. Wat er daarna mee gedaan is
> staat in de werklijst onderaan [docs/05-roadmap.md](05-roadmap.md); hoe je het
> controleert in [docs/08-qc-procedure.md](08-qc-procedure.md). Eén advies hieronder
> bleek onjuist: de handleiding lokaal openen (F7, besluit 6) lost "een correctie zonder
> release" niet op, zie de open vragen in docs/08.

Stand: versie 0.2.5, commit 94fe525, 10 november 2026. Een brede doorlichting van het
project op UX/UI, bugs, functionaliteit, nieuwe features, veiligheid, duurzaamheid van de
code en overbodige ballast. Bedoeld als grondstof voor een werklijst; de nummers (U1, B3,
C2 …) zijn er om naar te verwijzen.

Werkwijze: drie onafhankelijke leesrondes (GUI-crate; library-crate; CLI, repo, docs en
CI), `cargo test` en `cargo clippy` op de workspace, en daarna eigen verificatie van elke
bevinding die hieronder **bevestigd** heet. **Plausibel** betekent: uit de code afgeleid,
maar niet met hardware of een testcase aangetoond. Regelnummers horen bij commit 94fe525.

Cijfers die het beeld bepalen:

| | |
|---|---|
| Rust-code | 9 161 regels in drie crates; `app.rs` alleen al 2 684 |
| Tests | 39, allemaal in `litevna`; nul in `vna-cli` en `vna-gui` |
| Clippy | 17 stijlmeldingen, geen fouten |
| Git | 50 commits, één auteur, drie dagen (3–5 nov), zes tags op één dag |
| Afhankelijkheden | 479 packages in de lockfile, alle directe deps actueel |

---

## 1. Samenvatting: de tien dingen die er het meest toe doen

1. **Calibratie wordt in re/im geïnterpoleerd met een te ruime fasegrens** (B1). Bij een
   grof calibratieraster komt |Γ| na correctie te hoog uit, tot 3 dB bij de toegestane
   90° stap. De TDR-module doet het al goed (polair); dezelfde aanpak in `calibration.rs`
   lost het op. Dit raakt de kern van wat het programma belooft.
2. **Standaarden worden gemeten op het getypte, niet op het toegepaste raster** (U1), en
   de bevestiging "measured open" wordt direct weer gewist (B4). Samen is dit de
   makkelijkste manier om ongemerkt een calibratie op het verkeerde raster te maken.
3. **TDR-bereik is een factor 2 te groot** (B2): `max_distance_m()` geeft 1/df waar de
   data tot 1/(2·df) lopen. GUI-label "alias-free range" en de as-limiet kloppen niet.
4. **Een mislukte schermafdruk of "set average" stopt een lopende sweep** (B5).
5. **Markers zijn puntindices** en springen bij een rasterwijziging naar een andere
   frequentie (B6). Frequentie opslaan en per sweep opnieuw opzoeken.
6. **Pijltjestoetsen verplaatsen de marker ook tijdens typen** in Start/Stop of een
   referentienaam (B7).
7. **Geen tests voor alle I/O-paden** (D1): `LiteVna::sweep`, `scan_bin`, FIFO-afhandeling,
   screenshots. Eén injecteerbare nep-seriële-poort maakt dat allemaal testbaar.
8. **`app.rs` splitsen en UI-vrije logica naar de library** (D2, D3): resampling,
   middeling, segmentplanning en per-segment-correctie bestaan nu dubbel (GUI en CLI).
9. **Bewaarde instellingen hebben geen versie en gebruiken vaste arrays** (D4): één
   extra marker en alle instellingen resetten stil.
10. **Drie bestanden die niet in een MIT-repo horen** (L1): drie firmware-images en drie
    PDF's van de fabrikant staan getrackt in `assets/`.

---

## 2. UX/UI

Wat goed werkt en zo moet blijven: apparaat-I/O op een eigen thread met repaint op
events; poorten die elke 2 s opnieuw gescand worden; de CAL/CAL*/RAW/GATED/FILE-tag;
plausibiliteitswaarschuwingen per standaard; "Set sweep for TDR"; de gate die in
schermcoördinaten getekend wordt; segmentoverlap met een leesbare melding; Copy
diagnostics; Help-links naar de eigen versie.

Wrijvingspunten, ruwweg op volgorde van belang:

- **U1 — Standaarden op het getypte raster.** `standard_buttons` roept `current_config()`
  aan ([app.rs:1735](../crates/vna-gui/src/app.rs#L1735), [app.rs:587](../crates/vna-gui/src/app.rs#L587)),
  dat Start/Stop/Points live leest. Wie Stop wijzigt, "Apply sweep" vergeet en op
  open/short/load klikt, calibreert op raster B terwijl de weergave op raster A sweept; de
  calibratie wordt dan stil geïnterpoleerd. Gebruik het primaire segment uit `last_plan`,
  of waarschuw zodra het getypte raster afwijkt van het toegepaste. Bevestigd.
- **U2 — Bevestigingen verdwijnen.** "measured open", "loaded" en "kit updated … save to
  keep it" worden in `cal.message` gezet en direct daarna door `calibration_for` gewist
  ([app.rs:187](../crates/vna-gui/src/app.rs#L187)). Met apply aan (standaard) ziet
  niemand ze ooit. Scheid de solve-fout van de statusmelding. Bevestigd, zie B4.
- **U3 — Segmentwijzigingen gelden pas na Apply sweep**, zonder dat de UI dat zegt: het
  rijtje toont het nieuwe cal-bestand terwijl `sent_plan_cals` nog het oude gebruikt
  ([app.rs:641](../crates/vna-gui/src/app.rs#L641)). Bevestigd.
- **U4 — Stop en Disconnect reageren pas na een hele sweep.** De worker kijkt alleen
  tussen complete multi-segment-sweeps naar commando's ([worker.rs:85](../crates/vna-gui/src/worker.rs#L85)).
  Op een NanoVNA-F duurt 3201 punten bijna een halve minuut waarin Stop dood lijkt. De
  puntenkeuzes negeren `Limits.max_points_per_scan` en de snelheid van het apparaat.
  Minimaal een "sweeping…"-indicator; beter een annuleerpunt per segment of per
  `scan_bin`-stuk (zie F2).
- **U5 — "Set sweep for TDR" is LiteVNA-logica**: 6,3 GHz hard gecodeerd
  ([app.rs:1434](../crates/vna-gui/src/app.rs#L1434)); op een NanoVNA-F levert het direct
  de oranje buiten-bereik-waarschuwing op. Gebruik `device_limits().max_hz`. Bevestigd.
- **U6 — TDR-fouten gebruiken het globale foutvak en de placeholder liegt.** Bij een
  log-gespreide .s1p of een te smalle sweep schrijft `tdr_result` elk frame `self.error`
  ([app.rs:2244](../crates/vna-gui/src/app.rs#L2244)); het kruisje werkt niet en het
  middenvlak zegt "Connect and press Run" terwijl er data zijn. De gate heeft wel een
  eigen `gate_error`: twee conventies voor buurfuncties.
- **U7 — Clear wist alleen de paneelcalibratie**; segmentbestanden blijven gelden, dus de
  tag gaat RAW en bij de volgende sweep weer CAL*. Label het "Clear panel calibration" of
  gooi ook `seg_cals` leeg.
- **U8 — Average toont de bewaarde waarde, niet het register** van het apparaat na een
  reconnect, tot je "set" drukt. Het oogt als toestand maar is een verzoek.
- **U9 — Enter in Start/Stop doet niets**; validatie pas bij Apply; "Apply sweep" in
  rust doet één sweep, wat niet voor de hand ligt.
- **U10 — Verouderde teksten**: "Hold (top bar)" terwijl Hold in de markerbalk zit
  ([app.rs:1684](../crates/vna-gui/src/app.rs#L1684)); "step 8 will read them back" is
  roadmap-jargon in gebruikerstekst ([app.rs:1719](../crates/vna-gui/src/app.rs#L1719));
  standaard-bestandsnaam `litevna.cal.json` voor NanoVNA-bezitters; `sweep.s2p` zonder
  datum.
- **U11 — Kleine dingen**: Copy diagnostics meldt onder het kopje Files; TDR-impedantie
  staat in de S21-kleur; Smith-markers worden getekend als S11 verborgen is; batterij
  wordt één keer gelezen bij verbinden; één lange foutregel in de topbalk wordt afgekapt.

CLI-kant:

- **U12 — `vna-cli sweep` print elke keer een debugregel** ("first raw record: fwd0=…")
  naar stderr ([main.rs:748](../crates/vna-cli/src/main.rs#L748)). Hoort achter `--trace`.
- **U13 — Stil genegeerde vlaggen**: `--segment` overschrijft `--start/--stop/--points/--cal`
  zonder `conflicts_with`; `tdr --from` negeert raster- en meetargumenten; `--fifo-all`
  en `--keep-usb-mode` bestaan maar alleen op `sweep`; `--channel both` op een NanoVNA-F
  geeft een fout terwijl het de standaard is.
- **U14 — `cal <what>` is een vrije string** in plaats van subcommando's: geen hulp per
  actie, `--note` wordt stil weggegooid bij show/kit/fit/check/clear, en `cal clear`
  verwijdert het bestand zonder bevestiging.

---

## 3. Bugs

| # | Wat | Waar | Status |
|---|-----|------|--------|
| B1 | Calibratie-interpolatie in re/im krimpt alle standaarden met cos(θ/2); grens π/2 laat tot −3 dB toe; through via kabel ongecontroleerd | [calibration.rs:410](../crates/litevna/src/calibration.rs#L410), [:510](../crates/litevna/src/calibration.rs#L510) | bevestigd (wiskunde); omvang in de praktijk afhankelijk van rasterstap |
| B2 | `max_time_s = 1/df` terwijl de arrays tot 1/(2·df) lopen; GUI-label en as-limiet, CLI `limit_m` nemen dat over | [timedomain.rs:245](../crates/litevna/src/timedomain.rs#L245), [:256](../crates/litevna/src/timedomain.rs#L256) | bevestigd |
| B3 | Mislukte `NanoVnaShell::open` stuurt via Drop nog `resume` en wacht nog eens 3 s | [shell.rs:205](../crates/litevna/src/shell.rs#L205), [:500](../crates/litevna/src/shell.rs#L500) | bevestigd |
| B4 | "measured …", "loaded", "save to keep it" worden direct overschreven | [app.rs:187](../crates/vna-gui/src/app.rs#L187), [:965](../crates/vna-gui/src/app.rs#L965) | bevestigd |
| B5 | `fail()` zet altijd `continuous = None`; mislukte screenshot of SetAverage stopt de sweep | [worker.rs:115](../crates/vna-gui/src/worker.rs#L115) | bevestigd |
| B6 | Markers zijn indices; na rasterwijziging andere frequentie | [app.rs:258](../crates/vna-gui/src/app.rs#L258) | bevestigd |
| B7 | Pijltjestoetsen niet gefilterd op tekstfocus (`key_pressed` leest ruwe events) | [app.rs:2010](../crates/vna-gui/src/app.rs#L2010) | bevestigd in egui-bron |
| B8 | s2p-commentaar zegt "uncalibrated raw" bij `--segment …@cal.json`, en altijd "LiteVNA sweep" | [main.rs:757](../crates/vna-cli/src/main.rs#L757) | bevestigd |
| B9 | `tdr --csv -` maakt een bestand met de naam `-`; handleiding belooft stdout | [main.rs:852](../crates/vna-cli/src/main.rs#L852) | bevestigd |
| B10 | Drie encoderingen van "geen ruwe records" in `SweepResult.points`; Touchstone-sweep wordt als standaard geweigerd ("n points missing") | [touchstone.rs:153](../crates/litevna/src/touchstone.rs#L153), [shell.rs:443](../crates/litevna/src/shell.rs#L443), [device.rs:113](../crates/litevna/src/device.rs#L113) | bevestigd |
| B11 | Screenshot-header kan tot 17 GB laten alloceren (u16×u16×4) vóór er data gelezen zijn | [device.rs:574](../crates/litevna/src/device.rs#L574) | bevestigd |
| B12 | Export-commentaar en referentienaam gebruiken `is_calibrated()` van de vorige apparaatsweep bij een bestandstrace | [app.rs:903](../crates/vna-gui/src/app.rs#L903), [:1966](../crates/vna-gui/src/app.rs#L1966) | bevestigd |
| B13 | Standaard-sweeps gaan buiten de software-middeling om | [app.rs:776](../crates/vna-gui/src/app.rs#L776) | bevestigd (ontwerpgat) |
| B14 | `tdr_vf`, `tdr_max_m`, gate-grenzen worden ongeclampt uit de instellingen gelezen; 0 of NaN → deling door nul, elk frame een fout | [app.rs:417](../crates/vna-gui/src/app.rs#L417) | bevestigd, vereist kapot settings-bestand |
| B15 | `SweepConfig::stop_hz()` met `points == 0` → underflow; `check_grid` trekt u64's ongecontroleerd af | [device.rs:89](../crates/litevna/src/device.rs#L89), [timedomain.rs:277](../crates/litevna/src/timedomain.rs#L277) | bevestigd op handgebouwde input |
| B16 | `check_lines` hangt bij `band_hz < 0` | [calibration.rs:848](../crates/litevna/src/calibration.rs#L848) | bevestigd, alleen via library-API |
| B17 | Cache-thrash en misleidende rasterwaarschuwing bij ≥2 paneelgecalibreerde segmenten | [app.rs:154](../crates/vna-gui/src/app.rs#L154), [:932](../crates/vna-gui/src/app.rs#L932) | bevestigd |
| B18 | FIFO-timeouts (20 ms/punt, min 3 s) schalen niet met average 1..80 of shell-bandbreedte | [device.rs:228](../crates/litevna/src/device.rs#L228), [shell.rs:51](../crates/litevna/src/shell.rs#L51) | plausibel, meten |
| B19 | Na een timeout midden in een chunk geen resync (`configured` blijft staan) | [device.rs:497](../crates/litevna/src/device.rs#L497) | plausibel |
| B20 | Stale records na een configuratiewijziging kunnen als vers geteld worden | [device.rs:520](../crates/litevna/src/device.rs#L520) | plausibel, meten |
| B21 | NaN/inf kan via Touchstone, `fwd0 == 0` of `fit_open` in een `CalibrationSet` komen en maakt de JSON daarna onleesbaar | [calibration.rs:287](../crates/litevna/src/calibration.rs#L287), [:636](../crates/litevna/src/calibration.rs#L636) | plausibel |
| B22 | Touchstone: gelijke opeenvolgende frequenties worden geaccepteerd; ruisparameterblok geeft verwarrende fout | [touchstone.rs:143](../crates/litevna/src/touchstone.rs#L143) | plausibel |
| B23 | Eén sweep lang verkeerde segment-calibraties bij Apply tijdens lopen met gelijk aantal segmenten | [app.rs:750](../crates/vna-gui/src/app.rs#L750) | plausibel, zelfherstellend |
| B24 | Verbinden is niet te annuleren; onbekende poort probeert beide protocollen | [vna.rs:65](../crates/litevna/src/vna.rs#L65) | plausibel |
| B25 | `build.rs` kijkt alleen naar `.git/HEAD`; een nieuwe commit op dezelfde branch ververst de git-revisie niet | [build.rs:9](../crates/vna-cli/build.rs#L9) | bevestigd |

Gecontroleerd en in orde: FIFO-byteoffsets en checksum, `scan_bin`-framing, `recv_exact`,
de SOLT-oplosser en enhanced-response S21, TDR-binplaatsing, window-normalisatie, DC-term
uit causaliteit, gateflanken, teken en unwrap van groepsvertraging, `parse_hz`, geen
`unsafe`, geen shell-outs buiten `build.rs`, geen panics op gebruikersinvoer in de GUI
gevonden.

Over B1 wat nuance: de fout is cos(θ/2) per rasterstap van de *ruwe* standaard. Bij
1001 punten over 6,3 GHz is de stap een paar graden en verwaarloosbaar; bij 101 punten
over hetzelfde bereik zo'n 20° en dus ~0,15 dB; en een through door een meter kabel
draait veel sneller dan de open en wordt helemaal niet gecontroleerd. Polair interpoleren
kost weinig en maakt de grens overbodig.

---

## 4. Functionaliteit: gaten en inconsistenties

- **F1 — CLI mist wat de GUI heeft**: geen software-middeling (`Sweep avg`), geen
  afgeleide grootheden (R, X, |Z|, groepsvertraging) in de CSV, geen `sweep --from
  file.s2p` om offline te gaten of te converteren, geen `--json`. De roadmap noemt
  `--json` al als klein los punt.
- **F2 — Geen annulering of voortgang in `sweep()`** in de library; dat is de wortel van
  U4 en B24. Een `sweep_with(cfg, |done, total| ControlFlow)` is een niet-brekende
  toevoeging; beide lussen hebben al natuurlijke checkpoints (per 255-recordchunk, per
  `scan_bin`-stuk).
- **F3 — Averaging-register `valuesPerFrequency` wordt geschreven maar niet benut**: de
  sweep houdt het eerste record per index en telt de rest als duplicaat. Middelen op de
  host zou ruisreductie geven zonder het hardnekkige register 0x40.
- **F4 — Referenties overleven een herstart niet.** Bewust zo (roadmap 7), maar ze zijn
  gewoon `SweepResult`s; als .s2p naast de instellingen wegschrijven is een avond werk.
- **F5 — `Limits` neemt voor elk binair apparaat 50 kHz–6,3 GHz aan**; een NanoVNA V2
  Plus4 stopt bij 4,4 GHz. Minimaal documenteren als "aangenomen LiteVNA".
- **F6 — Touchstone**: geen `.s1p`-export (altijd 9 kolommen), geen v2-import.
- **F7 — Help-links zitten vast aan de release-tag.** Elke handleidingcorrectie die
  vanuit de GUI zichtbaar moet zijn vereist een release (zie ook de releasedruk onder
  §9). Alternatief: de handleiding zit al in het release-archief; open die lokaal, met de
  GitHub-link als terugval.

---

## 5. Nieuwe, zinvolle features

Op volgorde van wat ik zelf eerst zou doen. S = een avond, M = een paar sessies, L =
meerdere dagen inclusief meten.

| # | Feature | Grootte | Waarom |
|---|---------|---------|--------|
| N1 | Frequentie-verankerde markers die een rasterwijziging overleven | S | lost B6 op, `nearest_index` bestaat al |
| N2 | Annuleren en voortgang bij sweeps, Stop dat tussen segmenten/chunks ingrijpt | M | NanoVNA-F-gebruikers, U4 |
| N3 | Standaard-sweeps door dezelfde middeling als de weergave | S | B13; calibratie is waar ruis het meest pijn doet |
| N4 | `--json` voor `info`, `list`, `sweep`, `tdr`, `cal show` | S | scripting, roadmap-restpunt |
| N5 | Poortverlenging / elektrische vertraging op de host (referentievlak verschuiven) | S | een paar regels in `Calibration::correct`, standaardfunctie op elke VNA |
| N6 | Host-middeling via `valuesPerFrequency` | S/M | F3 |
| N7 | Sneltoetsen: spatie Run/Stop, S Single, 1–4 marker, H Hold, G gate | S | |
| N8 | Auto-connect bij start als de onthouden poort er is en op een VNA lijkt | S | |
| N9 | Batterij elke minuut verversen in rust | S | |
| N10 | Smith-chart hover-uitlezing (f, Z) en enkele frequentielabels op de trace | M | |
| N11 | TDR-marker met pijltjes nudgen, zoals in het frequentiebeeld | S | |
| N12 | Referenties bewaren met de sessie | M | F4 |
| N13 | Tekst-`scan` als terugval voor NanoVNA-H/H4-firmware zonder `scan_bin` | M | de H1 uit roadmap 10; alleen een regelparser ontbreekt |
| N14 | Kit-bestanden lezen (NanoVNA-Saver/Keysight-stijl) en load serie-L | S/M | docs/03 §9 noemt het als open |
| N15 | `.s1p`-export, Touchstone v2 import | S | |
| N16 | Afgeleide kolommen in CSV, `--columns` | S | F1 |
| N17 | `cal` als echte subcommando's + `clap_complete` | M | U14 |
| N18 | Volledig 2-poorts (12-term) via handmatig omdraaien van de DUT | L | haalt de poort-2-match-aanname weg die docs/03 §9 op ~1 % schat |
| N19 | Apparaatbewuste standaardwaarden: segment-stop, TDR-sweep, puntenkeuze uit `Limits` | S | U5 |

Wat ik *niet* zou doen: een eigen rendering van de apparaatmenu's, Bluetooth, of een
webversie. Het programma is sterk omdat het klein en controleerbaar is.

---

## 6. Veiligheid

Lage prioriteit, zoals afgesproken; voor de volledigheid:

- Geen `unsafe`, geen `std::process::Command` buiten `build.rs` (`git describe`).
  Help-URL's zijn constanten; de handleiding-URL bevat alleen de versie.
- Parsers zijn robuust: Touchstone weigert v2, niet-S, niet-eindige of absurde
  frequenties, niet-oplopende rasters; calibratie-JSON wordt gevalideerd; TDR capt de
  FFT op 4 Mi punten. Touchstone leest wel het hele bestand in geheugen (geen limiet).
- **V1 — De udev-regel leunt op MODE 0666** (wereld-schrijfbaar). `TAG+="uaccess"` in
  een 99-bestand komt ná `73-seat-late.rules` en doet daardoor niets. Hernoem naar
  `70-litevna.rules` en laat 0666 weg, of gebruik `MODE="0660", GROUP="dialout"`.
- **V2 — B11**: onbegrensde allocatie uit een screenshot-header van een kapot of vreemd
  apparaat; bereikbaar via de protocolprobe op een onbekende poort. Cap op bijvoorbeeld
  4096×4096 en weiger `bpp != 16` (de conversie neemt dat toch aan).
- `--port <willekeurig pad>` zonder herkenbare USB-id stuurt LiteVNA- en daarna
  shell-probebytes naar het apparaat. Onschadelijk voor een VNA, verwarrend voor een
  Arduino op dezelfde poort.
- Copy diagnostics bevat poortpad en het pad van het calibratiebestand (dus de
  home-map); gebruikers plakken dat in openbare issues. Eventueel de home-map vervangen
  door `~`.
- Bewaarde instellingen (`~/Library/Application Support/vna-r/app.ron` op macOS) bevatten
  geen geheimen.

---

## 7. Duurzaamheid van de code

- **D1 — I/O is ontestbaar.** `LiteVna` en `NanoVnaShell` zijn alleen vanaf een pad te
  bouwen. `serialport::SerialPort` is een trait; een `from_port(Box<dyn SerialPort>)` plus
  een gescripte nep-poort (`VecDeque<u8>` met geprogrammeerde antwoorden) maakt de
  sweep-lus, stale-record-telling, checksumfouten, `scan_bin`-foutpaden, echo-stripping
  en screenshots testbaar zonder hardware. Dit is de grootste enkele investering in
  duurzaamheid.
- **D2 — `app.rs` splitsen** in ongeveer zeven modules met elk hun eigen toestand:
  `settings.rs`, `calibration_ui.rs`, `segments.rs`, `plots.rs`, `tdr_ui.rs`, `files.rs`,
  `diagnostics.rs`. Twee `impl CalState`-blokken samenvoegen.
- **D3 — UI-vrije logica uit de GUI naar `litevna`**, waar de CLI het deels al dubbel
  heeft: `resample_complex` ([app.rs:2616](../crates/vna-gui/src/app.rs#L2616)) is bijna
  regel voor regel `calibration::interpolate_complex`; `push_average`; `segment_plan`
  met overlapcontrole (de CLI heeft dezelfde melding op [main.rs:554](../crates/vna-cli/src/main.rs#L554));
  "breedste segment voor TDR"; per-segment-correctie met eigen calibratiebestand;
  `nearest_index`; de "benodigde span"-regel; gate-spec en meter→seconde (twee keer).
- **D4 — Instellingen versioneren.** `marker_freqs: [Option<u64>; 4]` en
  `plot_kinds: [String; 3]` zijn vaste arrays; `#[serde(default)]` dekt ontbrekende
  velden, niet een andere arraylengte. Eén marker erbij en `get_value` geeft None: alles
  reset stil. Voeg `version: u32` toe, gebruik `Vec`s met clamping, en clamp de floats
  (B14). Hetzelfde geldt voor het calibratie-JSON: de CHANGELOG noemt het formaat
  semver-relevant, maar het heeft geen formaatveld.
- **D5 — Eén meldingsmodel.** `self.error`, `cal.message`, `gate_error`, `file_message`,
  `sweep_error` hebben elk een eigen levensduur en plek. Een `Notice { level, where, text }`
  met een vaste plaats per paneel maakt U2, U6 en U11 in één keer consistent.
- **D6 — Gedeelde seriële laag.** Trace-vlag, `*_on_drop`, read-exact-met-deadline en
  discard bestaan twee keer met verschillende semantiek (`device.rs` herbewapent de
  timeout per slice, `shell.rs` vertrouwt op 100 ms bij open). Een `SerialLink` van ~80
  regels, en meteen de plek voor de nep-poort uit D1.
- **D7 — Fouttype.** Vijf van zeven varianten zijn strings. De GUI classificeert al op
  variant voor "apparaat weg" (`Io | Serial`), maar een losgetrokken kabel op macOS kan
  als `Timeout` binnenkomen. Touchstone-fouten zijn `InvalidArgument`; een
  `Parse { line, msg }` laat de GUI de regel noemen. `NotConnected` en `Unsupported(Kind)`
  ontbreken; CLI en GUI verzinnen beide "needs a LiteVNA".
- **D8 — Conventies die drie keer bestaan**: `Z0` in `calibration.rs` en `timedomain.rs`;
  SWR-clamp 999 (`touchstone`) versus 20 (`traces`); dB-vloer −200 versus −240.
- **D9 — Werk per frame dat met de sweepgrootte schaalt**: `freq_sweep()` kloont de hele
  `SweepResult` inclusief ruwe FIFO-records drie keer per frame; `set.solve()` draait elk
  frame voor de statusregel. `Arc<SweepResult>` voor `shown` en referenties, en een
  gecachete statusstring.
- **D10 — Publieke API-oppervlak** is inconsistent: `CalKit`, `Calibration`, `FifoPoint`
  op de root; `TdrConfig`, `TraceKind`, `Kind`, `read_touchstone_file_full` niet.
  `FifoPoint` gebruikt niemand buiten de crate.
- **D11 — CI**: geen clippy, geen `cargo doc`, geen MSRV (`Option::is_none_or` eist
  Rust ≥ 1.82, nergens vastgelegd), tests alleen met `--release` (geen overflow-checks),
  Nix-job zonder binaire cache, geen dependabot. Release: `ubuntu-latest` bepaalt de
  glibc-ondergrens van de Linux-tarball (Debian 12 valt af); macOS `tar` zonder
  `COPYFILE_DISABLE=1` stopt `._*`-bestanden in het archief; `generate_release_notes:
  true` plakt automatische notes onder de changelog-tekst.
- **D12 — Onboarding**: geen CONTRIBUTING, geen CLAUDE.md, geen architectuurnotitie
  buiten `lib.rs` en de README-tabel; onderzoeksnotities in het Nederlands, README en
  issue-sjabloon in het Engels; roadmap is "af", dus een nieuwkomer ziet geen "wat nu".
- **D13 — Handleidingen synchroon op discipline.** Geen script of CI-check; nu geen
  drift gevonden (koppen, afbeeldingen en codeblokken per hoofdstuk gelijk), maar de
  verouderde tekst (v0.2.3, GTK) staat trouw in beide.

Ontbrekende tests, concreet: `SweepConfig::linear/from_frequencies/stop_hz` (geen
enkele), `SweepResult::concat/segment_ranges`, `Screenshot::to_rgb888_with`,
`PortCandidate::kind`, Touchstone met tabs/CRLF/dubbele rij/ruisblok/`nan`,
`plausibility_warning` per standaard, `fit_open` met `OpenModel::Delay`, Blackman en
rechthoekig venster door `tdr`, `peaks` met `min_separation`, `traces::compute(GroupDelay)`,
`format_hz` GHz-tak. Plus golden files voor s2p/CSV/cal-JSON en `assert_cmd`-tests voor
de CLI.

---

## 8. Overbodige ballast en verouderde teksten

- **L1 — Getrackt in git, niet van ons**: `assets/liteVNA62/64/67-260713.bin` (firmware
  van de fabrikant), `LiteVNA_User Guide.pdf`, `…-cn.pdf`, `LiteVNA Menu Structure
  Map-1.2.pdf`. In bytes onschuldig (pack 2 MiB), als licentiekwestie niet: een MIT-repo
  herdistribueert nu propriëtaire firmware. Vervang door links in docs/02. De 464 MB op
  schijf is genegeerd spul (zips, 7z, `nanovna-saver`) en nooit gecommit; dat kan lokaal
  weg of naar een map buiten de repo.
- **L2 — GTK is geen afhankelijkheid.** `rfd` 0.17 gebruikt standaard de xdg-portal; er
  zit geen `gtk-sys` in de lockfile. README, beide handleidingen, de CI-apt-regel en
  `flake.nix` (`gtk3`, `wrapGAppsHook3`) noemen het toch. De echte eis, een draaiende
  `xdg-desktop-portal`, staat nergens: op een kale X11-windowmanager geeft elke
  bestandsdialoog stil `None`.
- **L3 — Verouderd in README/docs**: "Status: version 0.2.0"; `vna-r-v0.2.3-…` in de
  voorbeelden (ook in beide installatiehoofdstukken); `[0.1.0]` in CHANGELOG wijst naar
  een commit van 4 nov; `docs/04-tdr.md:30` beschrijft nog de DC-extrapolatie van vóór
  0.2.5; roadmap zegt "kit-fit (CLI)" en noemt een batterijindicator die bestaat;
  `docs/06 §3` noemt `set_bandwidth` als "gebruikt door vna-r", wat niet zo is.
- **L4 — Ongebruikte code in de library**: `write_u64`, `write_u32`/`set_unix_time`,
  `set_power`, `raw_command` (met een verkeerd doc-commentaar), `read_fifo_all` (alleen
  met `fifo_all == true`), `pause`, `set_bandwidth`, `Kind::name`, `as_shell_mut`,
  `fit_open_capacitance`, `ErrorTerms` Serialize/Deserialize, `read_touchstone`
  (alleen tests). Niet alles hoeft weg; `set_bandwidth` en `set_power` zijn protocol-
  functies die later nut hebben, maar dan moet de doc dat zeggen.
- **L5 — Kleine ballast**: `num-complex` ongebruikt in `vna-cli`; `anyhow` in de GUI
  alleen om één closure te typen; `build.rs` byte-voor-byte dubbel; `MARKER_COLOR ==
  MARKER_COLORS[0]`; `eprintln!` naast `self.log`; `C0` geïmporteerd maar volledig
  gekwalificeerd gebruikt; `accesskit` (zbus/atspi op Linux) is de enige zinvolle trim
  in eframe-features; `.gitignore` blokkeert `*.s2p` en `*.csv` overal, wat toekomstige
  testfixtures in de weg zit.
- **L6 — Stale commentaren**: `protocol.rs:141` ("is something to verify" over de
  checksum, in docs/01 al op >20 000 records bevestigd); `device.rs:226` (">1600
  points/s", gemeten 357–1100); `device.rs:103` ("None where a point never arrived",
  komt bij een LiteVNA nooit voor); `app.rs:299` (tdr_cache-commentaar).
- **L7 — `examples/probe_shell.rs`**: verdient zijn plek (docs/06 §7 verwijst ernaar),
  maar herimplementeert `read_until_prompt`; laat hem `NanoVnaShell::command` gebruiken
  zodat hij meeloopt met de library.

---

## 9. Eigen inzichten

**Het project is drie dagen oud en heeft zes releases op één dag.** Dat is begrijpelijk
voor de eerste testronde, maar elke release vergrendelt het calibratieformaat, de
instellingen en de Help-links nog wat vaster. Mijn advies: 0.2.6 als pure bugfix (B1 t/m
B9, U1, U2, U5, U12, L3) en dan een 0.3 waarin bewust de structuur wordt aangepakt (D1
t/m D7) vóór er nieuwe features bijkomen. Zonder de nep-poort (D1) blijft elke wijziging
in `device.rs` en `shell.rs` een gok die pas met het apparaat op tafel getest kan worden.

**De waarde van het programma zit in correcte calibratie, en daar zitten de scherpste
randen.** B1 (interpolatie), U1 (verkeerd raster), U2 (verloren bevestigingen) en B13
(standaarden ongemiddeld) raken allemaal dezelfde flow. Eén gerichte sessie "calibratie
van begin tot eind, met NanoVNA-F én LiteVNA" is meer waard dan tien losse features.
Daarbij hoort ook een meting voor B18 en B20: wat doet de FIFO werkelijk na een
configuratiewijziging en bij average 80? Dat is onderzoek voor `docs/01`, geen code.

**Twee talen, één keuze.** De CHANGELOG is Nederlands en wordt letterlijk de Engelse
GitHub-releasetekst; README en issue-sjabloon zijn Engels; onderzoeksnotities Nederlands.
Voor testers buiten Nederland is Engels in CHANGELOG en releasenotes de logische stap;
de onderzoeksnotities mogen Nederlands blijven, dat zijn jouw aantekeningen.

**De Help-koppeling aan de tag dwingt releases af voor documentatiefouten.** Het archief
bevat de handleiding al; de GUI kan die lokaal openen (naast de binary of via een
`VNA_R_DOCS`-pad) en pas terugvallen op GitHub. Dan hoeft een typefout geen 0.2.7 te
worden.

**Windows is nooit met een apparaat getest.** De README zegt dat eerlijk, maar
`COM`-poortdetectie via `serialport` en `looks_like_vna()` op Windows-productnamen zijn
precies de dingen die daar anders zijn. Een tester vragen om `vna-cli list` en `info` te
draaien kost weinig en haalt een blinde vlek weg.

**De GUI-crate heeft nul tests en dat is deels een gevolg van D3.** Zodra resampling,
middeling, segmentplanning en marker-opzoeken in de library zitten, is wat overblijft
in `app.rs` echt alleen UI, en is "geen tests in de GUI" verdedigbaar.

---

## 10. Besluiten (10 november 2026)

De vragen uit de eerste versie van dit rapport zijn doorgenomen; dit is besloten.

1. **Firmware en PDF's in `assets/`**: uit git, vervangen door downloadlinks in
   `docs/02-assets-inventaris.md`. Geen geschiedenis herschrijven; de bestanden blijven
   lokaal en worden genegeerd.
2. **Taal**: CHANGELOG en releasenotes gaan naar het Engels, zoals README en
   issue-sjabloon. De onderzoeksnotities `docs/01` t/m `docs/07` blijven Nederlands.
3. **Calibratie-interpolatie (B1)**: polair interpoleren, zoals de TDR-module al doet.
   De 90°-grens vervalt; er blijft alleen een controle op ondersamplede fase over.
4. **Markers (B6)**: op frequentie houden; per sweep het dichtstbijzijnde punt zoeken,
   buiten bereik verdwijnt de marker.
5. **Stop-latentie (U4/F2)**: annuleerpunt per 255-record-chunk (LiteVNA) en per
   `scan_bin`-stuk (NanoVNA-F), via een `sweep_with`-variant in de library.
6. **Help (F7)**: handleiding lokaal openen uit `docs/` naast de binary, GitHub bij de
   tag als terugval.
7. **Windows**: geen tester beschikbaar; blijft als bekende blinde vlek in de README.
8. **Referenties (N12)**: bewaren met de sessie, als .s2p naast de instellingen.

---

## 11. Werklijst

De uitgewerkte werklijst staat in [docs/05-roadmap.md](05-roadmap.md) onder "Werklijst
november 2026", met per punt de code uit dit rapport.
