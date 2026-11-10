# QC-procedure werklijst november 2026

Per werklijstpunt (nummers uit [docs/05-roadmap.md](05-roadmap.md), codes uit
[docs/07-analyse-en-aanbevelingen.md](07-analyse-en-aanbevelingen.md)): wat er veranderd
is, hoe je het controleert en wat je moet zien. **Apparaat** zegt of er hardware voor
nodig is. Voer eerst de algemene controle uit, dan de punten.

## Algemeen (zonder apparaat)

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

Verwacht: geen fmt-afwijking, alleen de bekende clippy-stijlmeldingen, alle tests groen.

```bash
cargo run --release -p vna-gui
```

Verwacht: GUI start, titelbalk toont de versie, Help ▸ About toont versie en git-revisie.

---

## Punt 1 — Polaire calibratie-interpolatie (B1)

**Veranderd.** `calibration::interpolate_complex` interpoleert grootte en fase apart;
fasegrens 135° op open, short, through (S21), line open, line short. GUI-resampler voor
referenties gebruikt dezelfde functie.

**Zonder apparaat.**
```bash
cargo test -p litevna interpolation
```
Verwacht: 3 tests groen, waaronder `interpolation_keeps_magnitude_on_a_coarse_grid`.

**Met apparaat (LiteVNA, N-kit).**
1. Calibreer 50 kHz – 6,3 GHz met **101** punten (open, short, load), sla op als `qc-grof.cal.json`.
2. Sweep 50 kHz – 6,3 GHz met **1001** punten met deze calibratie en de **short** nog aangesloten.
3. Lees |S11| af op de 1001-puntenweergave (logmag).

Verwacht: |S11| ligt overal binnen ±0,2 dB van 0 dB. Vóór 0.2.6 lag de curve tussen de
calibratiepunten tot +0,3 dB te hoog (bij 101 punten draait de open zo'n 20° per stap;
bij een kabel tussen VNA en kit meer en dus erger).
4. Herhaal met `cal/n-6g3.cal.json` (1001 punten) op een sweep van 1001 punten: moet
   onveranderd zijn ten opzichte van 0.2.5 (eigen raster, geen interpolatie).
5. Zet een through-meting van een 1 m kabel in een calibratie met 51 punten over
   6,3 GHz en probeer te interpoleren: verwacht foutmelding "too coarse … through phase
   jumps …°" (de fase draait daar meer dan 135° per stap).

**Referenties.** Houd een trace vast (Hold) op 201 punten, sweep daarna op 401 punten,
zet Δ aan: de Δ-trace mag geen periodieke bult vertonen op de posities tussen de oude
rasterpunten.

## Punt 2 — TDR-bereik (B2)

**Veranderd.** `TdrResult::max_time_s` is 1/(2·Δf) in plaats van 1/Δf; GUI-label, as-limiet
en CLI-regel volgen; de gate-foutmelding noemt het bereik direct.

**Zonder apparaat.**
```bash
cargo test -p litevna timedomain
cargo run --release -p vna-cli -- tdr --from cal/sweep.s2p --vf 0.70
```
Verwacht: tests groen; de regel "TDR: resolution … alias-free range X m (Y ns)" geeft voor
`cal/sweep.s2p` (bekijk de stap Δf in het bestand) Y = 1/(2·Δf). Voor 50 kHz – 6,3 GHz in
1001 punten (Δf = 6,3 MHz): 79 ns, 8,3 m bij VF 0,70.

**GUI.** Open `cal/sweep.s2p` als trace, kies TDR, zet Max distance op 1000 m: de x-as
eindigt waar de laatste gegevens staan (geen lege rechterhelft), en het label
"alias-free range" onder de TDR-instellingen noemt dezelfde afstand als het einde van de as.
Vóór 0.2.6 stond de as tot het dubbele en was de rechterhelft leeg.

## Punt 3 — Mislukte shell-open wacht niet dubbel (B3)

**Veranderd.** `NanoVnaShell::open` zet `restore_on_drop` pas aan nadat het apparaat een
prompt gaf.

**Zonder VNA, wel een andere seriële poort** (bijvoorbeeld een Arduino of USB-serieel
adapter zonder USB-id die vna-r kent):
```bash
time cargo run --release -p vna-cli -- --port /dev/cu.<die-poort> info
```
Verwacht: foutmelding "neither protocol answered on …" na ongeveer 4 s (LiteVNA-probe plus
één shell-timeout van 3 s). Vóór 0.2.6 duurde het ongeveer 7 s. Zonder tweede apparaat is
dit punt alleen door code-inspectie te controleren: `restore_on_drop: false` in `open`,
`true` na `sync()`.

## Punt 4 — Calibratiemeldingen blijven staan (B4, U2)

**Veranderd.** `CalState` heeft naast `message` een `solve_error`; het paneel toont de
status in geel en een niet-toepasbare calibratie in oranje als "not applied: …".

**Met apparaat.** Verbind, Run, meet **open**: onder de calibratieregels verschijnt
"measured open" en blijft staan tot de volgende actie. Meet short en load: "measured
short", "measured load". **Load…** een bestand: "loaded". Kit fit ▸ Apply to kit: "kit
updated … save to keep it" blijft zichtbaar.

**Zonder apparaat.** File ▸ Open as trace met een .s2p van 1 – 30 MHz, dan File ▸ Load
calibration met `cal/n-6g3.cal.json` (50 kHz – 6,3 GHz, dat kan) en daarna een .s2p van
0 – 10 kHz of boven 6,3 GHz als trace: verwacht geel "loaded" én oranje "not applied:
target grid … lies outside the calibrated span …". Vóór 0.2.6 verdrong de tweede melding
de eerste.

## Punt 5 — Standaarden en weergave op één raster (U1)

**Veranderd.** Een standaardknop past eerst de getypte Start/Stop/Points toe als die
afwijken van de lopende sweep; het paneel toont een oranje regel zolang dat zo is.

**Met apparaat.**
1. Verbind, zet 1 M – 30 M, 201 punten, **Apply sweep**, Run.
2. Typ Stop = `50M` zonder Apply: onder de standaardknoppen verschijnt "Start/Stop/Points
   differ from the running sweep; measuring a standard applies them first."
3. Klik **open**: de weergave springt naar 1 – 50 MHz, de regel verdwijnt, daarna
   "measured open" en de paneelregel "201 pts, 1.000 MHz → 50.000 MHz".
4. Meet short en load: het label wordt CAL en de weergave blijft op 1 – 50 MHz.

Vóór 0.2.6 bleef de weergave op 1 – 30 MHz en werd de 50 MHz-calibratie stil
geïnterpoleerd.

## Punt 6 — Een mislukte schermafdruk stopt de sweep niet (B5)

**Veranderd.** `Event::Error` draagt `stops_sweep`; screenshot, set average en release
melden een fout zonder de sweep te stoppen.

**Met apparaat.** Run (continu), dan **Screenshot** en trek binnen de seconde die de
overdracht duurt de USB-kabel los en steek hem terug. Dit geeft een I/O-fout op de sweep
zelf, dus geen goede test. Beter: NanoVNA-F, Run, en **set** bij Average drukken is daar
uitgeschakeld. Praktisch controleerbaar scenario: LiteVNA, Run, Screenshot drie keer snel
achter elkaar; als één capture time-out geeft, staat de fout rood in de topbalk en loopt
de sweep door (Stop-knop blijft zichtbaar). Vóór 0.2.6 stond de sweep stil en moest Run
opnieuw. Als geen enkele capture faalt, controleer de code: `warn` in `worker.rs` bij
Screenshot en SetAverage, `fail` alleen bij Connect en in `sweep_once`.

## Punt 7 — Markers blijven op hun frequentie (B6)

**Veranderd.** Bij elke nieuwe sweep of geopend bestand met een ander raster worden de
markers op hun oude frequentie naar het dichtstbijzijnde nieuwe punt gezet; buiten het
nieuwe bereik verdwijnen ze.

**Zonder apparaat.** Open een .s2p van 1 – 30 MHz als trace, zet M1 op ongeveer 14 MHz
(tabel toont de frequentie). Open een .s2p van 1 – 60 MHz: M1 staat nog op ~14 MHz.
Open een .s2p van 100 – 200 MHz: M1 is weg.

**Met apparaat.** 1 – 30 MHz, 201 punten, M1 op 14,2 MHz; Apply met 401 punten: M1 blijft
op 14,2 MHz (vóór 0.2.6: 7,6 MHz, hetzelfde puntnummer). Apply met 1 – 60 MHz: M1 blijft
op ~14,2 MHz.

## Punt 8 — Pijltjestoetsen alleen buiten tekstvelden (B7)

**Zonder apparaat.** Open een .s2p als trace, zet M1 en lees de frequentie af. Klik in het
veld **Stop**, druk vijf keer ←: de cursor beweegt in het veld, M1 staat nog op dezelfde
frequentie. Klik in de grafiek (buiten een tekstveld) en druk ←: M1 schuift één punt.

## Punt 9 — CLI-uitvoer (B8, B9, U12)

**Zonder apparaat.**
```bash
cargo run --release -p vna-cli -- tdr --from cal/sweep.s2p --csv - | head -3
ls -- -
```
Verwacht: drie CSV-regels op stdout (kop `distance_m,time_ns,…`) en `ls: -: No such file`.

**Met apparaat.**
```bash
cargo run --release -p vna-cli -- sweep --start 1M --stop 30M --points 51 --s2p /tmp/qc.s2p 2>&1 | grep -c 'first raw record'
head -1 /tmp/qc.s2p
cargo run --release -p vna-cli -- sweep --segment 1M:30M:51@cal/n-hf.cal.json --s2p /tmp/qc-seg.s2p && head -1 /tmp/qc-seg.s2p
```
Verwacht: `0` (geen debugregel zonder `--trace`); de eerste regel noemt het model
(`! LiteVNA sweep, 51 points, uncalibrated raw` of `! NanoVNA-F 4.3 sweep, …`); de
segmentversie zegt `host calibration per segment`. Met `--trace` verschijnt de regel
"first raw record" wel.

## Punt 10 — Schermafdruk-header begrensd (B11)

**Met apparaat.** `vna-cli screenshot --out /tmp/qc.png` op LiteVNA 64 en 67 en de GUI-knop
**Screenshot**: werkt als voorheen (480×320 resp. 800×480, 16 bpp). Een negatieve test
vraagt een apparaat dat een foute header stuurt; controleer anders de code: `bpp != 16`
en `MAX_SIDE` in `device.rs::screenshot`.

## Punt 11 — Apparaatgrens in "Set sweep for TDR" en nieuwe segmenten (U5)

**Met NanoVNA-F.** Verbind, sweep 1 – 30 MHz, kies TDR met Max distance 2 m: de
waarschuwing "Span too small" noemt een stop van hoogstens 1.500 GHz en **Set sweep for
TDR** zet Stop op `1.5GHz` (geen oranje buiten-bereik-melding). **+ segment** onder
Segments krijgt stop `1.5GHz`.

**Met LiteVNA.** Hetzelfde geeft 6,3 GHz, zoals voorheen. Zonder apparaat ook 6,3 GHz.

