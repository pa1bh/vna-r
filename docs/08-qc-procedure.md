# QC-procedure werklijst november 2026

Stand: 13 november 2026: **0.3.4** is uitgebracht (acht kleine punten na NanoVNA-Saver,
gedeelde grafiek-as met reset view, sweep from calibration en de browserbouw), na **0.3.3**
(wachten bij een verbinding die komt en gaat, in plaats van opgeven), **0.3.2** (waarschuwing bij een sprong in de calibratie,
en opnieuw verbinden na uitval) en **0.3.1** (0.3.0 plus View ▸ Zoom); zie
het logboek. **0.3.0** kwam na de release candidates 0.3.0-rc.1
(vóór de QC, om het testen op andere platforms makkelijker te maken) en 0.3.0-rc.2 (na
het eerste deel van de QC op macOS met de LiteVNA 67), en na een ronde met de NanoVNA-F
op NixOS en macOS. Wat getest is, wat gevonden is en wat nog openstaat, staat in het
logboek hieronder. Per werklijstpunt (nummers uit [docs/05-roadmap.md](05-roadmap.md), codes uit
[docs/07-analyse-en-aanbevelingen.md](07-analyse-en-aanbevelingen.md)): wat er veranderd
is, hoe je het controleert en wat je moet zien.

Wat ik zelf al gecontroleerd heb: `cargo fmt`, `cargo clippy -D warnings` en alle tests
(85 in de library, 4 in de GUI) na elk punt; de release-build; de CLI-voorbeelden die
geen apparaat nodig hebben; en dat de GUI met jouw bestaande instellingen van 0.2.5
start en tien seconden draait zonder fout. **Niet gecontroleerd**: alles met een
apparaat, alles wat je moet zien (de GUI heb ik niet kunnen bekijken), Linux, Windows en
het gedrag van de release-archieven op andere machines.

## Voorbereiding

```bash
cargo build --workspace --release
export PATH="$PWD/target/release:$PATH"
cp ~/Library/Application\ Support/vna-r/app.ron /tmp/app.ron.0.2.5
```

Op een andere machine neem je in plaats van de eerste twee regels het archief van de
release: <https://github.com/pa1bh/vna-r/releases/tag/v0.3.4>, uitpakken
zoals in hoofdstuk 2 van de handleiding en de map in je PATH zetten.

De eerste regel bouwt wat je test; de tweede maakt dat `vna-cli` en `vna-gui` in de
voorbeelden hieronder de nieuwe versie zijn; de derde bewaart je instellingen van 0.2.5
(de nieuwe versie schrijft ze in een vorm die 0.2.5 niet meer leest, zie punt 25).

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && ./scripts/check-manuals.sh
```

Verwacht: geen uitvoer van fmt en clippy, alle tests groen, "manuals in step: 10 files
each".

```bash
vna-gui
```

Verwacht: de GUI start met je oude instellingen, de titelbalk toont de versie, Help ▸
About toont versie en git-revisie (`v0.3.0`, na latere commits gevolgd door het aantal commits sinds de tag en de hash).

## Volgorde: vier rondes

Loop de punten niet op nummer af maar per opstelling. In de tabel staat per punt wat je
ervoor nodig hebt; binnen een punt staan de stappen onder **Zonder apparaat**, **Met
apparaat**, enzovoort.

1. **Ronde A, zonder apparaat** (ongeveer een uur): van elk punt het deel "Zonder
   apparaat". Dit zijn commando's met een verwachte uitvoer en handelingen in de GUI
   met bestanden uit `cal/`.
2. **Ronde B, LiteVNA met N-kit**: de delen "Met apparaat" en "Met LiteVNA".
3. **Ronde C, NanoVNA-F**: de delen "Met NanoVNA-F"; de punten 11 en 22 zijn hier het
   belangrijkst.
4. **Ronde D, Linux en CI**: punt 15 en 16 op de NixOS-laptop, en na een push de CI
   (punt 16 en 29).

Sluit af met de **slotronde** onderaan: de hele GUI één keer van begin tot eind, omdat
`app.rs` gesplitst is (punt 24) en veel punten dezelfde panelen raken.

| Punt | Onderwerp | Zonder apparaat | LiteVNA | NanoVNA-F | Linux/CI |
|---|---|---|---|---|---|
| 1 | Polaire calibratie-interpolatie | test | ja, belangrijk | | |
| 2 | TDR-bereik | ja | | | |
| 3 | Shell-open wacht niet dubbel | code | | | andere seriële poort |
| 4 | Calibratiemeldingen blijven staan | ja | ja | | |
| 5 | Standaarden op het toegepaste raster | | ja, belangrijk | ja | |
| 6 | Screenshot stopt de sweep niet | code | ja | | |
| 7 | Markers op frequentie | ja | ja | | |
| 8 | Pijltjestoetsen buiten tekstvelden | ja | | | |
| 9 | CLI-uitvoer | ja | ja | ja | |
| 10 | Schermafdruk-header begrensd | | ja | | |
| 11 | Apparaatgrens in TDR-sweep en segmenten | | ja | ja, belangrijk | |
| 12 | Geen ruwe records is een lege lijst | test | | | |
| 13 | Gemiddelde standaarden, bestandslabels, instellingen clampen | ja | ja | | |
| 14 | Underflow en eindeloze lus | test | | | |
| 15 | udev-regel | | | | ja |
| 16 | Documentatie en flake zonder GTK | ja | | | ja, CI |
| 17 | CHANGELOG Engels | lezen | | | |
| 18 | Ballast en UI-teksten | ja | | | |
| 19 | Fabrikantbestanden uit git | ja | | | |
| 20 | Git-revisie volgt commits | ja | | | |
| 21 | Drivertests; `open` en sweep-lus aangeraakt | test | ja, regressie | ja, regressie | |
| 22 | Stop per blok, voortgangsbalk | test | ja | ja, belangrijk | |
| 23 | Gedeelde regels in de library | ja | ja | | |
| 24 | `app.rs` gesplitst | ja | slotronde | | |
| 25 | Formaatversies | ja, belangrijk | | | |
| 26 | Meldingen op hun plek | ja | | | |
| 27 | Fouttype, "apparaat weg" | ja | ja, kabel lostrekken | ja | |
| 29 | CI, clippy, MSRV, handleidingcheck | ja | | | ja, CI |
| 30 | Dode code en commentaren | ja | | optioneel | |
| 31 | Minder werk per frame | ja | ja | | |
| 32 | Eén Z0, dB-vloer, SWR-plafond | ja | | | |
| 33 | CONTRIBUTING.md | lezen | | | |
| 34 | `--json` | ja | ja | | |
| 35 | Poortverlenging | ja | ja | | |
| 36 | Referenties bewaard | ja | | | |
| 38 | Sneltoetsen, batterij, verbinden bij start | ja | ja | | |
| 40 | `.s1p` en CSV-kolommen | ja | ja | | |
| 41 | `cal`-acties, completions | ja | ja | | |
| 48 | Niet-eindige waarden, dubbele frequenties | ja | | | |
| 49 | Return past toe, CLI weigert tegenstrijdige vlaggen | ja | ja | | |

Niet gedaan en dus niet te controleren: 28 (vervallen, de Help-links blijven bij de tag),
37, 39, 42, 43, 44 en de meetpunten 45 tot 47; de reden staat per punt in de werklijst.

## Logboek van de QC

### 10 november 2026, macOS, LiteVNA 67 (firmware 2.2, hw 2)

**Door Claude gedraaid, met het apparaat aan deze Mac en poort 1 open.** Alle
commando's van ronde A (zonder apparaat) en de CLI-delen van ronde B tegen de
release-build: uitkomst zoals beschreven, tenzij hieronder genoemd.

| Punt | Wat | Uitkomst |
|---|---|---|
| voorbereiding | fmt, clippy, tests (85 + 4), handleidingcheck | goed |
| 2, 9, 16, 18, 19, 20, 23, 27, 29, 30, 32, 34, 35, 41, 48, 49 | delen zonder apparaat (CLI) | goed |
| 21 | `info`; sweep 1–30 MHz/201 en 50 kHz–6,3 GHz/1001 (2,8 s, 0 ontbrekend, 0 dubbel, checksum 1001/1001) | goed |
| 10, 21 | `screenshot`: 1024×600, 16 bpp, geldige PNG | goed |
| 9 | geen "first raw record" zonder `--trace`, wel met; commentaar noemt model en calibratie | goed |
| 34 | `--json info`, `--json sweep`, weigering met `--csv -` | goed |
| 40 | `--s1p` en CSV met vier extra kolommen | goed |
| 41 | `cal open/short/load` en alias `o` (alleen de mechaniek, geen echte standaarden) | goed |
| 23 | `--gate 0.2:0.7` meldt de gate | goed |
| 35 | `--port-extension` | **bevinding 1** |

**Bevindingen.**

1. *CLI-bestandscommentaar noemde poortverlenging en gate niet* (en `--s1p` de calibratie
   niet), terwijl de handleiding dat belooft en de GUI het doet. Gerepareerd in commit
   `b0ba881`; gecontroleerd met het apparaat: `! LiteVNA sweep, 201 points, host
   calibration cal/n-6g3.cal.json, port extension 100.0 ps, S11 time-gated 0.20..0.70 m
   band pass`.
2. *Dit document*: de schermafdruk van de 67 is 1024×600 (stond 800×480), de foutmelding
   bij een onvolledig segment luidt anders dan hier stond, en de `sed`-regels bij punt 35
   telden de stderr-regel mee. Verbeterd.

**Door PA1BH in de GUI, met Claude als loods** (revisie `v0.3.0-rc.1-2-g2bda786`).

| Blok | Punten | Uitkomst |
|---|---|---|
| 1: starten, verbinden, eerste sweep | 25 (instellingen van 0.2.5 gelezen, `version:1` geschreven), 38 (spatie, S), 49 (Return past toe), 7 (marker op frequentie), 8 (pijltjes in tekstveld) | goed |
| 2: calibreren 1–50 MHz, 201 punten, vervangende load | 5 (standaard past getypt raster toe), 4 (meldingen blijven staan), 13 (gemiddelde standaard), 18 (voorgestelde naam `1MHz-50MHz.cal.json`), 49 (uitleg bij Clear) | goed |

Bij blok 2, laatste stap: na **Load…** van `cal/n-1g.cal.json` op de sweep 1–50 MHz staat
er geel "loaded" en oranje "⚠ 20 sweep points lie between the first two calibration
points …". Dat is juist: die calibratie dekt 1–50 MHz en wordt toegepast, maar haar
tweede punt ligt op 5,995 MHz. De regel "not applied" komt pas bij een sweep buiten
1 MHz–1 GHz; punt 4 hieronder beschrijft nu beide.

Het logboek van de GUI toont om 16:15:53 "sweep: I/O error: Broken pipe" en twee seconden
later een nieuwe Connect: PA1BH zat op dat moment aan de USB-kabel. Daarmee is punt 27
ongepland gezien: fout in de topbalk, knop terug op Connect, opnieuw verbinden werkt.

3. *Geen Stop-knop bij een standaardmeting of enkele sweep als Run uit staat* (blok 3,
   stap 5: short op 3201 punten met Sweep avg 8, de balk liep, maar er stond Run in
   plaats van Stop). Afbreken kon alleen door Run te starten. Gerepareerd in commit
   `ce59d33`: knop, Device-menu en spatiebalk kijken nu of er iets gemeten wordt. Nog door
   PA1BH te bevestigen.
4. *Geen fout, wel genoteerd*: met `n-6g3` (1001 punten, tweede punt op 6,35 MHz) en een
   sweep van 3201 punten staat de short onder 6,35 MHz tot ongeveer +8 dB te hoog en
   buiten de Smith-cirkel. De oranje waarschuwing zegt dat ook. Nagerekend met de dichte
   HF-calibratie `n-hf` als waarheid: de ruwe short gaat tussen 50 kHz en 1 MHz van
   0,22∠−70° naar 0,70∠−152°, dat volgt geen enkele interpolatie tussen twee punten
   (polair tot +9,4 dB, cartesisch zoals in 0.2.5 tot +6,8 dB). Mogelijke verbetering
   voor later: die sweeppunten niet tekenen, of een calibratieraster dat onderin dichter
   is.

**Afsluiting van deze ronde.** PA1BH heeft blok 3 afgemaakt met de gerepareerde build
(revisie `v0.3.0-rc.1-5-ga4e502f`) en meldde dat alles in orde is; het logboek van de GUI
toont de bewust losgetrokken kabel om 16:50:36 en de nieuwe Connect zeven seconden later.
De uitkomst van de afzonderlijke stappen (grove calibratie binnen ±0,2 dB, Stop bij een
standaard, screenshot tijdens Run) is niet per stap gemeld.

5. *README en handleidingen nagelopen* tegen de code en tegen elkaar (Engels en
   Nederlands, alinea voor alinea). De afwijkingen staan in de changelog onder
   0.3.0-rc.2, Documentation. Niet op te lossen zonder apparaat en antenne: de
   schermafbeeldingen zijn van 0.2.3 en 0.2.4 (werklijst punt 50).

Daarna uitgebracht als **0.3.0-rc.2**.

**Nog niet gedaan met deze versie:** in de GUI de referenties, TDR, gate en bestanden
(blok 4) en de slotronde; ronde C (NanoVNA-F, vooral punt 11 en 22); ronde D (Linux,
punt 15 en 16 op de laptop); Windows.

### 11 november 2026: NanoVNA-F op NixOS en macOS (0.3.0-rc.2)

PA1BH testte de NanoVNA-F met de GUI op de NixOS-laptop (flake, tag `v0.3.0-rc.2`) en
meldde dat bijna alles werkt; welke stappen precies zijn gedaan is niet per stap gemeld.

6. *Through op NixOS gaf "median raw |S21| is -65.8 dB"* (50 MHz–1,2 GHz, 801 punten),
   ook met een tweede kabel. Dat is het niveau van de lek zonder verbinding tussen de
   poorten. Dezelfde calibratie op de Mac gaf geen waarschuwing, en een ruwe sweep daar
   (`vna-cli sweep --start 50M --stop 1200M --points 11`, kabel tussen de poorten) geeft
   S21 van −1,35 dB bij 50 MHz tot −2,86 dB bij 855 MHz met een gelijkmatig draaiende
   fase. De meetcode heeft geen platformafhankelijke delen.
   *Oorzaak gevonden met het apparaat aan de Mac*: het ligt aan de firmware, niet aan
   het platform, de kabel of ModemManager (gestopt en opnieuw ingeplugd: geen verschil;
   `AT` sturen: geen effect). Nadat het apparaat een tijd zelf heeft gesweept, verliest
   een scan die boven 280 MHz komt de through, en elke volgende ook; één scan onder
   280 MHz herstelt het. Dat het op de Mac eerst goed ging was toeval: daar was de
   eerste sweep kennelijk een lage. Details in `docs/06-nanovna-shell-protocol.md`.
   Gerepareerd: vna-r doet vóór de eerste scan van een sessie een scan van twee punten
   op 10 en 20 MHz. Gecontroleerd op de Mac: zonder die scan −64 dB na 25 s eigen sweep
   (4 van 4 keer), met de nieuwe build −1,35 dB (4 van 4 keer, ook vanuit de verloren
   toestand). Bevestigd door PA1BH op NixOS met de CLI (revisie `c02976e`): na de scan op
   10 en 20 MHz geeft 50 MHz–1,2 GHz S21 van −1,35 tot −2,86 dB, gelijk aan de Mac. Nog
   te doen: een through-calibratie in de GUI.
   PA1BH meldde daarna dat het werkt en dat metingen aan een filter, verzwakkers en
   antennes met de NanoVNA-F op NixOS er goed uitzien.
7. *`vna-cli sweep` op NixOS: `protocol error: unexpected sweep reply "\nch> sweep\r\n1000000
   30000000 301"`* (poort `/dev/ttyACM1`, direct na het openen). Een prompt van eerder
   stond vóór de echo van het eerste commando; het lezen van tekstantwoorden kon daar niet
   tegen. Nagebootst in een test met de nagebootste poort (zelfde foutmelding) en
   gerepareerd: na het openen wachten tot de lijn stil is, en elk antwoord lezen vanaf de
   echo van het eigen commando (ook bij `scan_bin` en `capture`). Tests 86 + 4, clippy en
   fmt schoon. **Niet met het apparaat gecontroleerd**, op geen van beide platforms; of dit
   met bevinding 6 samenhangt is onbekend.
8. *Bestandsdialogen doen niets op NixOS* (punt 15/16): **Save…** van een calibratie gaf
   geen dialoog; in het logboek "Can't connect to a portal: libdbus-1.so not found",
   daarna een mislukte terugval op zenity. rfd 0.17 laadt `libdbus-1.so.3` tijdens het
   draaien en de flake zette die niet op het zoekpad; dat is zo sinds 0.2.0, de
   dialogen waren op NixOS nooit geprobeerd. Gerepareerd in `flake.nix` (pakket en
   ontwikkelshell). **Niet gecontroleerd**: hier is geen Nix; de CI bouwt de flake maar
   opent geen dialoog.
9. *Open: S11 in de GUI een keer "all over the place"* (NixOS, NanoVNA-F, revisie
   `b09c7c7`, 12 november). Geen schermafbeelding, bereik en calibratietoestand onbekend.
   Ruwe sweeps met de CLI direct daarna waren goed, op 1–30 MHz en op 50 MHz–1,2 GHz:
   open −3,4 dB ∠0° (bovenin zakkend naar −8 dB), load −42 tot −25 dB, short −0,9 dB
   ∠180°. Daarna werkte de GUI weer, zonder dat duidelijk is wat er veranderde. Oorzaak
   onbekend; bij herhaling eerst een schermafbeelding met het linkerpaneel maken.

**Uitgebracht als 0.3.0** op 12 november 2026, op verzoek van PA1BH: verder testen komt
in een nieuwe ronde. Bij de release niet bevestigd: de bestandsdialogen op NixOS na
bevinding 8; onverklaard: bevinding 9; nog steeds niet met een apparaat geprobeerd:
Windows; met de LiteVNA in de GUI niet stap voor stap doorlopen: referenties, TDR, gate
en bestanden (blok 4) en de slotronde.

**Uitgebracht als 0.3.1** op 12 november 2026. PA1BH kondigde 0.3.0 aan in de groep
liteVNA op groups.io; de eerste reactie, van een gebruiker op Windows 11, was dat de tekst
te klein is. Zoomen zat al in de GUI (Ctrl of ⌘ met +, − en 0) maar stond nergens; 0.3.1
zet **View ▸ Zoom** in het menu en in beide handleidingen. PA1BH bekeek het menu op
macOS; dat de gekozen grootte wordt weggeschreven is gezien in `app.ron`, dat ze bij een
nieuwe start terugkomt is niet gemeld. Niet geprobeerd op Linux.

Dezelfde dag probeerde PA1BH de GUI op Windows, zonder apparaat: starten, de menu's en
het zoomen werken. Het is de eerste keer dat de Windows-build is gestart; verbinden,
sweepen en calibreren zijn daar nog steeds niet geprobeerd.

**Na 0.3.1, 12 november 2026: een zaagtand bij 100 MHz.** PA1BH mat met de LiteVNA 67 een
open N-naar-BNC-verloopstuk over 50–150 MHz met de calibratie `n-1g` (1 MHz–1 GHz, 201
punten) en zag R + jX en |Z| vanaf 96 MHz weglopen en op precies 100 MHz terugvallen.
Oorzaak: de calibratie wordt geïnterpoleerd over de frequentie waarop het apparaat van
signaalbron wisselt (zie "Valkuil: de bandgrens bij 100 MHz" in
[docs/03-calibratie.md](03-calibratie.md)). Gerepareerd met een waarschuwing, niet met
een andere interpolatie: CLI en GUI melden het als een sweep punten heeft tussen twee
calibratiepunten met een sprong ertussen, en `vna-cli cal show` toont de sprongen.
Gecontroleerd met de tests en met `cal show` op de zes bestanden in `cal/` (`n-1g` en
`n-6g3` melden de sprong, de andere vier niets). Niet gezien: de waarschuwing in de GUI
en bij een sweep met het apparaat.

**Na 0.3.1, 12 november 2026: "I/O error: Broken pipe" in de GUI.** PA1BH kreeg met de
LiteVNA 67 op de Mac af en toe deze fout; soms hielp de GUI herstarten, soms alleen de
USB-C-kabel opnieuw insteken. De systeemlog van macOS (`/usr/bin/log show` met
`eventMessage CONTAINS "LiteVNA6"`) toont dat het apparaat om 21:18:02, 21:18:16 en
21:18:21 telkens 1,4 tot 2,4 s van de USB-bus verdween; de fout van 21:18:20 is dus een
echte verbindingsuitval. Niet op te roepen zonder het apparaat aan te raken: 120 losse
CLI-sweeps, 150 s aaneengesloten sweepen, het lezen 8 en 20 s stilleggen midden in een
sweep, de poort 4 minuten open, en 200 s vrijgegeven met elke 10 s een batterij-uitlezing
gaven nul fouten en nul uitval. Oorzaak van de uitval zelf niet vastgesteld (vermoeden:
de USB-C-stekker bij het wisselen van loads, of een reset van het apparaat). Wel
gerepareerd is hoe de GUI erop reageert: de werker merkt de uitval op (ook in rust),
wacht tot 30 s op de poort, verbindt opnieuw en gaat verder met Run of de enkele sweep.
Gecontroleerd: de tests, en `hardware_session` (genegeerde test in `worker.rs`) met het
apparaat onaangeroerd: 18 sweeps, stop, rust, enkele sweep, geen valse uitval. Daarna
met `VNA_R_EXPECT_DROP=1` terwijl PA1BH de kabel lostrok en terugstak: de systeemlog
toont het apparaat 1,6 s en daarna 8,2 s van de bus; de werker meldde beide keren
`sweep: I/O error: Broken pipe` (dezelfde fout als in de GUI), was 0,4 en 0,5 s na
terugkomst van de poort opnieuw verbonden en deed daarna nog 24 sweeps. Niet gezien: de
oranje balk en de melding in de GUI zelf.

PA1BH trok de kabel daarbij één keer los. De eerste uitval (1,6 s) viel dus bij het
vastpakken van de stekker, vóór het lostrekken, en is even kort als de drie uitvallen van
21:18. Dat maakt een mechanische oorzaak aannemelijk: het contact van de USB-C-stekker,
in de kabel of in de bus van het apparaat, verdraagt geen aanraking. Welke van de twee
is niet onderzocht; een andere kabel is de eerste proef.

**Uitgebracht als 0.3.2** op 12 november 2026, met de twee bevindingen hierboven. Bij de
release niet gezien: de waarschuwing voor de sprong en de oranje balk bij uitval in het
venster van de GUI zelf; het opnieuw verbinden op Linux en Windows.

**Na 0.3.2, 12 november 2026: opgeven na vier keer.** PA1BH probeerde 0.3.2 in de GUI en
raakte alleen de N-connector aan, niet de USB-C-kabel: de GUI verbond opnieuw, en na een
paar keer meldde hij "the connection keeps dropping; not reconnecting again". De
systeemlog toont wat er gebeurt terwijl een hand aan de connector zit: het apparaat komt
en gaat in reeksen, tot veertien keer in 27 s (23:05:09–23:05:36), telkens 1,4 tot 1,6 s
weg en 0,1 tot 1,2 s terug. De regel uit 0.3.2 (opgeven na vier keer uitval op rij) is
daar te streng voor. Vervangen door: de poort moet eerst een seconde gebleven zijn voor
hij weer geopend wordt, bij elke uitval op rij langer (2, 4, hooguit 8 s), en er wordt
niet meer opgegeven zolang de poort terugkomt; wel na 30 s zonder poort. Gecontroleerd
met de tests; **niet met het apparaat**, want de GUI van PA1BH had de poort. Over de
oorzaak: uitval bij het aanraken van de N-connector zonder de kabel aan te raken past
bij een USB-C-contact dat beweegt als het apparaat beweegt, maar sluit iets elektrisch
(aanraken van de massa van de poort) niet uit. Proef: het apparaat vasthouden zodat de
stekker niet kan bewegen en dan de connector aanraken.

**Oorzaak gevonden: de USB-kabel.** Kort daarna meldde het apparaat zich helemaal niet
meer (macOS zag het niet op de bus, `vna-cli list` toonde het terecht niet). Met een
andere kabel is de uitval weg, ook bij het draaien aan de N-connector. PA1BH probeerde
daarna in de GUI met de nieuwe wachtregel: kabel eruit en erin, de verbinding kwam netjes
terug; en de oranje waarschuwing voor de sprong in de calibratie staat er zoals bedoeld.
Daarmee zijn de oranje balk bij uitval en de waarschuwing in het venster zelf gezien, op
macOS. Niet geprobeerd: een reeks van snel komen en gaan met de nieuwe wachtregel (die
deed zich met de goede kabel niet meer voor), en Linux en Windows.

**Uitgebracht als 0.3.3** op 12 november 2026, met de nieuwe wachtregel.

Na de release-commit probeerde PA1BH met dezelfde code nog de lange uitval: kabel eruit
terwijl er niet gemeten werd. De GUI merkte het zonder sweep op ("connection: the device
left the USB bus", de controle in rust) en meldde na 30 s "the device on … did not come
back within 30 s", met de knop terug op Connect. Daarmee zijn in het venster gezien: kort
lostrekken met herverbinden (7,6 s), uitval in rust, en opgeven na 30 s.

### 13 november 2026, macOS, LiteVNA 67: ronde na NanoVNA-Saver (punten 53–60)

**Door Claude gedraaid**, zonder apparaat: fmt, clippy, tests (99 + 8), `check-manuals.sh`
en de browserbouw na elk punt; in de ingebouwde browser met het voorbeeldbestand gezien:
de kolom L / C, de banden als achtergrond en de vensterlijst met kaiser 6 en 13.

**Door PA1BH in de GUI**, met Claude als loods; release-build vanuit de sessie gestart
met de bestaande instellingen van 0.3.3.

| Blok | Wat | Uitkomst |
|---|---|---|
| 1, zonder apparaat | 53, 54, 57, 58, 59 met `cal/sweep.s2p`; afsluiten en opnieuw starten | goed: "alles in orde"; in `app.ron` daarna Amateur bands aan, kaiser 13, VF 0,82, offset 0, drop 0 |
| 2, LiteVNA 67 (fw 2.2, hw 2) | 55: load op poort 1, Sweep avg 8 met drop 0 en 2, tik tegen de connector | goed |
| 2 | 56: 50 k–6,3 G, twee segmenten, **log spacing** | goed: 50 k → 2,507 M → 125,66 M → 6,3 G, 1001 punten per segment blijven; **bevinding 10** |
| 2 | 56: Apply sweep en Single met short, ruw en met `n-6g3` | goed: één doorlopende trace over de drie segmenten, marker op 106 en 360 MHz zonder sprong; **bevinding 11** |
| 2 | slotronde: segmenten weg, TDR met hann, kaiser 6 en 13, afsluiten en starten | goed; in `app.ron` segmenten leeg, avg 1, drop 0, banden aan |
| 3, later die avond | 69: gedeelde as, **reset view**, ook in de TDR-weergave met gate; 70: **sweep from calibration** met `n-1g` en `n-hf` aan de LiteVNA | goed: "alles in orde" |
| 4, na 0.3.4 | 71: **Band** ▸ 40 m in regio 1 en 2, lijst en achtergrond van regio 2, Band ▸ 20 m met Run aan, regio bewaard na herstart | goed: "alles in orde" |

**Bevindingen.**

10. *De knop log spacing was niet te vinden.* Hij stond alleen in het paneel **Segments**
    en pas na het eerste **+ segment**; PA1BH zocht iets met "log" in de sweepregel.
    Gerepareerd in commit `2705f4a`: de knop staat er altijd naast **+ segment**, grijs
    met uitleg zolang er één segment is; beide handleidingen zeggen het.
11. *Een bult aan het begin van de sweep met `n-6g3` en een fijn eerste segment*: punt 51
    opnieuw, in een andere gedaante. Het eerste segment (50 k–2,5 M) valt helemaal in het
    eerste interval van die calibratie (stap 6,3 MHz) en wordt tussen twee punten
    geïnterpoleerd; met een short loopt de trace in de Smith-kaart buiten de cirkel en
    staat S11 boven 0 dB. Geen fout in de segmenten: zonder calibratie en met de marker
    over de naden is het beeld goed. Werkwijze: `n-hf` (100 kHz–50 MHz) in het paneel voor
    het eerste segment en `n-6g3` als bestand bij segment 2 en 3, start op 100 k. Niet
    gerepareerd; werklijst 68 vraagt om een waarschuwing in het segmentenpaneel als de
    calibratiestap grover is dan het segment.

Geen bevinding, wel gezien: de referentie `hf-ocf-3-20-NanoVNA-F.s2p` (3–20 MHz) is op een
as tot 6,3 GHz een verticale veeg bij 0; en een grafiek die met het scrollwiel tot 10 GHz
is uitgezoomd blijft zo staan tot een dubbelklik.

---

# 0.2.6 — bugfixes

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
Verwacht: tests groen; voor `cal/sweep.s2p` (3 – 30 MHz, 1001 punten, Δf = 27 kHz) meldt
de regel "alias-free range 1943.1 m (18519 ns)": dat is 1/(2·Δf). Vóór 0.2.6 stond daar
het dubbele. Voor 50 kHz – 6,3 GHz in 1001 punten (Δf = 6,3 MHz) hoort er 79 ns, 8,3 m
bij VF 0,70 te staan.

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

**Zonder apparaat.** File ▸ Load calibration met `cal/n-6g3.cal.json`: onder de
calibratieregels staat geel "loaded" en het blijft staan. (Op een geopend bestand wordt
geen calibratie toegepast, dus de oranje regel hieronder zie je alleen met een apparaat.)

**Niet toepasbaar (met apparaat).** Laad `cal/n-1g.cal.json` (1 MHz – 1 GHz, 201 punten).
Op een sweep binnen dat bereik met een fijner raster, bijvoorbeeld 1 – 50 MHz, wordt hij
toegepast en staat er onder het gele "loaded" een oranje waarschuwing "⚠ N sweep points
lie between the first two calibration points …". Sweep daarna 1 M – 2 G: geel "loaded"
blijft staan en daaronder oranje "not applied: target grid … lies outside the calibrated
span …"; het label is RAW. Vóór 0.2.6 verdrong de tweede melding de eerste.

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
vna-cli tdr --from cal/sweep.s2p --csv - 2>/dev/null | head -3
ls -- -
```
Verwacht: drie CSV-regels op stdout (kop `distance_m,time_ns,…`) en `ls: -: No such file`.

**Met apparaat.**
```bash
cargo run --release -p vna-cli -- sweep --start 1M --stop 30M --points 51 --s2p /tmp/qc.s2p 2>&1 | grep -c 'first raw record'
sed -n 2p /tmp/qc.s2p
cargo run --release -p vna-cli -- sweep --segment 1M:30M:51@cal/n-hf.cal.json --s2p /tmp/qc-seg.s2p && sed -n 2p /tmp/qc-seg.s2p
```
Verwacht: `0` (geen debugregel zonder `--trace`); de tweede regel van het bestand noemt het model
(`! LiteVNA sweep, 51 points, uncalibrated raw` of `! NanoVNA-F 4.3 sweep, …`); de
segmentversie zegt `host calibration per segment`. Met `--trace` verschijnt de regel
"first raw record" wel.

## Punt 10 — Schermafdruk-header begrensd (B11)

**Met apparaat.** `vna-cli screenshot --out /tmp/qc.png` op LiteVNA 64 en 67 en de GUI-knop
**Screenshot**: werkt als voorheen (480×320 op de 64, 1024×600 op de 67, 16 bpp). Een negatieve test
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
`/tmp/qc-file.s2p`; `sed -n 2p /tmp/qc-file.s2p` toont `! vna-r, N points, from file
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
referentie: "References are kept between sessions. …" (sinds punt 36).

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
en een standaardmeting: balk loopt tot 25608 punten; Stop breekt binnen een seconde af,
ook als Run uit staat (de knop Run is dan Stop zolang de meting loopt).
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
and 20.000 MHz–50.000 MHz" (vóór de poort geopend wordt), de tweede met "a segment is
START:STOP:POINTS, got "1M:30M"".

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
vna-cli tdr --from cal/sweep.s2p 2>/dev/null | sed -n 3p
vna-cli tdr --from cal/sweep.s2p --port-extension 100000 2>/dev/null | sed -n 3p
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

## Punt 41 — `cal`-acties, completions, opties die genegeerd werden (N17, U14)

**Zonder apparaat.**
```bash
vna-cli cal --help | sed -n 8,22p
vna-cli cal bogus --file x.json
vna-cli cal show --file cal/n-6g3.cal.json --note test
vna-cli cal show --file cal/n-6g3.cal.json --apply
vna-cli cal thru --file cal/n-6g3.cal.json --help > /dev/null && echo "alias thru ok"
vna-cli completions zsh | head -1
vna-cli cal show --file cal/n-6g3.cal.json | head -1
vna-cli tdr --from cal/sweep.s2p --csv - 2>/dev/null | head -2
```
Verwacht: de helptekst somt de twaalf waarden op met uitleg; `bogus` geeft "invalid value
… [possible values: open, short, …]" (exitcode 2); `--note` en `--apply` bij `show` geven
een fout met uitleg; `#compdef vna-cli`; de twee `head`-regels eindigen zonder "Broken
pipe" of paniekmelding; de laatste toont de CSV-kop `distance_m,time_ns,…` als eerste
regel (geen tabel ervoor).

**Met apparaat.** `vna-cli cal open --file /tmp/qc.cal.json --start 1M --stop 30M --points
51`, dan `short` en `load` zonder raster: werkt als voorheen; `vna-cli cal o --file …`
(alias) ook.

---

# Slotronde: de hele GUI één keer door

Met de LiteVNA en de N-kit, in deze volgorde. Elke stap is één regel; noteer wat afwijkt.

1. Start `vna-gui`. Instellingen van de vorige keer staan er; de poortlijst toont de
   LiteVNA met een ster.
2. **Connect**: model, firmware en batterijspanning in de topbalk.
3. Sweep 1 M – 30 M, 201 punten, **Apply sweep**, **Run**: vier grafieken lopen, label
   RAW of CAL.
4. Kies in elk van de drie grafieken een andere weergave (log mag, phase, SWR, R + jX,
   |Z|, group delay, lin mag): alle zeven tekenen.
5. Klik een marker in een grafiek en op de Smith-chart; ← →, shift ← →; **min SWR**,
   **max S21**, **min S21**; tweede marker: de Δ-regel in de tabel verschijnt.
6. Calibratie: **Clear**, meet **open**, **short**, **load** (melding na elk), label CAL;
   **apply** uit en aan; **Save…**, **Load…**.
7. Zet Sweep avg op 8: teller loopt op; **reset**; terug naar 1.
8. **Hold**, verander iets aan de DUT, **Δ** bij de referentie: verschiltrace in elke
   grafiek; referentie hernoemen, uitvinken, verwijderen.
9. Segments: **+ segment** 30 M – 1 G, 401 punten, Apply: één trace over beide; kies een
   calibratiebestand voor het tweede segment (`cal/n-1g.cal.json`), Apply: label CAL of
   CAL* met uitleg in het segmentenpaneel. Verwijder het segment weer, Apply.
10. Sweep 50 k – 6,3 G, 1001 punten, laad `cal/n-6g3.cal.json`: CAL. Sluit een kabel met
    een short aan.
11. **TDR**: impulsgrafiek met de piek van de short, impedantiegrafiek met de duik naar
    0 Ω; verander velocity factor, window en Max distance; klik een marker, ← →.
12. **Gate** aan: sleep de oranje lijnen; band pass en notch; terug naar Frequency: label
    GATED en grijze randlijnen; Gate uit.
13. Kit fit: klap uit, bekijk het paneel (meet line open en line short alleen als je de
    kabel bij de hand hebt; anders alleen controleren dat het paneel er staat en **Fit
    kit** uitgeschakeld is zonder de twee metingen).
14. **Screenshot**: venster met het scherm van het apparaat, **Save…**.
15. File ▸ Save Touchstone, Open as trace (label FILE), Open as reference.
16. View ▸ Developer tools: Developer ▸ Log window, Copy diagnostics, Clear log.
17. Help ▸ About; Help ▸ Manual opent GitHub (bij de tag van de versie: tot de release
    bestaat die pagina voor deze versie nog niet, dat is verwacht).
18. **Stop**, **Disconnect**: het apparaat toont zijn eigen scherm weer.
19. File ▸ Quit; start opnieuw: alles staat zoals je het achterliet.

Herhaal de stappen 2, 3, 5, 6, 11, 14 en 18 met de NanoVNA-F.

---

# Ronde na NanoVNA-Saver (punten 53–60, 13 november 2026)

Acht kleine punten uit de vergelijking met NanoVNA-Saver (werklijst in
[docs/05-roadmap.md](05-roadmap.md), onderaan), elk in een eigen commit onder *Unreleased*.
Wat ik zelf gecontroleerd heb: `cargo fmt`, `cargo clippy -D warnings`, alle tests (99 in
de library, 8 in de GUI) en `check-manuals.sh` na elk punt; de browserbouw
(`scripts/build-web.sh`) compileert en is in de ingebouwde browser bekeken met het
voorbeeldbestand: de kolom L / C in de markertabel, de banden als achtergrond, de
vensterlijst met kaiser 6 en 13. **Niet gecontroleerd**: alles met een apparaat (de
middeling met drop en de log spacing zijn alleen op de library getest), de kabellijst en
de S21-offset op het scherm, en de native GUI op macOS, Linux en Windows. Uitgevoerd op
13 november 2026 op macOS met de LiteVNA 67; uitkomst en bevindingen 10 en 11 staan in het
logboek bovenaan. Daarna kwamen op verzoek van PA1BH de punten 69 en 70 erbij; die staan
onderaan deze ronde en zijn dezelfde avond door hem nagelopen: in orde. Punt 71 (Band en
regio 2, na de release van 0.3.4) eveneens.

Alles hieronder kan **zonder apparaat** met `cal/sweep.s2p` of het voorbeeld in de
browserbouw, tenzij anders gezegd. Begin met:

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && ./scripts/check-manuals.sh
```

Verwacht: geen uitvoer van fmt en clippy, alle tests groen, "manuals in step: 10 files
each".

## Punt 53 — Kolom L / C in de markertabel

**Veranderd.** `traces::series_equivalent`, `parallel_equivalent`, `q_factor`,
`format_eng`; de markertabel heeft een kolom **L / C** tussen Z en SWR.

```bash
cargo test -p litevna equivalents_of_a_coil engineering_format
```

GUI: open `cal/sweep.s2p` als trace, zet een marker. Verwacht: de kolom toont een
inductantie (positieve X, bijvoorbeeld `123 nH`) of een capaciteit (negatieve X, `47.0
pF`), met drie cijfers; bij X = 0 een streepje. Wijs de waarde aan: de hover geeft `Q =
|X|/R`, en de parallelvorm `R ∥ L of C`. Controle met de hand bij één marker: C =
1/(2π·f·|X|) uit de Z-kolom. De regel M2−M1 heeft op die plaats een lege cel.

## Punt 54 — Amateurbanden als achtergrond

**Veranderd.** `litevna::bands::REGION_1` (twintig banden, 2200 m tot 6 cm), View ▸
**Amateur bands**, bewaard in de instellingen.

```bash
cargo test -p litevna bands
```

GUI: View ▸ Amateur bands aan, sweep of bestand 1–30 MHz. Verwacht: grijze banden in de
drie frequentiegrafieken (niet in de Smith-chart), met de naam erboven waar de namen
elkaar niet raken; op 3–30 MHz staan er namen bij de brede banden, de smalle (30 m, 17 m,
12 m) hebben alleen hun streep. Inzoomen op 13,5–15 MHz (slepen in de plot): de 20 m-band
vult 14,000–14,350 MHz met naam. Zet uit, herstart: de keuze is bewaard. De banden van
regio 1: 160 m 1,810–2,000; 80 m 3,5–3,8; 40 m 7,0–7,2; 2 m 144–146; 70 cm 430–440 MHz.

## Punt 55 — Sweep avg met drop

**Veranderd.** `SweepResult::average_trimmed(sweeps, drop)`; `average` is het geval
`drop = 0`. In het sweeppaneel verschijnt **drop** vanaf Sweep avg 3, begrensd op avg − 2.

```bash
cargo test -p litevna trimmed_average
```

**Met apparaat.** Load op poort 1, Sweep avg 8, drop 0, Run. Tik tijdens het sweepen één
keer tegen de connector (of trek de load even los en zet hem terug): de trace schiet
omhoog en zakt in acht sweeps terug. Herhaal met drop 2: de trace beweegt niet of
nauwelijks. Zet Sweep avg terug op 1: het veld drop verdwijnt en staat bij de volgende
keer op 0.

## Punt 56 — log spacing van segmenten

**Veranderd.** `device::log_segment_edges`; knop **log spacing** naast **+ segment** in
het paneel **Segments**, grijs zolang er één segment is.

```bash
cargo test -p litevna log_segment_edges
```

GUI: Start 50k, Stop 6.3G, twee keer **+ segment**, **log spacing**. Verwacht: drie
segmenten met dezelfde verhouding (ongeveer 1:50 elk): 50 k → 2,5 M, 2,5 M → 126 M,
126 M → 6,3 G; de punten per segment en een gekozen calibratiebestand blijven staan; de
tekstvelden zijn herschreven, er is nog niets toegepast. **Met apparaat**: Apply sweep;
de trace is dicht op HF en grof op UHF, de markers lopen over de segmentgrenzen door. Een
start van 0 of een stop onder de start geeft de foutregel onder het sweeppaneel.

## Punt 57 — Kabellijst bij de verkortingsfactor

**Veranderd.** `timedomain::CABLES` (twaalf regels, cataloguswaarden); een keuzelijst
**cable** naast het getal in het TDR-paneel.

GUI: TDR-paneel, **cable** ▸ "0.830 Aircell 7": het getal springt op 0,830 en het
TDR-beeld rekent opnieuw (de piek van een kabel verschuift). Het getal blijft
bewerkbaar; na een handmatige wijziging staat er in de lijst niets meer aangevinkt.
De hover van de lijst zegt dat het nominale waarden zijn.

## Punt 58 — S21-offset

**Veranderd.** `SweepResult::offset_s21_db`; veld **offset** naast S21 onder Traces,
toegepast in `set_shown`, genoemd in het exportcommentaar; bewaard.

```bash
cargo test -p litevna s21_offset
```

GUI: open `cal/sweep.s2p` als trace, marker op een punt, lees S21 dB af. Zet offset op
20,0: S21 ligt in log mag 20 dB hoger, de marker ook, S11 ongewijzigd, de fase van S21
ongewijzigd. Save Touchstone…: de commentaarregel bevat `S21 offset +20.0 dB` en de
S21-kolommen zijn de gecorrigeerde waarden. Knop **0** zet hem uit. Herstart: de waarde
is bewaard.

## Punt 59 — Kaiser-vensters

**Veranderd.** `Window::Kaiser6`, `Window::Kaiser13`, `Window::ALL`; de GUI-lijst en
`vna-cli tdr --window` volgen `ALL`.

```bash
cargo test -p litevna kaiser
vna-cli tdr --from cal/sweep.s2p --window kaiser13 --max-distance 20
vna-cli tdr --from cal/sweep.s2p --window kaiser-13 --max-distance 20
vna-cli tdr --from cal/sweep.s2p --window kaiser --max-distance 20
```

Verwacht: de eerste twee geven dezelfde uitvoer met "kaiser 13 window" in de kopregel;
de derde weigert met "unknown window". GUI: Window ▸ kaiser 13 op een kabel met short:
de piek is breder dan bij hann en de bodem tussen de pieken ligt dieper (zijlobben weg);
kaiser 6 lijkt op hann met een stillere bodem. Herstart: de keuze is bewaard (ze wordt
als `kaiser 13` opgeslagen en teruggelezen).

## Punt 60 — Touchstone-tests

```bash
cargo test -p litevna files_from_other_programs
```

Verwacht: groen. Niets in het programma veranderd; de test legt vast dat commentaar na de
optieregel, een herhaalde optieregel en een kale `#` gelezen worden, en met welke melding
dalende frequenties en een halve regel geweigerd worden.

## Punt 69 — Gedeelde frequentie-as en reset view

**Veranderd.** De drie frequentiegrafieken delen hun x-as (`link_axis`), de twee
TDR-grafieken hun afstand-as; de y-as blijft per grafiek. Knop **reset view** in de
markerbalk en rechts in de kop van de TDR-weergave. In de browserbouw gezien: slepen in
log mag verschuift SWR en fase mee, **reset view** zet alle drie terug.

GUI, zonder apparaat met `cal/sweep.s2p`: sleep in log mag naar links. Verwacht: SWR en de
derde grafiek tonen hetzelfde stuk spectrum, de Smith-kaart verandert niet. Zoom met
Ctrl/⌘ en scrollen in SWR: de andere twee zoomen in frequentie mee, hun verticale schaal
blijft. **reset view**: alle drie weer automatisch; een dubbelklik in één grafiek doet
hetzelfde voor alle drie. TDR-weergave: sleep in de bovenste grafiek, de onderste volgt;
**reset view** rechts in de kop zet ze terug. De gate slepen werkt nog zoals voorheen.

## Punt 70 — Sweep uit de calibratie

**Veranderd.** `sweep_from_calibration` in `calibration.rs`; knop **sweep from
calibration** achter de regel "N pts, start → stop" in het calibratiepaneel. Niet in de
browserbouw te zien (het paneel staat er niet in); door PA1BH met de LiteVNA bekeken.

**Met apparaat**: Sweep 1M–30M, 201 punten, Apply. Laad `cal/n-1g.cal.json` (201 punten,
1 MHz–1 GHz). Klik **sweep from calibration**. Verwacht: Start `1MHz`, Stop `1GHz`, Points
201, de sweep start opnieuw (of één sweep als Run uit stond), de oranje regel "Start/Stop/
Points differ" verschijnt niet, label CAL. Laad daarna `cal/n-hf.cal.json` (1001 punten,
100 kHz–50 MHz) en klik weer: 100kHz–50MHz, 1001 punten. Zonder verbinding vult de knop
alleen de velden. Een calibratie met een puntental buiten de keuzelijst (bijvoorbeeld een
bestand van een andere versie met 301 punten) geeft de dichtstbijzijnde keuze en een rode
regel onder het sweeppaneel dat de calibratie geïnterpoleerd wordt.

## Punt 71 — Band in het sweeppaneel, regio 2

**Veranderd.** `bands::REGION_2`, `Region`, `Band::with_margin`; rij **Band** in het
sweeppaneel (keuzelijst "choose…"), radioknoppen IARU Region 1 / 2 onder View ▸ Amateur
bands, bewaard als `band_region` (1 of 2, iets anders leest als 1).

```bash
cargo test -p litevna bands
```

GUI, zonder apparaat: View ▸ Amateur bands aan, regio 1. Band ▸ 40 m: Start `6.980MHz`,
Stop `7.220MHz`, Points ongewijzigd; de hover van 40 m in de lijst noemt 7,000–7,200 en
6,980–7,220. Zet regio 2: dezelfde keuze geeft 6,97–7,33 MHz, en de lijst heeft nu ook
1.25 m, 33 cm en 5 cm; met `cal/sweep.s2p` als trace loopt de grijze 40 m-band tot 7,3 en
80 m tot 4,0 MHz. **Met apparaat**: Run aan, Band ▸ 20 m: de sweep herstart op 13,965–
14,385 MHz met de calibratie geïnterpoleerd (label CAL, of de waarschuwing als het bestand
het bereik niet dekt). Herstart: regio 2 is bewaard. In de browserbouw staat het
sweeppaneel niet, de regiokeuze en de achtergrond wel.

## Slotronde voor deze punten

1. Start de GUI met je bestaande instellingen (0.3.3): ze laden; View ▸ Amateur bands
   staat uit, offset op 0, drop afwezig (Sweep avg 1).
2. Loop de punten 53, 54, 57, 58 en 59 in de GUI af zoals hierboven, met `cal/sweep.s2p`.
3. Met de LiteVNA: 55 en 56.
4. Sluit af, start opnieuw: Amateur bands, offset, drop, venster en VF staan zoals je ze
   achterliet.
5. Na 13 november erbij: 69 (zonder apparaat) en 70 (met apparaat).

---

# Open vragen

1. **Punt 28, Help lokaal openen: beantwoord.** Mijn eerdere advies klopte niet (de
   handleiding in het release-archief is even oud als de tag, en het is markdown).
   Besluit van 10 november: de Help-links blijven naar de tag van de eigen versie wijzen;
   een correctie in de handleiding gaat mee met de volgende release. Er is niets
   veranderd aan de code.
2. **Versienummer en release: afgerond.** Uitgebracht als 0.3.0-rc.1, 0.3.0-rc.2 en op
   12 november 2026 als 0.3.0; de secties van de candidates in `CHANGELOG.md` zijn
   samengevoegd tot `## [0.3.0]`. Wat de volgende testronde vindt, komt in een volgende
   versie.
3. **Pushen: gedaan.** De CI heeft gedraaid; zie punt 16 en 29 voor wat je daarin
   naloopt.
4. **Punt 42, kit-bestanden.** Welk formaat wil je kunnen lezen (NanoVNA-Saver, een
   Keysight-achtige tabel, iets anders)? Zonder voorbeeldbestand bouw ik dat niet.
5. **Dubbele frequenties in Touchstone (punt 48)** worden nu geweigerd. Kom je een
   bestand van een ander programma tegen dat ze heeft en dat je wilt lezen, dan kan dat
   soepeler.
