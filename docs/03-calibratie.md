# Calibratie: van ruwe brugdata naar echte S-parameters

*Hoort bij `crates/litevna/src/calibration.rs`.*

## 1. Waarom de ruwe data niet klopt

De LiteVNA meet met een richtkoppelaar ("brug") hoeveel signaal er terugkomt uit poort 1 en hoeveel
er bij poort 2 aankomt. Wat je over USB krijgt is de verhouding van die ruwe golven. Daar zitten
drie systematische fouten in die niets met je antenne te maken hebben:

| Term | Naam | Wat het fysiek is |
|---|---|---|
| `e00` | directiviteit | lek van de brug: er "komt" altijd wat terug, ook met een perfecte 50 Ω belasting |
| `e11` | bronmatch | de bron is zelf geen perfecte 50 Ω, dus een deel van de reflectie reflecteert nogmaals |
| `e10·e01` | reflectietracking | versterking en fasedraaiing van het hele pad heen en terug (kabel, brug, mixer) |

Voor S21 komen er twee bij: `e30` (isolatie: overspraak van poort 1 naar 2 zonder DUT) en
`e10·e32` (transmissietracking: de versterking van het doorgaande pad).

Het model per frequentiepunt (m = gemeten, a = werkelijk):

```
S11m = e00 + e10e01 · S11a / (1 − e11 · S11a)
S21m = e30 + e10e32 · S21a / (1 − e11 · S11a)
```

Dit is het klassieke **3-term (one-port) foutmodel** plus de **"enhanced response"** uitbreiding
voor S21. Een echte 2-poorts VNA heeft 12 termen; de LiteVNA kan niet in de omgekeerde richting
meten, dus de "load match" van poort 2 (`e22`) blijft onbekend en wordt op 0 gesteld. Dat is
precies wat de firmware op het apparaat ook doet.

## 2. Hoe SOL de termen oplevert

Drie onbekenden per frequentie, dus drie metingen met een bekende `S11a`:

- **Short**: `S11a = −1`
- **Open**: `S11a = +1`
- **Load**: `S11a = 0`

Herschrijf het model lineair met `Δ = e00·e11 − e10e01`:

```
e00 + (S11a·S11m)·e11 − S11a·Δ = S11m
```

Drie standaarden geven een 3×3 stelsel dat exact oplosbaar is (regel van Cramer). De library
doet dat per punt en haalt daarna `e10e01 = e00·e11 − Δ` terug. Voordeel van deze algemene vorm:
de standaarden hoeven niet ideaal te zijn.

Correctie van een meting:

```
S11a = (S11m − e00) / (e10e01 + e11 · (S11m − e00))
S21a = (S21m − e30) · (1 − e11 · S11a) / e10e32
```

Merk op dat de S21-correctie de **gecorrigeerde** S11 van de DUT nodig heeft; daarom corrigeert
de library altijd eerst S11.

## 3. Niet-ideale standaarden

Een open connector is geen perfecte `+1`: aan het open einde zit een kleine randcapaciteit
(tientallen fF) die de fase boven enkele honderden MHz merkbaar draait. Een short heeft een kleine
inductie, een load is zelden exact 50,0 Ω, en alle drie hebben ze een stukje lijn ("offset delay")
voor de eigenlijke afsluiting. De module modelleert dat als:

```
Γ(f) = Γ_afsluiting(f) · e^(−j·2·ω·delay)
```

met `C(f)` en `L(f)` als derdegraads polynoom in f, zoals in de kit-definities van professionele
VNA's. `CalKit::ideal()` zet alles op nul; dat is ook wat de LiteVNA-firmware aanneemt en voor HF/VHF
ruim voldoende. Voor de N-kit die bij de 67 zit kennen we de parameters niet; die kunnen we later
schatten door te vergelijken met een bekende referentie.

## 4. Werkwijze met de CLI

Eén standaard per aanroep, alles in een JSON-bestand:

```bash
./target/release/vna-cli cal open  --file hf.cal.json --start 1M --stop 30M --points 201
```

```bash
./target/release/vna-cli cal short --file hf.cal.json
```

```bash
./target/release/vna-cli cal load  --file hf.cal.json
```

```bash
./target/release/vna-cli cal through --file hf.cal.json
```

```bash
./target/release/vna-cli cal show --file hf.cal.json
```

Daarna meet je gecalibreerd door het bestand mee te geven; het frequentieraster komt dan uit de
calibratie:

```bash
./target/release/vna-cli sweep --cal hf.cal.json --s2p antenne.s2p
```

Een ander raster binnen het gecalibreerde bereik mag ook; de library interpoleert dan:

```bash
./target/release/vna-cli sweep --cal hf.cal.json --start 3.5M --stop 3.8M --points 101
```

Regels:
- Een calibratie geldt voor haar eigen **frequentiebereik**. Binnen dat bereik wordt naar elk
  raster geïnterpoleerd (zie §7), erbuiten weigert de library.
- Meet de standaarden met dezelfde `--average` als je latere metingen, anders verschilt de ruis.
- Het referentievlak ligt waar je de standaarden aansluit. Calibreer dus aan het eind van de kabel
  die je ook voor de meting gebruikt.
- `cal show` toont de fouttermen in dB. Vuistregel voor een gezonde brug: |e00| ruim onder −30 dB,
  |e11| onder −20 dB, |e10e01| in de buurt van 0 dB.

## 5. Controle

Na calibratie moet een opnieuw aangesloten standaard zijn definitie teruggeven: open ≈ 0 dB bij 0°,
short ≈ 0 dB bij 180°, load diep onder −40 dB. Hoe diep de load komt is een directe maat voor de
ruis en de herhaalbaarheid van je connectoren. De beste onafhankelijke check is een vergelijking
met de calibratie op het scherm van de LiteVNA zelf (`--calibrated` haalt die data op).

## 6. Validatie op de LiteVNA 67 (3 november 2026)

Raster 1 MHz – 1 GHz, 201 punten, N-kit direct op poort 1, ideale kit-definities, standaard
IF-bandbreedte. Through: testkabel + N-verloop tussen poort 1 en 2.

**Ruwe metingen van de standaarden** (bij 500 MHz): open -0,56 dB ∠-82°, short +0,50 dB ∠+97°,
load -30,2 dB. Open en short liggen 179° uit elkaar; de ongecalibreerde directiviteit is ~30 dB.

**Fouttermen** die de solver vond:

| freq | \|e00\| (directiviteit) | \|e11\| (bronmatch) | \|e10e01\| (tracking) |
|---|---|---|---|
| 1 MHz | -12,7 dB | -26,3 dB | -1,62 dB |
| 200 MHz | -32,0 dB | -25,4 dB | -0,60 dB |
| 400 MHz | -30,9 dB | -27,2 dB | -0,25 dB |
| 600 MHz | -28,8 dB | -31,7 dB | +0,16 dB |
| 800 MHz | -25,7 dB | -28,0 dB | +0,29 dB |
| 1 GHz | -23,3 dB | -28,5 dB | +0,20 dB |

De slechte directiviteit bij 1 MHz is typisch voor deze brug aan de onderkant van het bereik en
precies de reden dat calibratie daar onmisbaar is.

**Controles** (zelfde aansluiting als tijdens de calibratie, dus een consistentie- en ruistest):

- Load gecalibreerd: -65 … -82 dB over de hele band, mediaan -71 dB, 0 van 201 punten boven -40 dB
  (ruw was -30 dB).
- Through gecalibreerd: S21 = 0,00 dB ∠0,0° op alle punten; S11 van het doorgaande pad
  -31 … -54 dB, dat is de werkelijke retourdemping van kabel + verloop + poort 2 (de "load match"
  die het model niet kan corrigeren).

De echte herhaalbaarheidstest is een standaard losdraaien en opnieuw aansluiten; zie §5.

**Herhaalbaarheid** (Open losgedraaid en opnieuw aangesloten, daarna gecalibreerd gemeten):
|S11| tussen -0,01 en +0,01 dB en fase tussen -0,1° en +0,1° op alle 201 punten tot 1 GHz. De
randcapaciteit van de N-open is tot 1 GHz dus niet zichtbaar boven de meetruis; de ideale kit volstaat
hier. Deze sweep bevatte bovendien 32 verouderde FIFO-records, die de lees-tot-compleet-logica van
de library correct opving.

## 7. Interpolatie naar een ander raster

De fouttermen veranderen glad met de frequentie, dus tussen twee calibratiepunten kun je ze
schatten. De library interpoleert niet de fouttermen zelf maar de **ruwe metingen van de
standaarden** naar het nieuwe raster en lost daarna opnieuw op. Dat gebeurt polair: grootte en
fase apart, de fase langs de kortste boog. Lineair in reëel en imaginair deel (zoals tot 0.2.5)
snijdt de hoek tussen twee fasoren af: het midden van een koorde over θ heeft grootte cos(θ/2),
−0,13 dB bij 20° en −3 dB bij 90°. Omdat alle standaarden evenveel krimpen wordt de
trackingterm te klein en komt elke gecorrigeerde |Γ| te hoog uit. Voordelen: het JSON-bestand blijft de enige bron van waarheid, en een through of
isolation die later wordt toegevoegd doet automatisch mee.

Twee grenzen:

1. **Bereik.** Buiten het gecalibreerde bereik is er niets om tussen te interpoleren; dat wordt
   geweigerd. Calibreer dus liever één keer breed (bijvoorbeeld 50 kHz – 1 GHz met 401 punten) en
   zoom daarna in.
2. **Fasedraaiing per stap.** De kortste boog is pas dubbelzinnig bij 180° per stap; de library
   meet de grootste fasestap van open, short, through (S21), line open en line short en weigert
   boven 135°, zodat ruis op de fase de richting niet kan omkeren. Load en isolation tellen niet
   mee, hun fase is ruis. Vuistregel: hoe langer de kabel tussen VNA en referentievlak, hoe
   sneller de fase draait en hoe meer calibratiepunten je nodig hebt.

Synthetische tests: 101 calibratiepunten over 10 MHz – 1 GHz, geïnterpoleerd naar 401 punten die
er precies tussenin liggen, geeft een restfout kleiner dan 2·10⁻⁴ op |Γ| = 0,4. Met 11 punten over
100 MHz – 1 GHz en fouttermen die 62° per stap draaien (lange kabel) blijft de fout op |Γ| onder
0,01; de re/im-interpolatie van vóór 0.2.6 gaf hier 0,17 (+1,3 dB).

**Gemeten op de LiteVNA 67** met de calibratie van §6 (201 punten, 1 MHz – 1 GHz) en de Open nog
aangesloten:

| Raster | \|S11\| | fase |
|---|---|---|
| 1 MHz – 1 GHz, 201 pt (eigen raster) | -0,01 … +0,01 dB | -0,1 … +0,1° |
| 50 – 900 MHz, 301 pt (geïnterpoleerd) | -0,02 … +0,01 dB | -0,4 … +0,3° |
| 140 – 150 MHz, 51 pt (geïnterpoleerd) | -0,006 … +0,003 dB | -0,04 … +0,05° |

De iets grotere fasefout op het brede geïnterpoleerde raster komt doordat de fase van de Open
tussen de calibratiepunten (stap 5 MHz) een paar graden draait en lineair wordt benaderd. Meer
calibratiepunten verkleinen dat direct.

### Valkuil: de onderkant van een brede calibratie (4 november 2026)

Gemeten: load aan het eind van de testkabel, sweep 100 kHz–50 MHz, gecorrigeerd met de
6,3 GHz-calibratie (1001 punten, stap 6,3 MHz). Resultaat: R zakt naar 14 Ω rond 1 MHz, SWR
piekt op 3,8, en vanaf precies 6,3 MHz is alles vlak op 51 Ω. Met een calibratie op het
HF-raster zelf (`cal/n-hf.cal.json`, 100 kHz–50 MHz, 1001 punten) is de load vanaf 100 kHz vlak.

Oorzaak: tussen het eerste (50 kHz) en tweede (6,35 MHz) calibratiepunt worden de fouttermen
lineair geïnterpoleerd, terwijl de brug en de koppelcondensatoren van de LiteVNA juist daar
sterk van frequentie afhangen. De fasesprong-regel van §7 vangt dit niet, want de open draait
op deze frequenties nauwelijks. Daarom is er een tweede controle:
`CalibrationSet::coarse_low_end_warning` waarschuwt wanneer de sweep punten heeft tussen de
eerste twee calibratiepunten en het tweede meer dan 2× zo hoog ligt als het eerste. De GUI
toont dat oranje onder het calibratiepaneel, de CLI als `WARNING`. Structureel lost stap 6 van
de roadmap (multi-segment sweeps met calibratie per segment) het op.

### Oplossing: segmenten met eigen calibratie

Sinds roadmap-stap 6 kan een sweep uit meerdere segmenten bestaan, elk met een eigen
calibratiebestand (GUI: **Segments**; CLI: `--segment START:STOP:POINTS@cal.json`). Een HF-segment
op zijn eigen dichte calibratie naast het brede segment op de 6,3 GHz-calibratie geeft één trace
zonder de kunstmatige dip hierboven.

### Valkuil: de bandgrens bij 100 MHz (12 november 2026)

Gemeten: een open N-naar-BNC-verloopstuk, sweep 50–150 MHz in 801 punten, gecorrigeerd met
`cal/n-1g.cal.json` (201 punten, 1 MHz–1 GHz, stap 4,995 MHz). Resultaat: R + jX en |Z|
lopen vanaf 96 MHz lineair weg (|Z| van 870 naar 930 Ω, R van 0 naar 50 Ω) en vallen op
precies 100 MHz terug op de verwachte lijn. De rest klopt: −1183 Ω op 71 MHz en −600 Ω op
139,75 MHz zijn allebei 1,9 pF.

Oorzaak: de LiteVNA 67 wisselt bij 100 MHz van signaalbron, en alles wat hij ruw meet
verspringt daar. In het calibratiebestand is dat te zien aan de ruwe short: 0,094 tussen
95,905 en 100,900 MHz, tegen 0,011 tot 0,014 tussen de punten eromheen (open 0,026 tegen
0,014; load niet zichtbaar). `cal/n-6g3.cal.json` toont hetzelfde tussen 94,549 en
100,849 MHz. Op het calibratieraster is elk punt gecorrigeerd met termen die op dat punt
gemeten zijn, dus daar is niets aan de hand. Ertussen schuift de interpolatie de termen
geleidelijk van de ene bron naar de andere, terwijl het apparaat tot 100 MHz nog met de
lage bron meet: de fout groeit lineair tot de grens en valt erachter terug. De exacte
omschakelfrequentie is afgeleid uit deze sweep, niet uit de firmware.

Sinds 12 november 2026 zoekt `CalibrationSet::jumps` zulke sprongen: een stap tussen twee
calibratiepunten die per hertz minstens viermaal zo groot is als de grootste van de vier
stappen aan weerszijden, en minstens 0,02 groot. `jump_warning` waarschuwt als een sweep
punten in zo'n stap heeft; de CLI en de GUI tonen dat naast de waarschuwing voor de
onderkant, en `vna-cli cal show` noemt de sprongen in een bestand. De drempel is gekozen
op de zes calibratiebestanden in `cal/`: de echte sprong geeft een verhouding van 5,7 en
6,9, de grootste ruis (open rond 5,8 GHz in `n-6g3`) 3,1. Beperkingen: de eerste en
laatste twee stappen worden niet beoordeeld, en een raster dat veel grover is dan de
sprong verbergt hem (`qc-grof`, stap 63 MHz, meldt niets). Er wordt alleen gewaarschuwd;
niet over de sprong heen interpoleren vraagt de exacte grens per apparaat.

## 8. Calibratie tot 6,3 GHz en de kit-fit

### Waarom de ideale kit boven ~1 GHz niet meer volstaat

Een Open is geen perfecte +1: aan het open einde zit randcapaciteit, en bij een N-female Open zit de
pen bovendien teruggetrokken, wat neerkomt op een stukje extra lijn. Beide laten de echte Γ
achterlopen: capaciteit met `2·arctan(ω·C·Z0)`, een offset-delay τ met `2·ω·τ`. Bij 6 GHz is dat al
gauw 10° of meer. Calibreren met ideale definities dwingt zo'n Open tóch naar +1 en vertekent
daarmee alle latere metingen.

Het vervelende: **aan de poort zelf zie je die fout nooit.** Een opnieuw aangesloten Open komt per
definitie op +1 uit, een Load op 0. Een SOL-calibratie legt de standaarden alleen *ten opzichte van
elkaar* vast; de absolute definitie is een conventie.

### Hoe je de asymmetrie tóch meet: Open en Short aan het eind van een kabel

Noem de werkelijke achterstand van de Open φ en die van de Short ψ. Calibreer je met ideale
definities, dan blijft er een bilineaire (Möbius-)vervorming over die aan de poort onzichtbaar is,
maar op een lijn niet. Uitgeschreven voor kleine hoeken, met `ε = (φ − ψ)/2`:

```
|Γ'| open-aan-kabel   ≈ |Γ| · (1 − ε · sin u)
|Γ'| short-aan-kabel  ≈ |Γ| · (1 + ε · sin u)        u = fase van de lijn (draait met f)
```

De rimpel is tegengesteld voor Open en Short, en de verhouding van de twee magnitudes elimineert
de kabeldemping:

```
(1 − |Γ'open| / |Γ'short|) / (2·|Γ|) ≈ ε · sin u ,     ε ≈ ω · (C·Z0 − L/Z0)   of   ε ≈ ω · τ
```

Een kleinste-kwadratenfit van die grootheid tegen `ω·sin(arg Γ'open)` geeft `k = C·Z0 − L/Z0`
(of `k = τ`). Alleen het *verschil* tussen Open en Short is meetbaar; de gebruikelijke conventie is
de Short ideaal te noemen en alles aan de Open toe te schrijven. De library doet dit iteratief
(toepassen, opnieuw oplossen, opnieuw fitten), zodat ook grote hoeken exact uitkomen. In een
synthetische test met 60 fF komt de fit binnen 1 fF uit.

In de CLI:

```bash
./target/release/vna-cli cal line-open  --file x.json    # kabel op poort 1, Open aan het verre eind
```

```bash
./target/release/vna-cli cal line-short --file x.json    # zelfde kabel, Short aan het verre eind
```

```bash
./target/release/vna-cli cal fit --file x.json [--as-delay] [--apply]
```

`--as-delay` slaat de asymmetrie op als offset-delay van de Open in plaats van als capaciteit; voor
N-connectoren met teruggetrokken pen is dat het fysisch juiste model. `cal kit` zet kit-parameters
met de hand (`--open-c` fF, `--open-delay` ps, `--short-l` pH, `--load-r` Ω, …).

### Valkuil: een losse standaard

Tijdens deze sessie leverde een niet goed aangedraaide Open een meting op van -4 tot -15 dB in
plaats van ~0 dB. De solver vindt dan nog steeds een passende map (een opnieuw gemeten Load komt
dus gewoon diep uit!), maar de fouttermen worden onfysisch: een bronmatch |e11| van +6 dB bestaat
niet. Controleer daarom altijd `cal show`; de CLI waarschuwt nu ook direct wanneer een Open/Short
mediaan onder -3 dB, een Load boven -10 dB of een Through onder -6 dB meet.

### Gemeten op de LiteVNA 67 (3 november 2026)

Raster 50 kHz – 6,3 GHz, 1001 punten, N-kit direct op poort 1; lijn = testkabel (eenweg-delay
≈ 2,5 ns, ≈ 53 cm bij VF 0,70).

Fouttermen met de (goede) Open-meting:

| freq | \|e00\| | \|e11\| | \|e10e01\| |
|---|---|---|---|
| 1,26 GHz | -21,2 dB | -35,5 dB | +0,16 dB |
| 2,52 GHz | -17,6 dB | -26,7 dB | 0,00 dB |
| 3,78 GHz | -17,1 dB | -24,4 dB | -1,67 dB |
| 5,04 GHz | -17,8 dB | -14,8 dB | -2,43 dB |
| 6,30 GHz | -17,9 dB | -19,3 dB | -3,26 dB |

Through gecalibreerd: S21 binnen ±0,04 dB tot 3 GHz, ±0,14 dB tot 6,3 GHz (de rest is de
niet-gemodelleerde ingangsmatch van poort 2: -27 → -20 dB met kabel en verloop).

**Kit-fit** uit line-open en line-short (953 punten ≥ 300 MHz): asymmetrie **+39,8 fF**
(equivalent +1,99 ps offset-delay van de Open), onafhankelijk van de ondergrens van de fit
(300 MHz of 1 GHz geeft 39,8 resp. 39,9 fF). Rimpel-RMS 0,037 → 0,013 rad; de rest is vermoedelijk
de Load (een imperfecte Load geeft een rimpel met dezelfde periode) en ruis. Over 0–6,3 GHz zijn
capaciteit en delay hier niet te onderscheiden (`ω·C·Z0` blijft < 0,08), dus is de conventie
"C0 = 40 fF, Short ideaal" aangehouden.

Onafhankelijke controle: de Short aan het kabeleinde, gecalibreerd met beide kits, piek-piek-rimpel
van |S11| na aftrek van de dempingstrend:

| band | ideale kit | gefitte kit (40 fF) |
|---|---|---|
| 0–1 GHz | 0,25 dB | 0,20 dB |
| 1–2 GHz | 0,48 dB | 0,18 dB |
| 2–3 GHz | 0,53 dB | 0,21 dB |
| 3–4 GHz | 1,03 dB | 0,39 dB |
| 4–5 GHz | 1,21 dB | 0,36 dB |
| 5–6,3 GHz | 1,30 dB | 0,56 dB |

Conclusie: tot ~1 GHz maakt de kit-definitie niets uit; daarboven halveert de gefitte Open de
restfout of beter. Wat overblijft (0,2–0,5 dB) vraagt om een betere Load-definitie of een echte
referentie (airline).

### In de GUI

Sinds 2026-11-04 zit de fit ook in `vna-gui`: calibratiepaneel → **Kit fit (line open / line short)**.
Knoppen `line open`/`line short` meten de kabel, **Fit** roept `CalibrationSet::fit_open` aan met de
ingevulde ondergrens en het gekozen model, **Apply to kit** schrijft het resultaat via
`CalibrationSet::apply_open_fit` in de kit (naam "ideal" wordt "fitted"), **Reset kit** zet de kit terug
op ideaal. De plausibiliteitscontrole van de CLI (`plausibility_warning`) staat nu in de library en
verschijnt in de GUI als oranje regel onder het calibratiepaneel.

## 9. Kan de load-definitie gefit worden? (4 november 2026)

Roadmap-stap 4 vroeg of de resterende ripple boven 3 GHz (na de open-fit van §8) uit de
load-definitie komt en of die te fitten is. Antwoord: niet met alleen kabelmetingen, en de
rest-ripple is grotendeels fysiek. De redenering en de getallen:

**Wat een load-fout doet.** Noem de echte reflectie van de load ε (wij definiëren hem als 0). De
solver legt die in e00 én e11: e00' = ε, e11' = −ε. Voor een DUT met Γ wordt de gecorrigeerde
waarde Γc ≈ Γ − ε(1 − Γ²). Op een kabel met open of short (|Γ| ≈ r) geeft dat
|Γc| ≈ r + Re(ε)(1 − r²)cos θ + Im(ε)(1 + r²)sin θ, met θ de lijnfase. Twee gevolgen:

- Het reële deel van ε (load-weerstand ≠ 50 Ω) is op een volle reflectie vrijwel onzichtbaar
  (factor 1 − r² ≈ 0,2).
- Het imaginaire deel (serie-inductie van de load) geeft precies hetzelfde sin θ-patroon,
  antisymmetrisch tussen open en short, als de open/short-asymmetrie van §8. De ratio-fit van
  §8 kan de twee niet onderscheiden: een deel van de "open C0 = 39,8 fF" kan dus load-inductie
  zijn. Voor de correctie op de kabel maakt dat niet uit; voor een DUT direct op de poort wel,
  maar dan in de orde van de term zelf (−30 dB).

**Wat de near-end term laat zien.** Een additieve ε zit in het tijdsdomein op t = 0; een
open/short-asymmetrie is multiplicatief en geeft daar niets. Met de gate uit docs/04 §6
(−0,4 tot 1,5 ns) rond het referentievlak van de gecorrigeerde line-open en line-short:

| band (GHz) | ripple open | ripple short | na far-end gate | near-end |X| |
|---|---|---|---|---|
| 1–2 | 0,0054 | 0,0054 | 0,0002 | −32,1 dB |
| 2–3 | 0,0052 | 0,0053 | 0,0001 | −31,8 dB |
| 3–4 | 0,0081 | 0,0081 | 0,0002 | −33,3 dB |
| 4–5 | 0,0086 | 0,0089 | 0,0004 | −32,8 dB |
| 5–5,7 | 0,0111 | 0,0115 | 0,0004 | −28,7 dB |

(ripple = RMS van lineaire |Γ| rond een kwadratische trend; `vna-cli cal check`.)

- Open en short geven dezelfde near-end term (verschil 0,0007 rms): consistent met een
  additieve term, zoals het model voorspelt.
- Die term draait met de frequentie als e^{−jωτ} met τ ≈ 0,09 ns (≈ 1 cm bij VF 0,7) en
  groeit met f: een kleine discontinuïteit net voorbij het referentievlak, de connector-
  overgang van de kabel, of de resistor van de load die ook ~1 cm achter zijn connector zit.
  Beide liggen binnen één resolutiecel (0,16 ns) en zijn in de tijd niet te scheiden. Een
  tweetermfit (vast + vertraagd) past wel, maar levert een onfysisch teken op: de twee termen
  zijn te gecorreleerd.
- De ripple die |X| ≈ 0,025 zou moeten geven (≈ 0,87·|X| ≈ 0,02 rms) is groter dan de gemeten
  0,005–0,011: de open-fit van §8 heeft het sin θ-deel van X dat ∝ ω loopt al in de open-
  capaciteit weggewerkt. Wat overblijft is het deel dat niet ∝ ω is (X draait) plus het kleine
  cos θ-deel.
- Met een band-pass gate om de verre reflectie is de lijn glad tot op 0,0002–0,0004 en komt de
  asymmetrie-refit op +0,14 fF uit: de calibratie zelf laat op de kabel niets ruws achter, de
  fouttermen zijn glad in frequentie. Alles wat ribbelt komt van reflecties op de kabel.

**Conclusie.** Een load-fit is met deze metingen niet identificeerbaar: een load-fout, de
kabelconnector en de open-asymmetrie zijn met één kabel niet uit elkaar te halen. De rest-
ripple van 0,3–0,4 dB p-p boven 3 GHz is de ingangsconnector van de kabel (−32 tot −29 dB)
plus de dubbele doorgang ervan, geen calibratiefout. Beter wordt het alleen met beter
materiaal: een kit met gespecificeerde load, of een airline als onafhankelijke referentie.

**Open experiment.** Of X in de kabel zit of in de load is wél te testen: meet line-short
nog eens met de kabel omgedraaid (`cal line-short --file kopie.json`, daarna `cal check`).
Verandert de near-end term, dan is het de connector; blijft hij gelijk, dan is het de load
en kan hij als serie-L in de kit (nog niet geïmplementeerd; dan is ook §8 opnieuw te doen).

**Poort-2 match.** Staat niet in het enhanced-response model en is met drie ontvangers niet
te calibreren. Effect op S21: ≈ |e22|·|S22,DUT|; met e22 ≈ −20 dB en een DUT van −10 dB
uitgangsaanpassing is dat 1 % (0,09 dB). Voor kabels, verzwakkers en doorlaatbanden van
filters verwaarloosbaar, in stopbanden niet.
