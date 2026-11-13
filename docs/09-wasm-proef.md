# De GUI in de browser (WebAssembly): proef en viewer

Stand: 13 november 2026, op de branch `spike/wasm-viewer`. Het staat niet op `main` en er
is niet besloten wanneer het daar komt. Het begon als onderzoek: kan de GUI in een browser
draaien, en wat is daarvoor nodig? De variant **zonder apparaat** is daarna afgemaakt tot
iets dat op een website kan staan: een viewer voor Touchstone-bestanden, die opent met een
voorbeeldmeting. Meten met een apparaat in de browser is niet gebouwd; het laatste
hoofdstuk zegt wat dat vraagt en wat het voor de native versies betekent.

## Uitkomst in het kort

Het werkt. Dezelfde `App` draait in een browsertab, getekend met WebGPU: Smith-chart, de
drie plots, markers, referenties, TDR en gate. De ingreep is klein: geen regel in
`litevna` of `vna-cli`, een kleine vijfhonderd regels erbij in `vna-gui` (witruimte niet
meegeteld; een deel daarvan is bestaande code die naar een eigen functie is verhuisd), en
de native build doet wat hij deed: formattering, clippy met `-D warnings` op beide doelen
en alle tests zijn groen.

## Bouwen en bekijken

```sh
rustup target add wasm32-unknown-unknown   # eenmalig
scripts/build-web.sh                       # schrijft target/web
python3 -m http.server -d target/web 8080  # en dan http://localhost:8080
```

Het script bouwt `vna-gui` voor `wasm32-unknown-unknown`, laat `wasm-bindgen` de
JavaScript-lijm schrijven en zet `crates/vna-gui/web/index.html` erbij, met de
vingerafdruk van deze build ingevuld (zie Hosten). Het
commandoregelprogramma van wasm-bindgen moet dezelfde versie hebben als het crate in
`Cargo.lock`; het script bouwt het in `target/web-tools` als het er niet is (eenmalig een
minuut of twee). Er wordt niets buiten de projectmap geïnstalleerd. `trunk` is in de
egui-wereld de gebruikelijke weg en doet hetzelfde met minder regels; voor een proef
wilde ik geen extra gereedschap op het systeem.

## De viewer

Wat een bezoeker ziet, en waarin het van het programma verschilt:

- De pagina toont "loading vna-r…" tot het eerste beeld er staat, en opent dan met een
  **voorbeeldmeting**: de OCF-draadantenne uit hoofdstuk 6 van de handleiding, 3 tot
  30 MHz, 1001 punten, gemeten met de NanoVNA-F. Het bestand is
  `crates/vna-gui/web/example-ocf-antenna.s1p` en zit in de wasm ingebakken (40 kB), dus
  er is geen tweede verzoek naar de server en een test controleert dat het leest.
- De bovenbalk heeft **Open as trace…** en **Example**, de zin dat het een viewer is en
  dat bestanden op de eigen computer blijven, en een link naar de laatste release voor wie
  wil meten.
- **Weggelaten** is alles wat een apparaat nodig heeft: poortkeuze, Connect, Run, Single,
  Screenshot, het menu Device, de secties Sweep (met segmenten) en Calibration (met de
  kit-fit), View ▸ Connect at start, calibratie laden en opslaan, en File ▸ Quit.
- De calibratie is bewust helemaal weg, ook het laden. Een calibratie wordt nooit op een
  geopend bestand toegepast (`corrected` geeft een bestand ongemoeid terug) en standaarden
  meten kan niet zonder apparaat: het paneel zou alleen kunnen laden en tonen.
- De **port extension** is gebleven, onder Traces: die werkt op wat getoond wordt, dus ook
  op een bestand.
- Een bestand met één poort zet **S21** uit en een bestand met twee poorten zet het aan.
  Zonder dat is S21 van een `.s1p` een rij nullen: een vlakke lijn op −200 dB die de
  schaal oprekt. Het programma zelf doet dat (nog) niet, zie onderaan.

Hoe het in de code zit: een constante `HAS_DEVICE` (`cfg!(not(wasm32))`) in
`app/mod.rs`, en gewone `if`'s eromheen. Geen `#[cfg]` op de interface zelf, zodat beide
varianten in elke build worden gecompileerd en gecontroleerd: de native build ziet een
fout in de viewerbalk, en omgekeerd. `#[cfg]` staat alleen waar de code op het andere
platform niet bestaat (de bestandsdialogen, de thread, `main`).

## Hosten

`scripts/build-web.sh` schrijft `target/web`. Die map is de hele site: drie statische
bestanden, samen 11,8 MB.

| Bestand | Wat |
|---|---|
| `index.html` | de pagina, 2 kB |
| `vna-gui.js` | de lijm van wasm-bindgen, 159 kB |
| `vna-gui_bg.wasm` | het programma, 11,6 MB |

Zet de drie in een map op de website, bijvoorbeeld `/vna-r/`. Alle adressen zijn relatief,
dus de naam van de map doet er niet toe. Er is geen serverkant: de pagina haalt na het
laden niets meer op, stuurt niets weg en zet geen cookies. De instellingen van de viewer
staan in `localStorage` van de browser van de bezoeker.

Waar de webserver voor moet zorgen:

- **https.** WebGPU bestaat alleen in een veilige context (https of localhost). Over
  gewoon http valt de pagina terug op WebGL, en die terugval is niet getest.
- **Compressie.** Met brotli is de wasm 2,3 MB, met gzip 3,5 MB, zonder 11,6 MB. De meeste
  servers comprimeren `application/wasm` niet uit zichzelf; zet het aan of zet een
  vooraf gecomprimeerd bestand ernaast als de server dat kan aanbieden.
- **Het type `application/wasm`** voor `.wasm`. Met een ander type werkt het nog wel,
  maar trager en met een waarschuwing in de console.
- **`index.html` niet lang cachen.** De pagina vraagt de twee grote bestanden op met de
  vingerafdruk van de build in het adres (`?v=…`), dus die mogen lang in de cache blijven
  en een browser combineert nooit een oude `vna-gui.js` met een nieuwe wasm. Dat werkt
  alleen als de pagina zelf vers wordt opgehaald.

Bijwerken is opnieuw bouwen en de drie bestanden vervangen. Bouw bij voorkeur vanaf een
release-tag: het versienummer staat rechtsboven en de links onder Help wijzen naar de
handleiding van die tag.

## Wat er veranderd is

| Waar | Wat |
|---|---|
| `litevna` | niets: het crate `serialport` heeft voor onbekende platforms lege stubs, dus alles compileert; openen en poorten opsommen geven daar een fout |
| `main.rs` | de native ingang en de adapterkeuze achter `cfg(not(wasm32))`; een tweede `main` die `eframe::WebRunner` op een canvas start |
| `worker.rs` | in de browser geen thread: `spawn` geeft alleen de kanalen, niemand antwoordt |
| `app/mod.rs` | `Instant` uit `web-time` (die van std paniekt in een browser; op native is het std zelf); `HAS_DEVICE`; het apparaatdeel van de bovenbalk en van het zijpaneel elk in een eigen functie (`device_controls`, `measuring_panels`), de viewerbalk ernaast (`viewer_controls`) |
| `app/web.rs` (nieuw) | bestandsdialogen die later antwoorden: kiezen is een future die naam en bytes oplevert, opslaan is een download |
| `app/files.rs`, `app/calibration.rs` | het dialoogdeel gescheiden van wat er met het bestand gebeurt, zodat beide platforms dat tweede deel delen; de voorbeeldmeting (`open_example`); de port extension als eigen rij |
| `app/plots.rs` | geen map voor de referenties in de browser |
| `Cargo.toml` | `web-time` erbij, `env_logger` alleen native, `wasm-bindgen-futures` alleen voor wasm |
| `web/index.html`, `scripts/build-web.sh` | de pagina, en de vingerafdruk van de build in de adressen van de twee grote bestanden |
| `.github/workflows/ci.yml` | een job die de browserbuild compileert en met clippy controleert, zodat hij niet ongemerkt stukgaat |

De compiler vond op het eerste gezicht negen fouten: zes keer `rfd::FileDialog` (bestaat
niet in de browser, alleen de async-variant), `eframe::storage_dir`, `NativeOptions` en
`run_native`. Daarnaast twee dingen die wel compileren maar bij het draaien paniek geven:
`std::time::Instant` en `thread::spawn`.

## Gemeten

| | |
|---|---|
| `vna-gui_bg.wasm` (release, zonder wasm-opt) | 11,6 MB |
| idem met gzip | 3,5 MB |
| idem met brotli | 2,3 MB |
| JavaScript-lijm | 156 kB |
| releasebuild na een kleine wijziging | enkele seconden |

Een webserver die brotli of gzip aanbiedt (GitHub Pages doet gzip) brengt de download dus
op een paar megabyte. `wasm-opt` en een kleinere renderer (glow in plaats van wgpu) zijn
niet geprobeerd.

## Getest, en hoe

In de ingebouwde browser van de ontwikkelomgeving (Chromium), op macOS:

- de pagina start, renderer `BrowserWebGpu`;
- **Open as trace** met een s2p-bestand van 1001 punten: Smith-chart en plots verschijnen;
- **TDR**: de FFT rekent (rustfft in wasm);
- **Load calibration** met `n-1g.cal.json`: geladen, 201 punten, klaar voor S11 + S21;
- **Save calibration**: er komt een downloadlink met de juiste naam en grootte;
- instellingen staan in `localStorage` en komen na herladen terug.

Twee kanttekeningen bij de manier van testen. De bestandskiezer van het systeem is niet te
bedienen vanuit een geautomatiseerde browser; die stap is onderschept en het bestand is
rechtstreeks aan het invoerveld gegeven. Alles daarna (lezen, parsen, tonen) is de echte
route. En de downloadlink is gecontroleerd op naam en inhoud, maar niet aangeklikt.

Daarna door Bas in een gewone Chrome op macOS, vanaf `http://localhost:8080`, dus zonder
die omweg:

- **Open as trace** via de bestandskiezer van het systeem, met
  `hf-ocf-3-20-NanoVNA-F.s2p`: Smith-chart, log mag, SWR en fase zoals in de native versie;
- **Load calibration** via dezelfde kiezer, met `n-hf.cal.json`: geladen, 1001 punten,
  100 kHz tot 50 MHz, klaar voor S11 + S21;
- een marker plaatsen: de uitlezing (frequentie, S11, impedantie, SWR, S21) klopt met de
  grafieken.

Daarmee is de eerste kanttekening hierboven voor Chrome weggenomen: de echte
bestandskiezer werkt.

Na het afmaken van de viewer, weer in de ingebouwde browser, met een leeg `localStorage`
zoals bij een eerste bezoek:

- "loading vna-r…" blijft staan tot het eerste beeld, daarna staat de voorbeeldmeting er:
  Smith-chart, log mag, SWR en fase, S21 uit;
- de bovenbalk van de viewer en het zijpaneel zonder Sweep en Calibration;
- File heeft Open as trace, Open as reference, Save Touchstone en Open example, en geen
  calibratie of Quit;
- de pagina laadt de twee bestanden via de adressen met de vingerafdruk.

**Niet getest:** Edge, Firefox en Safari; opslaan in een gewone browser (de download
zelf); de terugval naar WebGL (zit erin via `egui-wgpu`, maar is niet gezien); Windows en
Linux; een telefoon of een smal venster (de zin in de bovenbalk is lang); een echte
webserver met https en compressie; de nieuwe CI-job op GitHub (die draait pas bij een
pull request of op `main`). Het native programma is na het verhuizen van de balk en het
zijpaneel gebouwd, getest en met clippy nagekeken, maar niet opnieuw bekeken met een
apparaat eraan.

## Wat nog niet goed is

- **Het spoor blijft niet bewaard.** Na herladen staat de voorbeeldmeting er weer, niet
  het bestand dat de bezoeker had geopend. Hetzelfde geldt voor **referenties**: op native
  staan ze als bestanden naast de instellingen, in de browser is er geen map. Ze zouden in
  `localStorage` of IndexedDB moeten.
- **Opslaan** gaat via het kaartje van `rfd` ("click here to download your file", Ok,
  Cancel): een klik extra en niet in de stijl van het programma. Een eigen downloadlink is
  een paar regels. Touchstone wordt altijd `.s2p`, want de naam ligt vast.
- **Slepen** van een bestand naar het venster werkt niet; in een browser verwacht je dat.
- **Het starten duurt even.** In de testbrowser enkele seconden tussen de pagina en het
  eerste beeld; in die tijd staat er nu "loading vna-r…". Hoe lang het in een gewone
  browser op een gewone verbinding duurt is niet gemeten.
- **S21 bij één poort.** De viewer zet S21 uit bij een `.s1p`; het programma tekent dan
  nog de vlakke lijn op −200 dB. Beter is één regel voor beide: geen S21 tekenen als het
  bestand er geen heeft. Dat verandert het gedrag van het programma en is daarom hier niet
  gedaan.
- De knoppen **max S21** en **min S21** boven de grafieken staan er ook als er geen S21 is.

## Als we verder gaan: het apparaat

Dit is het echte werk, en de proef heeft er niets van gedaan. Web Serial bestaat alleen in
Chromium-browsers (Chrome, Edge, Brave, Opera, ook op Android), niet in Firefox en Safari.
Het vraagt https of localhost en een klik van de gebruiker; filteren op VID/PID kan.

Het knelpunt zit aan onze kant. Web Serial is async en een tab heeft geen threads, terwijl
`LiteVna` en `NanoVnaShell` blokkerend lezen met timeouts en de GUI het apparaat op een
eigen thread heeft. De grens is smal (in `device.rs` drie functies: `send`, `recv_exact`,
`discard_input`; in `shell.rs` vijf leeslussen en twee schrijfplekken), maar async kleurt
door naar alle publieke functies die ze aanroepen.

De route die het minst kapot maakt:

1. een eigen `Port`-trait in `litevna` (async schrijven, lezen met timeout, leegmaken,
   wachten) in plaats van `serialport::SerialPort`; `FakePort` laat al zien dat de driver
   tegen een trait kan praten;
2. de driverkern async; native blokkeert de `Port` gewoon en draait de bestaande thread
   `block_on`, de CLI idem, dus daar verandert het gedrag niet;
3. een `Port` op Web Serial met één doorlopende leeslus die een buffer vult (een
   `read()` is niet af te breken zonder data te verliezen);
4. de worker als async taak, op native in de thread en in de browser via `spawn_local`;
5. poortkeuze via `requestPort`/`getPorts`, en de `connect`/`disconnect`-gebeurtenissen
   in plaats van kijken of het pad nog bestaat.

Af te raden: de blokkerende code laten staan en met wasm-threads in een Web Worker
draaien. Dat vraagt speciale serverheaders en bouwvlaggen en is de minst stabiele hoek
van wasm.

Grove omvang: ongeveer een week gericht werk, inclusief testen op de LiteVNA 67 en de
NanoVNA-F. Voor native levert het niets op; de enige reden is de browser.

### Wat het voor de native versies betekent

De native versies zijn het hoofdproject, dus dit is de vraag die telt. De inschatting,
zonder dat er iets van gebouwd is:

- **Het gedrag hoeft niet te veranderen.** De native poort blokkeert binnen de async
  functies precies zoals nu, met dezelfde `serialport` en dezelfde timeouts. Er komt geen
  runtime bij, alleen een minimale uitvoerder. `Vna` is een enum en geen trait-object, dus
  async vraagt geen boxing.
- **De ombouw is begrensd.** Van de ruim 6700 regels in `litevna` raakt het de ongeveer
  2650 van `device.rs`, `shell.rs`, `vna.rs` en `discovery.rs`. Calibratie, tijddomein en
  Touchstone blijven zoals ze zijn. De 38 tests van beide drivers draaien al tegen
  `FakePort` en blijven het vangnet.
- **Het risico zit in de overgang.** Die raakt het moeizaamst verworven deel: de oude
  FIFO-records, het herverbinden na USB-uitval, het wachten bij een flapperende poort en
  de aanloopscan van de NanoVNA-F. `FakePort` test geen echte timing, dus een terugval
  zie je pas met hardware.
- **De blijvende last** is bescheiden maar echt: `.await` door de driver en de CLI, een
  tweede poort die niet in CI te testen is, een QC-procedure die voor alles met een
  apparaat verdubbelt, en meer vragen van mensen met een onbekende browser.
- **Niet doen:** een aparte driver voor de browser. Dan staat de kennis over de
  eigenaardigheden van de apparaten op twee plekken.

In stappen die elk op zichzelf staan: eerst de viewer hosten (dit document, raakt de
driver niet), dan de driver ombouwen op `main` als zuivere refactor met volledige QC en
een eigen release, en pas daarna de Web Serial-poort. De tweede stap is het
beslismoment: bevalt het resultaat niet, dan is er nog niets in de browser gestoken.
