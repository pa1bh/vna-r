# vna-r handleiding

Een vectornetwerkanalyzer (VNA) meet hoe een antenne, kabel, filter of ander
RF-onderdeel zich gedraagt per frequentie. vna-r is een programma voor de macOS- en
Linux-desktop dat een LiteVNA 64 of 67 of een NanoVNA-F via USB aanstuurt, de metingen zelf calibreert en
ze toont, met markers, tijddomein (TDR), gating, referenties en export. Deze handleiding
legt uit wat je meet, hoe je het programma bedient en waar de valkuilen zitten.

De hoofdstukken bouwen op elkaar voort, maar zijn ook los te lezen:

1. [Inleiding: wat een VNA meet](01-inleiding.md). S11, S21, reflectiecoëfficiënt, SWR,
   impedantie, dB en fase, de Smith-chart, en hoe vna-r in elkaar zit.
2. [Installeren en aansluiten](02-installeren.md). Bouwen op macOS en Linux, de USB-poort,
   het eerste contact, wat het scherm van het apparaat doet tijdens een meting.
3. [De GUI](03-gui.md). Menubalk, sweep, segmenten, weergaven, markers, middelen,
   instellingen die bewaard blijven.
4. [Calibreren](04-calibreren.md). Waarom het moet, open/short/load/through stap voor stap,
   bestanden, interpolatie naar een ander raster, de kit-fit boven 1 GHz, controles en
   valkuilen.
5. [Tijddomein: TDR en gating](05-tijddomein.md). Van frequentie naar afstand, de twee
   grafieken, vensters, resolutie en bereik, kabels doormeten, een reflectie uitsnijden.
6. [Meetrecepten](06-meetrecepten.md). Antenne afregelen, kabel meten (lengte,
   verkortingsfactor, verlies), filter en verzwakker meten, voor/na vergelijken.
7. [Bestanden](07-bestanden.md). Touchstone exporteren en inlezen, calibratiebestanden,
   referenties, schermafbeeldingen.
8. [Commandoregel: vna-cli](08-cli.md). Alle commando's en opties, met voorbeelden.
9. [Apparaten, platform en problemen oplossen](09-apparaten-en-problemen.md). LiteVNA 64
   en 67 en de NanoVNA-F naast elkaar, Linux-rechten, veelvoorkomende foutmeldingen, een
   probleem melden, wat je kunt vertrouwen.

Wie de achtergrond wil, vindt in `docs/01` tot en met `docs/06` de onderzoeks- en
ontwikkelnotities: het USB-protocol, het calibratiemodel met metingen, de TDR-wiskunde en
de roadmap.

## Conventies

- Frequenties typ je als `14.2M`, `50k`, `1.3G` of `6,3G`; een komma mag als decimaalteken.
- dB-waarden van S11 en S21 zijn negatief of nul: 0 dB is alles terug of alles door,
  −20 dB is 1 % van het vermogen.
- Afstanden in TDR zijn enkele reis (de lengte van de kabel), tijden zijn heen en terug.
- "Referentievlak" is het punt waar de calibratie de meting op legt: bij standaarden
  direct op poort 1 is dat de N-connector van het apparaat.
