# 8. Commandoregel: vna-cli

Alle commando's accepteren `-p, --port <PORT>` (anders wordt de LiteVNA automatisch
gevonden) en `--trace` (print elke byte die over USB gaat, voor foutzoeken). `--help` bij
elk commando geeft de volledige lijst.

Frequenties: `50k`, `1M`, `14.2M`, `1.3G`, `6,3G`. Punten: 2 tot 65535.

Met `--json` geven de commando's `list`, `info`, `sweep`, `tdr` en `cal show` één
JSON-document op stdout in plaats van een tabel, voor gebruik in scripts; voortgang en
waarschuwingen blijven op stderr. Een sweep geeft elk punt als
`{"freq_hz": …, "s11": [re, im], "s21": [re, im]}`, `tdr` de gevonden reflecties onder
`peaks` en de hele curve onder `trace`.

## list

Toont de seriële poorten en markeert de VNA.

```bash
vna-cli list
```

## info

Identiteit en batterijspanning: op een LiteVNA de identiteitsregisters (variant,
protocolversie, hardwarerevisie, firmware), op een NanoVNA-F model, firmware, build,
bereik en punten per scan.

```bash
vna-cli info
```

## peek

Leest een register van een LiteVNA, voor wie het protocol wil verkennen (`docs/01`).
Een NanoVNA-F heeft geen registers; die verken je met de probe uit `docs/06`.

```bash
vna-cli peek F0
```

```bash
vna-cli peek --size 2 5C
```

## sweep

Eén sweep, afgedrukt als tabel en desgewenst geëxporteerd.

| optie | betekenis |
|---|---|
| `--start`, `--stop`, `--points` | het raster; standaard 1–30 MHz, 101 punten, of het raster van `--cal` |
| `--cal <bestand>` | calibratie toepassen; een ander raster binnen het bereik wordt geïnterpoleerd |
| `--segment START:STOP:POINTS[@cal]` | meerdere segmenten in één trace, herhaalbaar; vervangt start/stop/points/cal |
| `--s2p <bestand>`, `--s1p <bestand>`, `--csv <bestand>` | export (`--s1p` is alleen S11); `--csv -` naar stdout |
| `--port-extension <ps>` | referentievlak naar buiten schuiven met deze enkele-weg-vertraging (hoofdstuk 4) |
| `--rows <n>` | aantal tabelregels, 0 is alles |
| `--average <1..80>` | bandbreedte van het apparaat (blijft staan); alleen LiteVNA |
| `--channel s11\|s21` | één kanaal meten; alleen LiteVNA |
| `--gate START:STOP`, `--gate-notch`, `--vf` | tijd-gate op S11 in meters enkele reis |
| `--calibrated` | de calibratie van het apparaat zelf laten toepassen (LiteVNA mode 3, NanoVNA-F zonder maskerbit 8) |
| `--fifo-all` | de LiteVNA-variant van READFIFO gebruiken |
| `--keep-usb-mode` | het apparaat niet vrijgeven na de meting (LiteVNA: scherm blijft bevroren; NanoVNA-F: eigen sweep niet teruggezet) |
| `--keep-settings` | average en kanaal van het apparaat niet terugzetten |

Op een NanoVNA-F gaan sweeps van meer dan 301 punten automatisch in stukken; reken op
ongeveer 115 punten per seconde.

Voorbeelden:

```bash
vna-cli sweep --start 100M --stop 1G --points 1001 --cal cal/n-6g3.cal.json --s2p antenne.s2p
```

```bash
vna-cli sweep --segment 100k:50M:1001@cal/n-hf.cal.json --segment 50M:6.3G:1001@cal/n-6g3.cal.json --rows 40
```

```bash
vna-cli sweep --cal cal/n-6g3.cal.json --gate 0.3:0.8 --vf 0.70 --s2p gegate.s2p
```

## cal

Calibratiestandaarden meten in een bestand, of het bestand inspecteren en bewerken.
`--file` is verplicht.

| wat | doet |
|---|---|
| `open`, `short`, `load` | de drie reflectiestandaarden; de eerste legt het raster vast (`--start/--stop/--points`) |
| `through`, `isolation` | voor S21; isolation is optioneel |
| `line-open`, `line-short` | kabel met open resp. short aan het eind, voor `fit` |
| `show` | inhoud en status afdrukken |
| `kit` | kit-definities zetten: `--open-c` fF, `--open-delay` ps, `--short-l` pH, `--short-delay` ps, `--load-r` Ω, `--through-delay` ps, `--kit-name` |
| `fit` | open/short-asymmetrie schatten uit line-open/line-short; `--f-min` (standaard 300M), `--apply`, `--as-delay` |
| `check` | wat de calibratie op de lijnmetingen overlaat: golving per band, na gating, near-end term |
| `clear` | het bestand verwijderen |

`--note "tekst"` bewaart een notitie in het bestand. Een onwaarschijnlijke
standaardmeting geeft een WARNING op stderr.

```bash
vna-cli cal open --file cal/n-hf.cal.json --start 100k --stop 50M --points 1001
```

```bash
vna-cli cal fit --file cal/n-6g3.cal.json --f-min 300M --apply
```

```bash
vna-cli cal check --file cal/n-6g3.cal.json
```

## tdr

Sweept (of leest een bestand), transformeert S11 naar het tijddomein en somt de sterkste
reflecties op met afstand, |Γ| en de impedantie erachter.

| optie | betekenis |
|---|---|
| `--cal <bestand>` | sterk aanbevolen |
| `--vf <0.1..1>` | verkortingsfactor, standaard 0,70 |
| `--window rectangular\|hann\|blackman` | standaard hann |
| `--peaks <n>` | aantal reflecties, standaard 5 |
| `--max-distance <m>` | niet verder kijken dan dit |
| `--csv <bestand>` | afstand/tijd/impuls/stap/impedantie |
| `--from <bestand>` | S11 uit een .s1p/.s2p in plaats van sweepen |

```bash
vna-cli tdr --cal cal/n-6g3.cal.json --start 50k --stop 6.3G --points 1001 --vf 0.70
```

## screenshot

Het LCD van het apparaat als PNG. `--swap-bytes` draait de byte-volgorde van de pixels
om als de kleuren verkeerd zijn.

```bash
vna-cli screenshot --out scherm.png
```

## release

Geeft het scherm terug aan het apparaat: een LiteVNA die in USB MODE is blijven hangen
verlaat die, een NanoVNA-F krijgt zijn eigen sweep terug en gaat weer sweepen.

```bash
vna-cli release
```

## Exitcodes en uitvoer

Tabellen en data gaan naar stdout, meldingen en waarschuwingen naar stderr, zodat
`--csv -` schoon in een pijp te gebruiken is. Een fout geeft exitcode 1 met een
leesbare melding.
