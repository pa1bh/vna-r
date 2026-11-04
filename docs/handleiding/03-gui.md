# 3. De GUI

Start met `cargo run --release -p vna-gui` of `target/release/vna-gui`. Het venster heeft een
bovenbalk (poort, verbinding, Run), een linkerpaneel met instellingen en een
hoofdgebied met vier grafieken.

![Weergaven, markers en een gecalibreerde meting van een kabel met load](../images/gui-load-rx.webp)

## Verbinden en sweepen

1. Kies de poort in de keuzelijst; de LiteVNA heeft een ster. De verversknop zoekt
   opnieuw.
2. **Connect**. De bovenbalk toont firmware, hardwarerevisie en batterijspanning.
3. **Run** start continu sweepen, **Single** doet er één. Run nogmaals stopt.

Onder **Sweep** stel je **Start**, **Stop** en **Points** in en bevestig je met **Apply
sweep**; tijdens Run herstart de sweep dan met de nieuwe instellingen. Frequenties typ je
als `50k`, `14.2M`, `1.3G` of `6,3G`. De LiteVNA meet 50 kHz tot 6,3 GHz; meer punten
geeft fijner detail maar duurt langer (ongeveer 300 tot 600 punten per seconde).

**Average** (alleen LiteVNA) is de bandbreedte-instelling van het apparaat zelf (1 tot 80): hoger is
rustiger maar trager, en de instelling blijft in het apparaat staan. **Sweep avg** is iets
anders: het programma middelt de laatste N sweeps complex. Dat drukt ruis op lage niveaus
zonder de magnitude te vertekenen, en een teller laat zien hoeveel sweeps al meetellen.
Verander je het raster, dan begint het middelen opnieuw.

## Segmenten

Onder **Segments** voeg je extra frequentiebereiken toe die na het eerste geveegd en in
één trace getoond worden. Elk segment kan een eigen calibratiebestand hebben; zonder
bestand wordt de calibratie van het paneel geïnterpoleerd. De toepassing bij uitstek is
een fijn HF-segment met een HF-calibratie naast een breed segment tot 6,3 GHz, omdat een
brede calibratie aan de onderkant te grof is (hoofdstuk 4). Segmenten mogen elkaar raken
maar niet overlappen. TDR en gate werken op het breedste segment.

![Twee segmenten: 1–50 MHz met HF-calibratie en 50 MHz–6,3 GHz met de brede](../images/gui-segmenten.webp)

## De vier grafieken

Linksboven staat de Smith-chart, altijd van S11. De drie andere grafieken kiezen in hun
kop wat ze tonen:

| weergave | eenheid | wat je ziet |
|---|---|---|
| log mag | dB | \|S11\| en \|S21\| in dB; de standaard |
| lin mag | – | \|S\| lineair, 0 tot 1 |
| phase | ° | fase, −180 tot +180 |
| SWR | – | staande-golfverhouding, afgekapt op 20 |
| R + jX | Ω | weerstand (doorgetrokken) en reactantie (gestippeld) van de poortimpedantie |
| \|Z\| | Ω | modulus van de impedantie |
| group delay | ns | −dφ/dω over ±N punten; N staat onder Traces |

SWR, R + jX en |Z| gaan alleen over S11. Onder **Traces** zet je S11 en S21 aan of uit;
dat geldt voor alle grafieken. Dubbelklik in een grafiek zet de schaal terug op
automatisch; slepen en scrollen zoomt.

![Groepsvertraging van 53 cm kabel: vlak op 2,5 ns](../images/gui-weergaven-groepsvertraging.webp)

## Markers

Bovenin staan M1 tot M4. De geselecteerde is actief: klik in een grafiek of op de
Smith-chart om hem daar te zetten, en verschuif hem met de pijltjestoetsen (met shift tien
punten tegelijk). De tabel eronder toont per marker frequentie, S11 in dB en graden, Z,
SWR en S21; staan er twee, dan ook het verschil, handig voor de bandbreedte van een filter
of de afstand tussen twee resonanties. De knoppen **min SWR**, **max S21** en **min S21**
springen met de actieve marker naar dat punt.

Rechtsboven staat een label dat zegt wat je ziet: **CAL** (gecalibreerd), **RAW**
(ongecorrigeerd), **CAL\*** (sommige segmenten niet gecalibreerd), **GATED** (S11 is
gegate, hoofdstuk 5) of **FILE** (een bestand in plaats van het apparaat, hoofdstuk 7).

## Referenties

**Hold** in de markerbalk zet de trace zoals hij nu getoond wordt vast. Tot vier
referenties, elk met naam en kleur, staan onder **Traces**; vink ze aan of uit, hernoem
ze, of haal ze weg. De knop **Δ** bij een referentie tekent live min referentie in de
log mag- en fasegrafieken, in dB en graden. Zo zie je direct wat een aanpassing aan een
antenne doet. Referenties verdwijnen bij het afsluiten; bewaar ze als Touchstone en open
ze later weer als referentie.

## Schermafbeelding van het apparaat

**Screenshot** haalt de inhoud van het LCD van de LiteVNA op en toont hem in een
venster, met een knop om hem als PNG te bewaren. Handig voor documentatie van wat het
apparaat zelf liet zien.

## Wat bewaard blijft

Bij afsluiten worden bewaard: poort, start, stop, punten, average, sweep avg, segmenten,
het calibratiebestand en of het toegepast wordt, de weergavekeuzes, TDR- en
gate-instellingen, markerfrequenties en de venstergrootte. Bij de volgende start komt dat
terug; het calibratiebestand wordt automatisch geladen en de markers worden bij de eerste
sweep op de dichtstbijzijnde frequentie gezet. Niet bewaard worden de meetdata zelf en de
referenties.
