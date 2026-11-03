# TDR: reflecties op afstand zien

*Hoort bij `crates/litevna/src/timedomain.rs`, `vna-cli tdr` en de TDR-weergave in de GUI.*

## 1. Van S11(f) naar afstand

Een VNA meet in het frequentiedomein. De inverse Fourier-transformatie van S11(f) is de
**impulsrespons** h(t): wat er terugkomt als je een oneindig korte puls de kabel in stuurt. Elke
overgang (connector, knik, einde van de kabel, kortsluiting) geeft een piek op zijn eigen looptijd.
Met de velocity factor wordt looptijd afstand: `d = t · c · VF / 2` (de 2 omdat het signaal heen en
terug reist).

Twee getallen uit de sweep bepalen alles:

| sweep-parameter | bepaalt | voorbeeld 50 kHz – 6,3 GHz, 1001 pt |
|---|---|---|
| **span** | resolutie ≈ 1/span | 159 ps ≈ 1,7 cm bij VF 0,70 |
| **stap Δf** | eenduidig bereik ≈ 1/Δf | 159 ns ≈ 17 m |

Wil je verder kijken: meer punten (kleinere stap). Wil je fijner kijken: bredere span. Een reflectie
voorbij het bereik "vouwt terug" op een verkeerde afstand (aliasing).

## 2. Twee weergaven

**Bandpass-impuls.** De magnitude van de complexe IFFT van de gemeten band. Werkt met elk raster,
heeft geen DC nodig en laat zien *waar* reflecties zitten en hoe sterk (|Γ| in dB). Dit is wat
NanoVNA-firmware en -software standaard tonen.

**Lowpass-stapresponse.** Hiervoor wordt het spectrum eerst herbemonsterd op een harmonisch raster
(k·Δf), naar DC geëxtrapoleerd en conjugeerd-symmetrisch gespiegeld, zodat de IFFT reëel is. De
lopende som van die impuls is Γ(t) **met teken**, en daaruit volgt de impedantie langs de kabel:
`Z(t) = Z0 · (1 + Γ) / (1 − Γ)`. Zo zie je 50 Ω kabel, een 75 Ω stuk, een open (Z → ∞) of een
short (Z → 0) als niveaus.

## 3. Vensters

De sweep is een hard afgekapt stuk spectrum; zonder venster krijgt elke piek zijlobben die zwakke
reflecties ernaast maskeren. Hann (standaard) halveert de resolutie maar drukt de zijlobben tot
-31 dB; Blackman gaat tot -58 dB maar verbreedt nog iets meer. Rectangular geeft de scherpste piek
en de slechtste zijlobben.

## 4. Gebruik

```bash
./target/release/vna-cli tdr --cal cal/n-6g3.cal.json --vf 0.70
```

```bash
./target/release/vna-cli tdr --cal cal/n-6g3.cal.json --vf 0.70 --max-distance 3 --csv tdr.csv
```

Zonder `--cal` zie je ook de reflecties van de brug zelf (directiviteit) als valse piek op 0 m.
In de GUI: kies **TDR** in het linkerpaneel; velocity factor, venster en maximale afstand zijn daar
instelbaar en een klik plaatst een marker met afstand, |Γ| en Z.

Velocity factors: RG-58 0,66; RG-316/RG-174 0,69–0,70; RG-213 0,66; LMR-400 0,85; Aircell 7 0,83.
Ken je de fysieke lengte, dan is TDR juist een manier om de VF van een onbekende kabel te bepalen.

## 5. Gemeten op de LiteVNA 67 (3 november 2026)

Testkabel (≈ 53 cm) met de Short van de N-kit aan het eind, op poort 1.

| meting | span / punten | resolutie | sterkste reflectie | |Γ| | Z erachter |
|---|---|---|---|---|---|
| gecalibreerd (n-6g3, 40 fF kit) | 50 kHz–6,3 GHz, 1001 | 1,7 cm | **0,531 m** | -1,0 dB | 1,5 Ω |
| gecalibreerd (n-1g) | 1 MHz–1 GHz, 201 | 10,5 cm | 0,533 m | -0,5 dB | 0,8 Ω |
| ongecalibreerd | 50 kHz–6,3 GHz, 1001 | 1,7 cm | 0,579 m | -2,0 dB | 4,5 Ω |

Impedantieprofiel (6,3 GHz): 50,0 ± 0,2 Ω over de hele kabel, bij 0,50 m een kleine stoot
(+1,3 Ω, de connector), en tussen 0,52 en 0,56 m de val naar 0 Ω. De -1,0 dB van de piek is de
heen-en-terug-demping van kabel plus verloop bij hoge frequentie; bij de 1 GHz-meting is dat -0,5 dB.

Leerpunten uit de vergelijking:

- **Ongecalibreerd ligt alles 5 cm te ver** (0,579 m): het referentievlak zit dan ergens in de brug,
  niet op de connector. Bovendien verschijnt de directiviteit als valse reflectie van -25 dB op 0 m.
- **Resolutie volgt de span.** Met 1 GHz span is de piek 10 cm breed, met 6,3 GHz 1,7 cm; de afstand
  zelf klopt in beide gevallen op 2 mm.
- De **zijlobben van het Hann-venster** (-31 dB op ±4 cm van de piek) zijn geen echte reflecties.
  Een piekzoeker moet daar rekening mee houden; `vna-cli tdr` eist minimaal één resolutie-afstand
  tussen gemelde pieken, maar de zijlobben staan er nog in. Blackman drukt ze tot -58 dB.
- Kent u de fysieke kabellengte, dan is de VF omgekeerd te bepalen: 0,531 m bij VF 0,70 betekent
  een echte lengte van `0,531 · VF_echt / 0,70`.

Twee implementatiefouten die de synthetische tests niet vingen maar de echte meting wel:
een piekzoeker die bin 0 (het referentievlak) oversloeg, en een stapresponse die de helft van een
impuls op t = 0 miste omdat die in de IFFT op "negatieve tijd" terechtkomt. Beide zijn gerepareerd
en hebben nu een test.
