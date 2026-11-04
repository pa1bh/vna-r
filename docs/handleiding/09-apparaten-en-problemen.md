# 9. Apparaten, platform en problemen oplossen

## LiteVNA 64 en 67

Beide spreken hetzelfde binaire protocol en zijn over USB niet aan hun registers te
onderscheiden; vna-r behandelt ze gelijk en is op allebei geverifieerd.

| | LiteVNA 67 | LiteVNA 64 |
|---|---|---|
| bereik | 50 kHz – 6,3 GHz | 50 kHz – 6,3 GHz |
| scherm | 1024×600 | 480×320 |
| USB-naam | ZeenKo.tech LiteVNA6 | Black Sphere Technologies CDC-ACM Demo |
| poort op macOS | /dev/cu.usbmodemv1_01 | /dev/cu.usbmodemDEMO1 |
| snelheid | 340–600 punten/s | ~300 punten/s |

De LiteVNA-H1 gebruikt de NanoVNA-H-firmware met een tekstprotocol en wordt nog niet
ondersteund (roadmap).

Richtwaarden voor wat de apparaten kunnen, gemeten met vna-r en een N-kit:
richtingsfout ruw −17 tot −21 dB boven 1 GHz (daarom calibreren), gecalibreerde load
−65 tot −80 dB op HF, dynamiek van S21 ongeveer 90 dB op HF en 60 tot 70 dB boven 3 GHz,
herhaalbaarheid na los- en vastdraaien ±0,01 dB en ±0,1°.

## Platform

- **macOS**: geen stuurprogramma, poort `/dev/cu.usbmodem…`.
- **Linux**: poort `/dev/ttyACM*`, udev-regel of groep `dialout` voor de rechten,
  bouwpakketten `pkg-config libudev-dev libgtk-3-dev`. De GUI is op Linux gebouwd en
  getest in een container, nog niet op een Linux-desktop met scherm gedraaid; meldingen
  zijn welkom.
- **Windows**: niet getest.

## Foutmeldingen

**"no VNA found"** of de poort ontbreekt in de lijst. Datakabel? Staat het apparaat
aan? Op Linux: rechten (zie boven). Op macOS verschijnen soms twee poorten
(`cu.` en `tty.`); vna-r gebruikt `cu.`.

**"device did not answer INDICATE"** of een timeout. Een ander programma heeft de poort
(GUI verbonden terwijl je de CLI gebruikt, of NanoVNA-Saver open). Sluit dat eerst. Komt
het incidenteel voor direct na het aansluiten, probeer dan opnieuw; het apparaat heeft
soms een seconde nodig.

**"target grid lies outside the calibrated span"**. De sweep valt buiten het bereik van
het geladen calibratiebestand. Laad het juiste bestand of pas de sweep aan. Het segment
wordt tot die tijd ruw getoond (label CAL\*).

**"calibration grid too coarse to interpolate"**. Te weinig punten in de calibratie voor
deze frequenties; calibreer met meer punten.

**Oranje waarschuwing over de eerste twee calibratiepunten**. De sweep heeft punten onder
het tweede calibratiepunt van een brede calibratie; dat stuk is onbetrouwbaar. Gebruik
een HF-calibratie of segmenten (hoofdstuk 4).

**Oranje waarschuwing bij een standaard**. De gemeten standaard lijkt niet op wat hij
hoort te zijn: losse connector, verkeerde standaard, of stuk. Controleer en meet opnieuw.

**Het apparaat blijft in USB MODE**. `vna-cli release`, of uit en aan.

## Wat je kunt vertrouwen

Een goede gewoonte is om na het calibreren en voor elke serieuze meting drie dingen te
controleren:

1. **Load op het referentievlak**: S11 onder −50 dB over de band. Zo niet, dan is er iets
   mis met de calibratie of de load.
2. **Open of short op het referentievlak**: 0 dB vlak, fase 0° resp. 180° en langzaam
   draaiend.
3. **Herhaalbaarheid**: los- en vastdraaien verandert minder dan 0,05 dB.

Als een meting verrast, kijk dan eerst **RAW** (apply uit). Ruw hoort een open of short
op 0 dB te staan en een load tussen −10 en −30 dB; staat een "load" ruw op −3 dB met een
fase die langzaam draait, dan maakt zijn middenpen geen contact of is hij inwendig
gebroken. Een multimeter tussen pen en huis moet 50 Ω geven.

Twijfel je aan de software, dan geven GUI en CLI dezelfde getallen op dezelfde
calibratie; een snelle `vna-cli sweep --cal … --rows 10` is de controle.

## Connectoren

N-connectoren gaan lang mee als je alleen de wartel draait en nooit het huis, en als er
geen mannelijke pen in komt die te ver uitsteekt (goedkope adapters en SMA-N-verlopen).
Een te lange pen drukt het vrouwelijke contact van de poort naar binnen; daarna maken
pennen die wel binnen tolerantie zijn geen contact meer. Een pendieptemeter kost minder
dan een nieuwe VNA.

## Onderzoeksnotities

Wie dieper wil: `docs/01-onderzoek-protocol.md` beschrijft het USB-protocol en de
metingen op beide apparaten, `docs/03-calibratie.md` het foutmodel met alle metingen en
de grenzen van de kit-fit, `docs/04-tdr.md` de wiskunde van TDR en gating,
`docs/05-roadmap.md` wat er nog komt.
