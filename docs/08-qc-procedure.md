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

## Punt 12 — Geen ruwe records is een lege lijst (B10)

**Zonder apparaat.**
```bash
cargo test -p litevna a_sweep_without_raw_records
```
Verwacht: groen. De API heeft geen pad om een bestand als standaard in te lezen (CLI
`cal` meet altijd), dus dit is een library-eigenschap; hij komt van pas zodra de GUI of
CLI standaarden uit .s1p-bestanden accepteert.

## Punt 13 — Drie kleine GUI-punten (B12, B13, B14)

**Standaarden gemiddeld (met apparaat).** Zet Sweep avg op 8, verbind, meet **load**: de
knop toont "load…" ongeveer acht sweeps lang en de melding luidt "measured load (mean of
8 sweeps)". Met Sweep avg 1: "measured load".

**Bestandstrace (zonder apparaat).** Open een .s2p als trace, File ▸ Save Touchstone naar
`/tmp/qc-file.s2p`; `head -1 /tmp/qc-file.s2p` toont `! vna-r, N points, from file
<naam>`. **Hold** op dezelfde trace: de referentienaam eindigt niet op " raw".

**Beschadigde instellingen (zonder apparaat).** Sluit de GUI. Zet in
`~/Library/Application Support/vna-r/app.ron` (macOS) `tdr_vf:0.85` om in `tdr_vf:0.0`
en `tdr_max_m:…` in `tdr_max_m:NaN`. Start de GUI, kies TDR: velocity factor staat op
0,70 en Max distance op 2,0 m, geen foutmelding. Herstel het bestand daarna, of laat de
GUI het bij afsluiten overschrijven.

## Punt 14 — Underflow en eindeloze lus afgevangen (B15, B16)

**Zonder apparaat.**
```bash
cargo test -p litevna zero_point_config non_increasing_grid check_lines_rejects
```
Verwacht: drie tests groen. Praktijkcontrole: open een .s2p waarvan je twee regels hebt
omgewisseld (niet-oplopend) als trace: de reader weigert hem al ("frequencies are not
increasing"); de TDR-controle is de tweede verdedigingslinie voor samengevoegde segmenten.

## Punt 15 — udev-regel 70-litevna.rules (V1)

**Op een Linux-machine (hp-nixos of een Debian/Fedora-doos), met apparaat.**
```bash
sudo rm -f /etc/udev/rules.d/99-litevna.rules
sudo cp linux/70-litevna.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger
```
Herplug het apparaat en controleer:
```bash
ls -l /dev/ttyACM0 && getfacl /dev/ttyACM0 2>/dev/null | grep user:
vna-cli info
```
Verwacht: `crw-rw----+ 1 root dialout` (een `+` voor de ACL) met een regel
`user:<jouw naam>:rw-` uit getfacl, en `vna-cli info` werkt zonder sudo. Op NixOS via de
flake: `nix build .#vna-r && ls result/lib/udev/rules.d/` toont `70-litevna.rules`.

## Punt 16 — Documentatie en flake zonder GTK (L2, L3)

**Zonder apparaat.**
```bash
grep -rn -i 'gtk\|v0\.2\.3\|version 0\.2\.0' README.md docs/manual docs/handleiding .github flake.nix
```
Verwacht: alleen treffers waarin "gtk" als portal-backend genoemd wordt
(`xdg-desktop-portal-gtk`), geen `libgtk-3-dev`, geen oude versienummers.

**Na push (CI).** De job "nix build (flake)" moet groen zijn: de flake bouwt nu zonder
`gtk3` en `wrapGAppsHook3`, met `makeWrapper` voor het `LD_LIBRARY_PATH` van de GUI. Op de
NixOS-laptop daarna:
```bash
nix run github:pa1bh/vna-r
```
Verwacht: GUI start en File ▸ Open as trace… toont een dialoog (de portal van de desktop).

**Op Fedora/Debian met een release-tarball.** `vna-gui` start zonder GTK-ontwikkelpakketten;
File ▸ Open… toont de dialoog van de desktop. Op een kale X11-windowmanager zonder
`xdg-desktop-portal` verschijnt geen dialoog; dat staat nu in README en handleiding.

## Punt 17 — CHANGELOG in het Engels (L3)

**Zonder apparaat.** Lees `CHANGELOG.md` door: alle versies Engels, kopjes Added / Changed
/ Fixed / Documentation, de link `[0.1.0]` wijst naar commit `1a78b67` (laatste commit van
3 november). Controleer dat de release-workflow zijn sectie nog vindt:
```bash
awk -v v="0.2.5" '/^## /{p = index($0, "[" v "]") > 0} p' CHANGELOG.md | head -5
```
Verwacht: de kop `## [0.2.5] - 2026-11-05` en de eerste regels van die sectie. Bij de
volgende release moet de GitHub-releasetekst Engels zijn.

## Punt 18 — Kleine ballast en UI-teksten (L5, U10)

**Zonder apparaat.** Open een .s2p van 1 – 30 MHz als trace. File ▸ Save Touchstone: de
voorgestelde naam is `sweep-1MHz-30MHz.s2p`. Laad `cal/n-6g3.cal.json` en kies Save…
in het calibratiepaneel: voorgestelde naam `50kHz-6.3GHz.cal.json`. Onder **Traces**
zonder referenties: "Hold (in the marker bar above the charts) keeps …"; met een
referentie: "References are not saved; export one as .s2p and open it later with Open as
reference…."

```bash
cargo tree -p vna-cli --depth 1 | grep -c num-complex
```
Verwacht: `0`.

## Punt 19 — Fabrikantbestanden uit git (L1)

**Zonder apparaat.**
```bash
git ls-files assets
ls assets/*.pdf assets/*.bin
git status --short assets
```
Verwacht: alleen `assets/litevna-projects.txt` is getrackt; de drie PDF's en drie
firmware-images staan nog op schijf; `git status` toont ze niet (genegeerd). Ze zitten
nog wel in de git-geschiedenis: dat was het besluit (geen history rewrite).

## Punt 20 — Git-revisie volgt nieuwe commits (B25)

**Zonder apparaat.**
```bash
cargo build -q -p vna-cli && ./target/debug/vna-cli --version
git commit -q --allow-empty -m "qc: empty commit" && cargo build -q -p vna-cli && ./target/debug/vna-cli --version
git reset -q --hard HEAD~1
```
Verwacht: de tweede `--version` toont een andere revisie dan de eerste (de hash van de
lege commit). Vóór 0.2.6 bleef de oude staan tot een branchwissel. De laatste regel haalt
de testcommit weer weg; voer hem alleen uit op een schone werkboom.

---

# 0.3 — structuur

## Punt 29 — CI, clippy, minimale Rust-versie, handleidingcheck (D11, D13)

**Zonder apparaat.**
```bash
cargo clippy --workspace --all-targets -- -D warnings
./scripts/check-manuals.sh
grep -n 'rust-version' Cargo.toml crates/*/Cargo.toml
```
Verwacht: clippy zonder meldingen; "manuals in step: 10 files each"; `rust-version =
"1.95"` in de workspace en `rust-version.workspace = true` in de drie crates.

Negatieve test van het script: voeg tijdelijk een regel `## Test` toe aan
`docs/manual/03-gui.md` en draai het script: verwacht "DIFFERENT STRUCTURE" en exitcode 1.
Haal de regel weer weg.

**Na push.** Alle CI-jobs groen, inclusief de nieuwe stappen "Clippy", "Manuals in step"
en "Test the library with overflow checks".

**Bij de volgende release.** De Linux-job draait op `ubuntu-22.04`; als GitHub die runner
heeft uitgefaseerd faalt de job direct met een duidelijke melding en moet de regel in
`release.yml` naar de oudste beschikbare LTS. Controleer in het macOS-archief:
`tar tzf vna-r-v*-aarch64-macos.tar.gz | grep -c '/\._'` geeft `0`. De releasetekst op
GitHub bevat alleen de changelog-sectie.

## Punt 25 — Formaatversies (D4)

**Zonder apparaat.**
```bash
cargo test -p litevna calibration_file_format && cargo test -p vna-gui settings_
```
Verwacht: groen; de GUI-test leest een letterlijke instellingenregel van 0.2.5.

**Echte bestanden.**
1. Maak een kopie van je instellingen (`cp ~/Library/Application\ Support/vna-r/app.ron
   /tmp/app.ron.bak`), start de nieuwe GUI: poort, sweep, calibratiebestand, TDR-waarden,
   markerfrequentie en plotkeuzes zijn zoals je ze achterliet. Sluit af en bekijk
   `app.ron`: het bevat nu `version:1`, `marker_freqs:[…]` en `plot_kinds:[…]`.
2. Laad `cal/n-6g3.cal.json` (zonder `format`-veld): werkt. Save… naar `/tmp/qc.cal.json`
   en `head -3 /tmp/qc.cal.json` toont `"format": 1`.
3. Zet in `/tmp/qc.cal.json` de waarde op `99` en laad het: foutmelding "calibration file
   has format 99 … use a newer vna-r".
4. Terug naar 0.2.5 (oude binary) met de nieuwe `app.ron`: die versie kan de lijsten
   `[…]` niet als array lezen en begint met standaardinstellingen. Dat is de enige
   onomkeerbare kant van deze wijziging; zet zo nodig `/tmp/app.ron.bak` terug.

## Punt 27 — Fouttype en "apparaat weg" (D7)

**Zonder apparaat.**
```bash
cargo test -p litevna messages parse_errors_name_the_line
printf '# MHz S RI R 50\n1 0.5 0.0\n2 0.5 oops\n' > /tmp/qc-bad.s1p
cargo run -q --release -p vna-cli -- tdr --from /tmp/qc-bad.s1p
```
Verwacht: tests groen; de CLI meldt `… line 3: not a number: "oops"`. In de GUI geeft
File ▸ Open as trace met hetzelfde bestand "opening /tmp/qc-bad.s1p: line 3: not a number:
"oops"" in de topbalk.

**Met apparaat.** Run (continu) en trek de USB-kabel los: binnen één sweep (LiteVNA:
enkele seconden; de time-out is 3 s of meer) toont de topbalk de fout en staat de knop
weer op **Connect**; de poort verdwijnt uit de lijst. Steek de kabel terug: de poort
verschijnt binnen 2 s en Connect werkt. Vóór 0.3 kon de GUI bij een time-out "verbonden"
blijven met een verdwenen apparaat.

**NanoVNA-F.** `vna-cli peek F0` geeft "peek needs a LiteVNA (register protocol); this is
a NanoVNA" (zelfde tekst als voorheen, nu uit de library).

## Punt 30 — Dode code en commentaren (L4, L6, L7)

**Zonder apparaat.** `cargo build --workspace` en `cargo clippy` schoon; `cargo tree -p
vna-gui --depth 1 | grep -c anyhow` geeft `0`. Device ▸ Device screenshot ▸ Save
stelt `screenshot.png` voor (was `litevna.png`).

**Met NanoVNA-F (optioneel).**
```bash
cargo run -p litevna --example probe_shell -- /dev/cu.usbmodemXXXX info version
```
Verwacht: de antwoorden van `info` en `version` zoals in docs/06.

## Punt 32 — Eén definitie voor Z0, dB-vloer en SWR-plafond (D8)

**Zonder apparaat.**
```bash
cargo test -p litevna db_and_swr_conventions
grep -rn 'const Z0' crates/
```
Verwacht: test groen; precies één treffer (`crates/litevna/src/lib.rs`). In de GUI: een
trace met een punt waar |S21| = 0 (bijvoorbeeld een .s1p, S21 is daar nul) tekent in
log mag een vlakke lijn op −200 dB (was −240 dB); de markertabel toont −200.00.

## Punt 33 — CONTRIBUTING.md (D12)

Lees `CONTRIBUTING.md` door en controleer of de conventies kloppen met hoe jij wilt
werken (dit is jouw project; de tekst legt vast wat we tot nu toe deden). De drie
commando's bovenaan moeten op een schone checkout werken.

## Punt 21 — Drivertests zonder hardware (D1, B19)

**Veranderd.** `LiteVna::from_port` en `NanoVnaShell::from_port`; `fakeport.rs` (alleen in
tests) met een nagebootste LiteVNA en NanoVNA-F; na een mislukte sweep reset de LiteVNA-
driver het protocol en schrijft de sweepregisters opnieuw.

**Zonder apparaat.**
```bash
cargo test -p litevna device:: shell::
```
Verwacht: 32 tests groen in minder dan een seconde.

**Met apparaat: gedrag ongewijzigd.** Dit punt raakt `open` van beide drivers en de
sweep-lus van de LiteVNA, dus loop de basis na op beide apparaten:
```bash
vna-cli info
vna-cli sweep --start 1M --stop 30M --points 201
vna-cli sweep --start 50k --stop 6.3G --points 1001      # LiteVNA
vna-cli screenshot --out /tmp/qc.png
```
Verwacht: als in 0.2.5; in de samenvatting van `sweep` 0 ontbrekende punten.
GUI: Connect, Run, een minuut laten lopen, Stop, Disconnect, opnieuw Connect.

**Herstel na een fout (LiteVNA).** Run, trek de kabel los tijdens een sweep, steek hem
terug, Connect, Run: de sweep loopt weer normaal (geen "sweep did not complete").

## Punt 22 — Stop per blok en voortgangsbalk (F2, N2, U4)

**Veranderd.** `sweep_with` in de library (voortgang per blok, annuleren); de worker kijkt
na elk blok naar de commando's en breekt af bij Stop, Disconnect, Connect of een nieuwe
weergavesweep; een standaardmeting wacht op de lopende sweep. Voortgangsbalk in de topbalk
na 0,7 s.

**Zonder apparaat.**
```bash
cargo test -p litevna progress_is_reported
```
Verwacht: twee tests groen (LiteVNA: 255, 510, 600 van 600; shell: 301, 602, 700 van 700).

**Met NanoVNA-F** (hier is het verschil het grootst).
1. Sweep 1 – 900 MHz met 3201 punten, **Run**: na ongeveer een seconde verschijnt in de
   topbalk een balk die in stappen van 301 oploopt ("301/3201", "602/3201", …).
2. Druk **Stop** halverwege: binnen ongeveer 3 s staat de knop weer op Run en verdwijnt de
   balk. Vóór 0.3 duurde dat tot het einde van de sweep (ongeveer 28 s).
3. Run opnieuw, wijzig Points naar 201 en **Apply sweep**: de lange sweep wordt afgebroken
   en de korte verschijnt binnen enkele seconden.
4. Run met 3201 punten, **Disconnect** halverwege: binnen 3 s verbroken, het apparaat
   toont weer zijn eigen scherm.
5. Run met 1001 punten, klik **open** in het calibratiepaneel: de lopende sweep maakt
   zijn ronde af, dan volgt de standaardmeting ("open…"), daarna loopt de weergave door.
6. Klik **load** en direct **Stop**: melding "measuring load was stopped", de knop is weer
   vrij.

**Met LiteVNA.** 201 punten continu: geen balk (te snel). 3201 punten met Sweep avg 8
en een standaardmeting: balk loopt tot 25608 punten; Stop breekt binnen een seconde af.
Screenshot tijdens Run: de sweep loopt daarna door.

## Punt 48 — Niet-eindige waarden en dubbele frequenties (B21, B22)

**Zonder apparaat.**
```bash
cargo test -p litevna non_finite a_non_finite tabs_crlf
printf '# MHz S RI R 50\n1 0.5 0.0\n1 0.4 0.0\n2 0.3 0.0\n' > /tmp/qc-dup.s1p
cargo run -q --release -p vna-cli -- tdr --from /tmp/qc-dup.s1p
```
Verwacht: tests groen; de CLI meldt "frequency 1000000 Hz appears more than once". Een
bestand van een ander programma dat een frequentie dubbel bevat (aansluitende segmenten)
wordt nu geweigerd; vna-r zelf schrijft zulke bestanden niet. Als je zo'n bestand
tegenkomt dat je wel wilt lezen, is dat het moment om hierover te beslissen.

## Punt 23 — Gedeelde regels in de library (D3)

**Veranderd.** Segmentplanning, breedste segment, afstand→tijd, dichtstbijzijnde punt en
middeling staan in `litevna` met tests; CLI en GUI roepen ze aan. Gedrag hoort gelijk te
zijn.

**Zonder apparaat.**
```bash
cargo test -p litevna segments_are_parsed distances_convert nearest_index
cargo run -q --release -p vna-cli -- sweep --segment 1M:30M:51 --segment 20M:50M:51 2>&1 | tail -2
cargo run -q --release -p vna-cli -- sweep --segment 1M:30M 2>&1 | tail -3
cargo run -q --release -p vna-cli -- tdr --from cal/sweep.s2p --csv - | head -2
```
Verwacht: tests groen; de eerste sweep faalt met "segments overlap: 1.000 MHz–30.000 MHz
and 20.000 MHz–50.000 MHz" (vóór de poort geopend wordt), de tweede met "--segment wants
START:STOP:POINTS[@cal.json]".

**GUI zonder apparaat.** Segments ▸ + segment met een start onder de Stop van het eerste
segment en Apply sweep (vereist een verbinding; zonder apparaat is de knop uit): met
apparaat verschijnt in rood "segments overlap: …" onder de sweepvelden.

**Met apparaat.** Twee segmenten (bijv. 1 – 30 MHz/201 en 30 MHz – 1 GHz/401), Run, TDR:
de regel "TDR and gate use the widest segment: 30.000 MHz → 1.000000 GHz" staat onder
de TDR-instellingen; Gate aan met 0,2 – 0,7 m: gedrag als in 0.2.5. CLI:
`vna-cli sweep --start 50k --stop 3G --points 1001 --gate 0.2:0.7 --vf 0.7` meldt dezelfde
gate als voorheen.

## Punt 26 — Meldingen op hun plek (U6, U11)

**Zonder apparaat.**
1. Maak een .s1p met ongelijke stappen:
   ```bash
   printf '# MHz S RI R 50\n1 0.5 0\n2 0.5 0\n4 0.5 0\n8 0.5 0\n16 0.5 0\n32 0.5 0\n64 0.5 0\n128 0.5 0\n' > /tmp/qc-log.s1p
   ```
   File ▸ Open as trace, kies TDR: in het middenvlak staat oranje "No TDR for this sweep:
   TDR needs a uniformly spaced sweep. …"; de topbalk blijft leeg. Terug naar Frequency:
   de trace is gewoon zichtbaar.
2. Open daarna een bestand met gelijke stappen: de TDR verschijnt weer.
3. View ▸ Developer tools, Developer ▸ Copy diagnostics: rechts in de topbalk staat
   ongeveer zes seconden groen "diagnostics copied to the clipboard" en verdwijnt dan.
4. File ▸ Load calibration met een bestand dat geen calibratie is (bijv. `Cargo.toml`):
   rode fout rechtsboven; maak het venster smal: de tekst wordt afgekapt met "…" en de
   muis erboven toont de hele melding; × haalt hem weg.

## Punt 31 — Minder werk per frame (D9)

**Zonder apparaat.** Open een .s2p met 1001 of meer punten: markers plaatsen, **min SWR**,
**max S21**, **Hold** en de markertabel werken als voorheen. Laad een calibratie met
alleen open en short (of meet ze): het paneel zegt "incomplete: still to measure load";
met alle drie "ready for S11", met through erbij "ready for S11 + S21".

**Met apparaat.** 3201 punten continu, Sweep avg 16, vier referenties: de GUI blijft
vloeiend (menu's openen direct). Activity Monitor: het CPU-gebruik van vna-gui in rust
(geen sweep, muis stil) is vrijwel nul.

## Punt 24 — app.rs gesplitst (D2)

**Veranderd.** Alleen de plaats van de code: `crates/vna-gui/src/app/` met `mod.rs`
(toestand, topbalk, menubalk, zijpaneel, events) en de submodules `settings`, `sweep`,
`calibration`, `plots`, `tdr`, `files`, `diagnostics`. Geen gedrag gewijzigd.

**Zonder apparaat.**
```bash
ls crates/vna-gui/src/app/ && wc -l crates/vna-gui/src/app/*.rs
cargo test -p vna-gui
```
Verwacht: acht bestanden, het grootste (`mod.rs`) rond 900 regels; drie tests groen.

Dit punt is de reden om na afloop **de hele GUI één keer door te lopen**; zie de
slotronde onderaan dit document.

## Punt 49 — Kleine UX-punten (U7, U9, U13)

**Zonder apparaat.**
```bash
cargo run -q --release -p vna-cli -- sweep --segment 1M:30M:51 --start 2M 2>&1 | head -3
cargo run -q --release -p vna-cli -- tdr --from cal/sweep.s2p --points 101 2>&1 | head -3
```
Verwacht: beide een clap-fout "the argument '--segment …' cannot be used with '--start
<START>'" resp. "'--from <FILE>' cannot be used with '--points <POINTS>'".

**Met apparaat.** Verbind, typ in Start `2M` en druk Return: de sweep wordt toegepast (de
weergave begint bij 2 MHz) zonder de knop. Zonder verbinding doet Return niets. Hover
boven **Apply sweep** en boven **Clear** in het calibratiepaneel toont de uitleg.

---

# Later — features

## Punt 34 — `--json` (N4)

**Zonder apparaat.**
```bash
vna-cli --json list
vna-cli --json tdr --from cal/sweep.s2p --max-distance 50 | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['peaks'][0], len(d['trace']))"
vna-cli --json cal show --file cal/n-6g3.cal.json | head -12
```
Verwacht: geldige JSON; `list` een array met per poort `path`, `vna` (LiteVNA/NanoVNA of
null) en `usb`; `tdr` met `peaks` en `trace`; `cal show` met `points`, `measured`,
`missing`, `kit`.

**Met apparaat.**
```bash
vna-cli --json info
vna-cli --json sweep --start 1M --stop 30M --points 11 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['device'], d['calibrated'], len(d['points']), d['points'][0])"
vna-cli --json sweep --start 1M --stop 30M --points 11 --csv -
```
Verwacht: `info` met `model`, `firmware`, `battery_mv`, `range_hz`; de sweep 11 punten met
`freq_hz`, `s11`, `s21`; de laatste regel weigert ("--json and --csv - both write to
stdout").

## Punt 40 — .s1p-export en CSV-kolommen (N15, N16)

**Zonder apparaat.**
```bash
cargo test -p litevna s1p_round_trip
```
GUI: open `cal/sweep.s2p` als trace, Save Touchstone…, geef de naam `/tmp/qc.s1p`:
`head -4 /tmp/qc.s1p` toont de kop `# Hz S RI R 50` en regels met drie getallen. Open
`/tmp/qc.s1p` weer als trace: S11 identiek, S21 leeg (−200 dB). Opslaan als `.s2p` geeft
negen kolommen zoals voorheen.

**Met apparaat.**
```bash
vna-cli sweep --start 1M --stop 30M --points 51 --s1p /tmp/qc-cli.s1p --csv /tmp/qc.csv
head -1 /tmp/qc.csv
```
Verwacht: de CSV-kop eindigt op `…,s21_deg,r_ohm,x_ohm,z_mag_ohm,s21_group_delay_ns`; met
een 50 Ω-load op poort 1 staat `r_ohm` rond 50 en `x_ohm` rond 0 (na calibratie). Let op
voor bestaande scripts: de eerste tien kolommen zijn ongewijzigd, er staan er vier achter.

## Punt 38 — Sneltoetsen, batterij, verbinden bij start (N7–N9, N11)

**Zonder apparaat.** Open een .s2p als trace en klik in een grafiek (zodat geen veld de
focus heeft): `1`…`4` wisselen de actieve marker (vet in de markerbalk), `H` maakt een
referentie, `G` zet Gate aan/uit (vinkje in het zijpaneel), `T` wisselt naar TDR en
terug. In TDR: klik een marker en gebruik ← → (shift: tien stappen). Klik in het veld
**Start** en typ `hgt1`: er gebeurt niets behalve dat de tekst in het veld komt.

**Met apparaat.** Spatie start Run, nog een keer stopt; `S` doet één sweep (alleen als
Run uit staat). Laat de GUI vijf minuten verbonden staan: de spanning in de topbalk
verandert mee (vergelijk met `vna-cli info`). Zet View ▸ Connect at start aan, sluit af,
start opnieuw met het apparaat aangesloten: verbindt vanzelf. Start zonder apparaat:
geen foutmelding, de poortlijst wacht. Zet de optie weer uit als je hem niet wilt.

## Punt 35 — Poortverlenging (N5)

**Zonder apparaat.**
```bash
cargo test -p litevna port_extension
vna-cli tdr --from cal/sweep.s2p | sed -n 3,4p
vna-cli tdr --from cal/sweep.s2p --port-extension 100000 | sed -n 4,5p
```
Verwacht: test groen; de eerste reflectie schuift van 336,8 ns naar 137,9 ns (2 × 100 ns
eerder). GUI: open `cal/sweep.s2p`, TDR, zet Port extension op 100000 ps: de piek schuift
14,5 m naar links; de regel eronder toont "≈ 20985 mm at VF 0.70"; knop **0** zet hem
uit en de piek staat terug.

**Met apparaat (LiteVNA, gecalibreerd).** Schroef een verloopstuk of kort kabeltje op de
poort met een short aan het eind. Smith-chart: een boog. Draai Port extension omhoog tot
de boog een punt links is (bijv. enkele tientallen tot honderden ps): fase S11 is dan
±180° vlak. Controleer met TDR: de short staat op 0 m. Sla een .s2p op: het commentaar
bevat "port extension … ps". Sluit af en start opnieuw: de waarde is onthouden. Meet
daarna een standaard (open): de calibratie zelf is niet beïnvloed (het paneel toont
dezelfde punten, en met extension 0 is alles als voorheen).

## Punt 36 — Referenties blijven bewaard (N12)

**Zonder apparaat.**
```bash
cargo test -p vna-gui references_survive
```
GUI: open `cal/sweep.s2p` als trace, **Hold** (referentie 1), hernoem hem naar "voor",
File ▸ Open as reference… met een ander bestand (referentie 2), zet Δ bij referentie 1
aan en vink referentie 2 uit. Sluit de GUI af (File ▸ Quit). Controleer:
```bash
ls ~/Library/Application\ Support/vna-r/references/
```
Verwacht: `ref-0.s2p` en `ref-1.s2p`. Start opnieuw: onder **Traces** staan beide
referenties met hun naam, referentie 2 uitgevinkt, Δ bij "voor" actief. Verwijder
referentie 1 met ×, sluit af: `ref-1.s2p` is weg en `ref-0.s2p` bevat de overgebleven
(voormalige tweede) referentie. Verwijder de laatste: de map is leeg.

Let op: bij een autosave (elke 30 s) worden de bestanden alleen herschreven als er iets
aan de referenties veranderd is.

