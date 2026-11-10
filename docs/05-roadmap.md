# Roadmap

Stand: 2026-11-04. Wat er al is: binair LiteVNA-protocol, sweeps, SOLT-calibratie met
interpolatie en kit-fit (CLI), TDR, een egui-GUI met markers en bewaarde instellingen.
Alles hieronder bouwt daarop voort. De volgorde is bewust: eerst het instrument
afmaken, dan het gebruik verrijken, dan de breedte in. Elke stap sluit af met een
meting op echte hardware en een commit; de onderzoeksnotities in `docs/0x-*.md`
groeien per stap mee, de gebruikersdocumentatie komt als laatste stap samenhangend.

Grootte: S = een avond, M = een paar sessies, L = meerdere dagen inclusief meten.

| # | Stap | Grootte | Status |
|---|------|---------|--------|
| 1 | Kit-fit in de GUI | S | gedaan 2026-11-04 |
| 2 | Meer weergaven: R/X, fase, groepsvertraging | M | gedaan 2026-11-04 |
| 3 | Gated TDR (time gating) | M | gedaan 2026-11-04 |
| 4 | Load-definitie fitten | M | onderzocht 2026-11-04: niet identificeerbaar, zie docs/03 §9 |
| 5 | Averaging in software | S | gedaan 2026-11-04 |
| 6 | Multi-segment sweeps | M | gedaan 2026-11-04 |
| 7 | Referentietraces opslaan en vergelijken | S | gedaan 2026-11-04 |
| 8 | Touchstone inlezen | S | gedaan 2026-11-04 |
| 9 | LiteVNA 64 verifiëren | S | gedaan 2026-11-04 (zonder cal: kit-load defect) |
| 10 | NanoVNA-H tekstprotocol (LiteVNA-H1) | L | gedaan 2026-11-04 op de NanoVNA-F 4.3 (H1 niet getest), zie docs/06 |
| 11 | Linux-build | S | gedaan 2026-11-04 (CI, flake, CLI en GUI op NixOS-laptop geverifieerd) |
| 12 | Gebruikersdocumentatie | L | gedaan 2026-11-04 (docs/handleiding) |

---

## 1. Kit-fit in de GUI

**Doel.** De calibratie tot 6,3 GHz volledig vanuit de GUI kunnen doen, zonder CLI.

**Wat.** In het calibratiepaneel twee extra standaarden `line open` en `line short`
(bestaan al in `CalibrationSet`), een knop **Fit kit** die `fit_open` aanroept met
instelbare f_min en keuze C/delay, een resultaatregel (ΔC of Δτ, punten gebruikt,
ripple voor/na) en een **Apply**-knop die de kit in de set zet. De
plausibiliteitswaarschuwingen uit de CLI (losse open, onwaarschijnlijke e11) ook tonen.

**Klaar als** de 6,3 GHz calibratie uit `cal/n-6g3.cal.json` opnieuw gemaakt kan
worden in de GUI met hetzelfde fitresultaat (open C0 ≈ 40 fF) en dezelfde ripple op de
short-afgesloten kabel.

**Raakt** `crates/vna-gui/src/app.rs`, `worker.rs` (Purpose::Standard dekt het al).

## 2. Meer weergaven

**Doel.** Antennewerk: R en X apart zien, plus fase en groepsvertraging voor filters
en kabels.

**Wat.** Een trace-keuzelijst per plot in plaats van vaste |S11|/|S21| dB:
`log mag`, `phase`, `SWR`, `R + jX`, `|Z|`, `group delay`. Groepsvertraging is
−dφ/dω van S21, numeriek over naburige punten met een instelbare apertuur
(in punten), met unwrap van de fase. Markers tonen altijd de volledige set, de
tabel blijft zoals hij is. Y-as schaalt per type (graden ±180, ohm, ns).

**Klaar als** een stuk kabel een vlakke groepsvertraging geeft die overeenkomt met
de TDR-afstand (τ = 2·d/(c·VF)), en R/X van de load vlak rond 50 + j0 ligt.

**Raakt** `app.rs`; mogelijk een `traces.rs` die uit S-parameters de afgeleide
grootheden rekent (UI-vrij, zodat de CLI het later ook kan exporteren).

## 3. Gated TDR

**Doel.** Een reflectie in de tijd isoleren of wegsnijden en terugrekenen naar
frequentie: de connectorreflectie uit een antennemeting filteren, of alleen het
antenne-eind van een voedingslijn bekijken.

**Wat.** In `timedomain.rs` een `gate(freqs, s11, cfg, t_start, t_stop, GateKind)`
die het band-pass impulssignaal vermenigvuldigt met een venster in de tijd
(rechthoek met Hann-flanken, flank instelbaar), terugtransformeert en de
oorspronkelijke frequentiepunten terugpakt. De windowcompensatie moet netjes:
voorwaarts met window, gate, dan delen door het window in het frequentiedomein met
een vloer, of de klassieke aanpak via de responsie van een ideale reflectie. In de
GUI: twee sleepbare lijnen in de TDR-plot, een schakelaar `gated` in het
frequentiebeeld, en de gate-grenzen in de instellingen.

**Klaar als** de kabel met short, gegated rond de short, in het frequentiebeeld
een gladde |S11| ≈ 0 dB oplevert met de 0 m connectorbult weg, en een gate om de
connector alleen de -34 dB bult laat zien. Rand-effecten aan de bandgrenzen
documenteren in `docs/04-tdr.md`.

**Raakt** `crates/litevna/src/timedomain.rs` (+ tests met synthetische
reflecties), `app.rs`, `vna-cli` (`tdr --gate 0.9m:1.2m`).

## 4. Load-definitie fitten

**Doel.** De resterende 0,2 tot 0,5 dB ripple boven 3 GHz verkleinen.

**Wat.** Meetsessie met de load op de kabel (`line-load`), naast de bestaande
line-open/line-short. Model voor de load: R plus serie-L of offset delay. Fit zoals
bij de open: residuen van de gecorrigeerde lijnmeting minimaliseren, iteratief.
Eerst onderzoeken of de ripple werkelijk uit de load komt of uit de poort-2 match
(die zit niet in het enhanced-response model); dat onderscheid is zelf een
meetresultaat voor `docs/03-calibratie.md` §9.

**Klaar als** de ripple op de short-afgesloten kabel boven 3 GHz aantoonbaar daalt
(nu 0,56 dB) of gedocumenteerd is waarom niet.

**Raakt** `calibration.rs` (Standard::LineLoad, `fit_load`), CLI `cal fit`, GUI.

## 5. Averaging in software

**Doel.** Lage niveaus (−60 dB en lager) stabiel maken zonder het apparaat trager
te zetten.

**Wat.** In de worker een lopend complex gemiddelde over de laatste N sweeps
(exponentieel of blok), per S-parameter, gereset bij wijziging van het grid.
Instelling `avg N` naast de apparaat-average; tonen hoeveel sweeps al meegeteld
zijn. Complex middelen, niet in dB: dat haalt ruis weg zonder de
magnitude-bias van dB-middelen.

**Klaar als** de spreiding op de load-meting (nu ±0,01 dB) met N=16 zichtbaar
kleiner is, en een markerlezing op −70 dB stil staat.

**Raakt** `worker.rs`, `app.rs`.

## 6. Multi-segment sweeps

**Doel.** HF fijn en UHF/SHF grof in één beeld, bijvoorbeeld 1–30 MHz in 1001
punten plus 30 MHz–6 GHz in 1001 punten.

**Wat.** `SweepConfig` krijgt een lijst segmenten; de worker sweept ze na elkaar en
plakt de resultaten; de calibratie wordt per segment geïnterpoleerd (kan al). GUI:
een segmententabel met start/stop/punten en een `+`. Markers, TDR en export werken
op de geplakte lijst; TDR moet weigeren of waarschuwen bij een niet-uniform grid
(het harmonische resampling neemt dan het fijnste segment).

**Klaar als** een twee-segmentsweep met calibratie dezelfde waarden geeft als
twee losse sweeps, en de s2p-export oplopend gesorteerd is.

**Raakt** `device.rs` (SweepConfig), `worker.rs`, `app.rs`, `touchstone.rs`.

## 7. Referentietraces

**Doel.** Voor/na vergelijken: antenne afregelen, kabel vervangen.

**Wat.** Knop **Hold** zet de huidige sweep vast als referentie (gestippeld, zelfde
kleur gedimd); meerdere referenties met naam; verschil-trace (live min referentie
in dB) als optie. Referenties overleven een herstart via de bewaarde instellingen
niet (te groot), wel via opslaan als s2p en inlezen (stap 8).

**Klaar als** een antenne afgeregeld kan worden met de oude curve in beeld.

**Raakt** `app.rs`.

## 8. Touchstone inlezen

**Doel.** Zonder apparaat werken: oude metingen, simulaties, bestanden van de VNA
zelf bekijken en als referentie gebruiken.

**Wat.** `touchstone::read_s1p/s2p` (formaten RI/MA/DB, Hz/kHz/MHz/GHz, 1 en 2
poorten, commentaar). In de GUI **Open...** die de inhoud als trace of als
referentie laadt; alle weergaven, markers en TDR werken erop.

**Klaar als** een door `write_s2p` geschreven bestand bit-identiek rondgaat, en een
s1p van de LiteVNA zelf (SD-kaart) correct inleest.

**Raakt** `touchstone.rs` (+ tests), `app.rs`, CLI `tdr --from file.s2p`.

## 9. LiteVNA 64 verifiëren

**Doel.** Zekerheid dat het protocol op het tweede apparaat werkt.

**Wat.** Dezelfde reeks als op de 67: `info`, `peek`, sweep, screenshot, FIFO-
checksum, stale records, bereik 50 kHz–6,3 GHz. Let op verschillen in
identiteitsregisters (de 67 is niet van een 64 te onderscheiden) en op de
screenshot-resolutie (de 64 heeft een ander scherm). Resultaten in
`docs/01-onderzoek-protocol.md` §8.

**Klaar als** de GUI op de 64 een calibratie en TDR doet zonder codewijziging, of
de nodige wijzigingen gecommit zijn.

## 10. NanoVNA-H tekstprotocol

**Doel.** De LiteVNA-H1 (NanoVNA-H firmware, tekst-shell over USB) ondersteunen.

**Wat.** Eerst onderzoek zoals bij stap 0: shell-commando's `info`, `sweep`,
`frequencies`, `data 0/1`, `scan`, prompt-detectie, snelheid (deze apparaten zijn
veel trager, ~100 punten/s). Dan een `trait Vna { info, configure, sweep, ... }`
in de library met twee implementaties, detectie op VID/PID en zo nodig een probe
(NOP-antwoord versus `ch>`-prompt). GUI en CLI gaan via de trait. Calibratie en TDR
zijn apparaatonafhankelijk en werken meteen.

**Klaar als** `vna-cli info` en `sweep` op de H1 werken, en de GUI het apparaat
automatisch kiest. Documentatie in `docs/06-nanovna-h-protocol.md`.

**Resultaat (4 november 2026).** Gedaan met een NanoVNA-F 4.3 in plaats van de H1:
`shell.rs` (`NanoVnaShell`, `scan_bin` met ruwe data, 301 punten per stuk, `capture`),
`vna.rs` (`enum Vna` in plaats van een trait; CLI en GUI gaan erdoorheen), detectie op
`0483:5740` plus productnaam. `info`, `sweep` (ook 1001 punten in stukken) en
`screenshot` werken; de GUI toont model en bereik en zet Average uit. Metingen en
formaat in `docs/06-nanovna-shell-protocol.md`. Niet getest: H1/H/H4 (zelfde familie,
andere firmware) en calibratie met de SMA-kit.

## 11. Linux-build

**Doel.** Portabiliteit waarmaken.

**Wat.** Bouwen en draaien op een Linux-machine of VM (of GitHub Actions voor
alleen de build): `serialport` heeft libudev nodig, eframe wayland/x11-
afhankelijkheden. Udev-regel voor 04B4:0008 in `docs/` en README. Mogelijk een
CI-workflow die op macOS en Linux `cargo test` draait.

**Klaar als** een Linux-build een sweep doet, of de CI groen is met een
gedocumenteerde udev-regel.

## 12. Gebruikersdocumentatie

**Doel.** Een handleiding waarin de RF-uitleg zit die we onderweg hebben opgebouwd:
wat een VNA meet, wat S11/S21/SWR/Z betekenen, waarom en hoe je calibreert, wat de
kit-fit doet, wat de TDR-plots laten zien en wat het window betekent, wat gating
doet, hoe markers en referenties werken.

**Wat.** Een `docs/handleiding/` of een mdBook met hoofdstukken per onderwerp, in
het Nederlands, met schermafbeeldingen uit de GUI en de meetvoorbeelden uit
`docs/03` en `docs/04`. De CLI krijgt een referentiehoofdstuk uit `--help`.

**Klaar als** iemand met een LiteVNA en zonder RF-voorkennis met alleen de
handleiding een antenne kan meten, calibreren en een kabelfout kan lokaliseren.

---

## Buiten de volgorde

Kleine dingen die tussendoor kunnen zonder een stap te zijn: apparaat-bandbreedte
(register 0x40 kan niet teruggelezen worden, dus alleen schrijven met een eigen
administratie), batterijindicator in de GUI, een `--json` export in de CLI, en een
icoon voor de app-bundel.

Gedaan voor de eerste testronde met anderen (0.2.0): één versienummer in de workspace
met git-revisie, menubalk met About, Developer tools (log en diagnostiek) standaard uit,
CHANGELOG met releaseprocedure en een tag-controle in de release-workflow.

---

# Werklijst november 2026

Stand: 2026-11-10, na de doorlichting in [docs/07-analyse-en-aanbevelingen.md](07-analyse-en-aanbevelingen.md).
De codes (B1, U2, D3 …) verwijzen naar dat rapport; de besluiten staan in §10 daarvan.
Grootte als hierboven: S = een avond, M = een paar sessies, L = meerdere dagen.

## 0.2.6 — bugfix, geen nieuwe features

| # | Werk | Code | Grootte | Status |
|---|------|------|---------|--------|
| 1 | Calibratie-interpolatie polair (grootte en fase), 90°-grens eruit, test met grof raster | B1 | S | gedaan 2026-11-10 (grens 135°, ook short/through/line) |
| 2 | `max_time_s = 1/(2·df)`, test en GUI-label "alias-free range" mee | B2 | S | gedaan 2026-11-10 |
| 3 | `NanoVnaShell::open`: `restore_on_drop` pas na geslaagde `sync` | B3 | S | |
| 4 | Statusmelding ("measured …", "loaded", "save to keep it") scheiden van de solve-fout | B4, U2 | S | |
| 5 | Standaarden meten op het toegepaste raster, of waarschuwen bij afwijking van het getypte | U1 | S | |
| 6 | `fail()` stopt de lus alleen bij sweep- en connect-fouten; screenshot en set average niet | B5 | S | |
| 7 | Markers op frequentie: per sweep `nearest_index`, buiten bereik weg | B6, N1 | S | |
| 8 | Pijltjestoetsen alleen zonder tekstfocus (`wants_keyboard_input`) | B7 | S | |
| 9 | CLI: s2p-commentaar bij `--segment` en per apparaat, `tdr --csv -` naar stdout, debugregel achter `--trace` | B8, B9, U12 | S | |
| 10 | Screenshot-header cappen (bijv. 4096×4096, alleen bpp 16) | B11, V2 | S | |
| 11 | "Set sweep for TDR" en nieuwe segmenten uit `device_limits()` | U5, N19 | S | |
| 12 | Eén encodering voor "geen ruwe records" in `SweepResult.points`; Touchstone-sweep als standaard mogelijk | B10 | S | |
| 13 | Export-commentaar en referentienaam bij bestandstrace; standaard-sweeps door de middeling; floats uit instellingen clampen | B12, B13, B14 | S | |
| 14 | `points == 0` en u64-aftrekking afvangen; `check_lines` band ≤ 0 weigeren | B15, B16 | S | |
| 15 | udev-regel: `70-litevna.rules` met uaccess, zonder 0666 | V1 | S | |
| 16 | README/handleidingen: versie, archiefnamen, GTK-verwijzingen weg, xdg-portal noemen; docs/04 DC-term; roadmap-intro | L2, L3 | S | |
| 17 | CHANGELOG naar het Engels (besluit 2); `[0.1.0]`-link herstellen | L3 | S | |
| 18 | `num-complex` uit vna-cli; verouderde UI-teksten (Hold, step 8, bestandsnamen) | L5, U10 | S | |
| 19 | Firmware en PDF's uit git, links in docs/02 (besluit 1) | L1 | S | |
| 20 | `build.rs`: ook `refs/heads/*` en `packed-refs` als rerun-trigger | B25 | S | |

## 0.3 — structuur, vóór nieuwe features

| # | Werk | Code | Grootte | Status |
|---|------|------|---------|--------|
| 21 | `SerialLink` met injecteerbare nep-poort; tests voor `LiteVna::sweep`, `scan_bin`, `command`, screenshots | D1, D6 | M/L | |
| 22 | Annuleren en voortgang: `sweep_with(cfg, callback)` per chunk; worker en Stop erdoorheen (besluit 5) | F2, N2, U4 | M | |
| 23 | UI-vrije logica naar `litevna`: resampling, middeling, segmentplanning, breedste segment, per-segment-correctie, `nearest_index`, gate-spec | D3 | M | |
| 24 | `app.rs` splitsen in settings, calibration_ui, segments, plots, tdr_ui, files, diagnostics | D2 | M | |
| 25 | Versieveld in bewaarde instellingen en calibratie-JSON; `Vec` in plaats van vaste arrays | D4 | S | |
| 26 | Eén meldingsmodel (`Notice`) voor error, cal.message, gate_error, file_message, sweep_error | D5, U6, U11 | S/M | |
| 27 | Fouttype: `Parse { line }`, `NotConnected`, `Unsupported(Kind)`; "apparaat weg" ook bij `Timeout` | D7 | S | |
| 28 | Help opent de lokale handleiding naast de binary, GitHub als terugval (besluit 6) | F7 | S | |
| 29 | CI: clippy, `rust-version`, `COPYFILE_DISABLE`, `generate_release_notes` uit, handleiding-synccheck | D11, D13 | S | |
| 30 | Dode code en stale commentaren opruimen; `probe_shell` op `NanoVnaShell::command` | L4, L6, L7 | S | |
| 31 | `Arc<SweepResult>` voor shown en referenties; statusstring cachen | D9 | S | |
| 32 | Z0, SWR-clamp en dB-vloer op één plek | D8 | S | |
| 33 | CONTRIBUTING of korte architectuurnotitie | D12 | S | |

## Later — features

| # | Werk | Code | Grootte | Status |
|---|------|------|---------|--------|
| 34 | `--json` voor info, list, sweep, tdr, cal show | N4 | S | |
| 35 | Poortverlenging / elektrische vertraging op de host | N5 | S | |
| 36 | Referenties bewaren met de sessie (besluit 8) | N12 | M | |
| 37 | Host-middeling via `valuesPerFrequency` | N6, F3 | S/M | |
| 38 | Sneltoetsen, auto-connect, batterij verversen, TDR-marker nudgen | N7–N9, N11 | S | |
| 39 | Smith-chart hover-uitlezing en frequentielabels | N10 | M | |
| 40 | CSV met afgeleide kolommen; `.s1p`-export; Touchstone v2 | N15, N16 | S | |
| 41 | `cal` als subcommando's met completions | N17 | M | |
| 42 | Kit-bestanden lezen, load serie-L | N14 | S/M | |
| 43 | Tekst-`scan` voor NanoVNA-H/H4 zonder `scan_bin` | N13 | M | alleen met hardware |
| 44 | 12-term 2-poorts via omdraaien van de DUT | N18 | L | alleen met hardware |

## Meten

| # | Werk | Code | Status |
|---|------|------|--------|
| 45 | FIFO-timeouts bij average 80 en lage shell-bandbreedte | B18 | |
| 46 | Stale records na een configuratiewijziging (zelfde aantal punten, ander raster) | B20 | |
| 47 | Windows met een apparaat (`list`, `info`, `sweep`, `screenshot --trace`) | — | geen tester |
