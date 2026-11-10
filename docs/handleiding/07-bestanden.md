# 7. Bestanden

## Touchstone (.s1p, .s2p)

Touchstone is het uitwisselingsformaat voor S-parameters dat elk RF-programma leest:
tekst, één regel per frequentie, met een optieregel die eenheid en formaat aangeeft.

**Schrijven.** In de GUI **Save Touchstone…** onder **Files**; met de CLI
`vna-cli sweep --s2p naam.s2p`. vna-r schrijft Hz, reëel/imaginair, 50 Ω, met S12 en
S22 als nul omdat de LiteVNA die niet meet. Voor een eenpoort zoals een antenne geef je
het bestand in de GUI de extensie `.s1p` of gebruik je `--s1p naam.s1p`: dan wordt
alleen S11 geschreven. De commentaarregels vermelden of de data
gecalibreerd, gegate of gemiddeld was. Met gate aan wordt de gegate S11 geschreven.
Bij segmenten die elkaar raken wordt het gedeelde punt eenmaal geschreven, omdat
Touchstone strikt oplopende frequenties eist.

**Lezen.** **Open as trace…** toont een bestand in plaats van het apparaat: alle
weergaven, markers, TDR en gate werken erop, het label wordt **FILE**, en de calibratie
wordt niet nog eens toegepast. De volgende sweep vervangt het. **Open as reference…**
laadt het als referentie naast de live meting. De lezer begrijpt s1p en s2p in de
formaten RI, MA en DB met eenheden Hz tot GHz, ook de bestanden die de LiteVNA zelf op
zijn SD-kaart schrijft en de uitvoer van simulatieprogramma's. Een bestand met een andere
referentie-impedantie dan 50 Ω (`R 75` in de optieregel) wordt voor S11 omgerekend naar
50 Ω, zodat impedantie en SWR kloppen; S21 blijft zoals in het bestand, en de melding
onder **Files** zegt het erbij. Met de CLI:
`vna-cli tdr --from naam.s2p`.

## Calibratiebestanden (.cal.json)

Een calibratie is een JSON-bestand met het frequentieraster, de kit-definities en de ruwe
metingen van elke standaard. Dat de ruwe metingen bewaard worden en niet de fouttermen,
maakt interpolatie en de kit-fit achteraf mogelijk. GUI en CLI gebruiken hetzelfde
formaat; `vna-cli cal show --file x.cal.json` drukt de inhoud leesbaar af.

Een calibratie hoort bij één apparaat, één kit en één referentievlak. Bewaar ze
herkenbaar, bijvoorbeeld `n-hf.cal.json` en `n-6g3.cal.json`, en zet in `--note` wat
erbij hoort ("N kit, direct op poort 1"). De map `cal/` in de repository is daarvoor
bedoeld en staat in `.gitignore`.

## CSV

`vna-cli sweep --csv naam.csv` schrijft per frequentie S11 en S21 als reëel/imaginair,
dB, graden en SWR, gevolgd door de impedantie op poort 1 (R, X en |Z| in ohm) en de
groepsvertraging van S21 in nanoseconden; `--csv -` schrijft naar de standaarduitvoer. `vna-cli tdr --csv`
schrijft afstand, tijd, impuls, stap en impedantie. Voor spreadsheets en eigen scripts.

## Referenties

Referenties uit **Hold** blijven tussen sessies bewaard: vna-r schrijft ze als
`ref-0.s2p` tot `ref-3.s2p` in de map `references` naast zijn instellingen (op macOS
`~/Library/Application Support/vna-r`, op Linux `~/.local/share/vna-r`, op Windows
`%APPDATA%\vna-r`). Wil je er een voorgoed houden of doorgeven: kopieer dat bestand, of
**Save Touchstone** terwijl de meting live is en later **Open as reference…**.

## Schermafbeeldingen

**Screenshot** in de GUI en `vna-cli screenshot --out naam.png` halen het LCD van het
apparaat op als PNG: 1024×600 op de LiteVNA 67, 480×320 op de 64, 800×480 op de
NanoVNA-F. Komen de kleuren verkeerd uit (bij een ander apparaat), dan draait
`--swap-bytes` de byte-volgorde van de pixels om.

## Instellingen van de GUI

De GUI bewaart zijn instellingen in de gebruikelijke map voor applicatiegegevens van het
besturingssysteem (via eframe). Wissen is zelden nodig; **Clear** in het calibratiepaneel
en de standaardwaarden in het Sweep-paneel volstaan meestal.
