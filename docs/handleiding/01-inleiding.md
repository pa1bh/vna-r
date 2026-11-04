# 1. Inleiding: wat een VNA meet

## Een golf heen, een golf terug

Een VNA stuurt een sinus van bekende frequentie in een poort en meet twee dingen: hoeveel
ervan terugkomt (reflectie) en hoeveel er aan de andere poort uitkomt (transmissie).
Hij doet dat niet alleen in sterkte maar ook in fase, vandaar *vector*. Daarna schuift
hij de frequentie een stapje op en herhaalt dat, honderden of duizenden keren: een
*sweep*.

De LiteVNA heeft twee poorten. Poort 1 zendt en meet wat terugkomt; poort 2 luistert
alleen. Daarmee meet hij twee van de vier S-parameters van een tweepoort:

- **S11**: de reflectie aan poort 1. Hoe goed is het aangesloten ding aangepast aan 50 Ω?
  Dit is de grootheid voor antennes, kabels met afsluiting en alles wat je aan één
  connector meet.
- **S21**: de transmissie van poort 1 naar poort 2. Hoeveel komt erdoor, en met welke
  vertraging? Dit is de grootheid voor filters, verzwakkers, versterkers en kabels als
  doorverbinding.

S12 en S22 (de andere richting) kan de LiteVNA niet meten; in de exportbestanden staan
ze als nul.

## De reflectiecoëfficiënt Γ

Alles wat over S11 gezegd wordt, gaat over de reflectiecoëfficiënt Γ (gamma): het
complexe getal dat zegt hoe groot en met welke fase de teruggekaatste golf is ten
opzichte van de heengaande. |Γ| = 0 betekent geen reflectie, een perfecte 50 Ω-last;
|Γ| = 1 betekent alles terug, een open of kortgesloten einde. Daartussen zit alles wat
interessant is.

Uit Γ volgen de grootheden die je in de praktijk gebruikt:

| grootheid | formule | betekenis |
|---|---|---|
| S11 in dB | 20·log₁₀\|Γ\| | retourdemping; −10 dB = 10 % van het vermogen terug, −20 dB = 1 %, −30 dB = 0,1 % |
| SWR | (1 + \|Γ\|)/(1 − \|Γ\|) | staande-golfverhouding; 1,0 is perfect, 2,0 (−9,5 dB) is de gebruikelijke grens voor zenders, 3,0 is −6 dB |
| impedantie Z | 50·(1 + Γ)/(1 − Γ) | de weerstand R en reactantie X die de poort "ziet": R + jX in ohm |
| fase | arg Γ | de hoek; samen met \|Γ\| bepaalt die waar je op de Smith-chart staat |

Het verband tussen dB en SWR is niet lineair en dat wreekt zich bij het aflezen: van SWR
1,5 naar 1,2 is 14 naar 21 dB retourdemping, een factor vijf minder teruggekaatst
vermogen. Voor afregelen is dB daarom vaak de fijnere maat; voor de vraag "mag mijn
zender hieraan?" is SWR de gewoonte.

## Transmissie: S21

S21 in dB is de versterking of demping van poort 1 naar poort 2. Een goede kabel zit vlak
bij 0 dB en zakt bij hogere frequenties langzaam weg, een verzwakker van 10 dB staat op
−10 dB, een filter laat zijn doorlaatband op bijna 0 dB en zijn stopband op −40 dB of
dieper. De fase van S21 loopt lineair af met de frequentie als er alleen vertraging in het
spel is; de helling daarvan is de **groepsvertraging** τg = −dφ/dω: de looptijd door het
onderdeel. Voor een kabel is die vlak, voor een filter loopt hij op aan de bandranden.

## De Smith-chart

De Smith-chart is het complexe vlak van Γ: het middelpunt is Γ = 0 (50 Ω), de rand is
|Γ| = 1. De cirkels en bogen erin zijn lijnen van constante R en constante X, zodat je
in één oogopslag ziet of iets te hoog- of laagohmig is (rechts of links van het midden)
en of het inductief of capacitief is (boven of onder de horizontale as). Een kabel met
iets aan het eind tekent een spiraal rond het eindpunt, omdat de fase met de frequentie
ronddraait; hoe langer de kabel, hoe sneller.

## Waarom calibreren

Het apparaat meet niet rechtstreeks Γ maar een versie ervan die door zijn eigen brug,
versterkers en connectoren is vervormd. Zonder correctie zit daar bij de LiteVNA al gauw
een fout van |Γ| ≈ 0,1 in, en boven 1 GHz veel meer. Calibreren betekent: drie bekende
afsluitingen meten (open, short, load) en daaruit per frequentie de drie fouttermen van
het apparaat berekenen, waarna elke latere meting gecorrigeerd kan worden. Hoofdstuk 4 gaat
hier uitgebreid op in. Het korte advies: meet nooit zonder, behalve om te kijken of er
überhaupt iets hangt.

## Hoe vna-r in elkaar zit

vna-r bestaat uit drie delen:

- **litevna**, de bibliotheek: het USB-protocol, sweeps, calibratie, tijddomein, afgeleide
  grootheden, Touchstone-bestanden. Geen gebruikersinterface; alles wat rekent zit hier,
  met tests.
- **vna-cli**, de commandoregel: elke functie van de bibliotheek als commando, voor
  scripts, snelle controles en voor wie liever typt.
- **vna-gui**, het venster: live traces, calibratiepaneel, markers, TDR, referenties,
  bestanden.

Calibraties, metingen en instellingen worden op de computer bewaard, niet in het
apparaat. De calibratie die op het scherm van de LiteVNA zelf staat, speelt voor vna-r
geen rol: het programma vraagt het apparaat om ruwe metingen en corrigeert zelf.
