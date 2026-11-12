# Proef: de GUI in de browser (WebAssembly)

Stand: 13 november 2026. Dit is **onderzoek**, op de branch `spike/wasm-viewer`. Het staat
niet op `main` en er is niet besloten dat het daar komt. De vraag was: kan de GUI in een
browser draaien, en wat is daarvoor nodig? De proef beantwoordt de eerste helft voor de
variant **zonder apparaat**: een viewer voor Touchstone- en calibratiebestanden.

## Uitkomst in het kort

Het werkt. Dezelfde `App` draait in een browsertab, getekend met WebGPU: Smith-chart, de
drie plots, markers, TDR en de calibratie uit een bestand. De ingreep is klein: geen
regel in `litevna`, ruim driehonderd regels erbij in `vna-gui` (waarvan een kleine
honderd in één nieuw bestand), en de native build doet wat hij deed (formattering, clippy met `-D warnings` op beide doelen en alle tests zijn groen).

## Bouwen en bekijken

```sh
rustup target add wasm32-unknown-unknown   # eenmalig
scripts/build-web.sh                       # schrijft target/web
python3 -m http.server -d target/web 8080  # en dan http://localhost:8080
```

Het script bouwt `vna-gui` voor `wasm32-unknown-unknown`, laat `wasm-bindgen` de
JavaScript-lijm schrijven en zet `crates/vna-gui/web/index.html` erbij. Het
commandoregelprogramma van wasm-bindgen moet dezelfde versie hebben als het crate in
`Cargo.lock`; het script bouwt het in `target/web-tools` als het er niet is (eenmalig een
minuut of twee). Er wordt niets buiten de projectmap geïnstalleerd. `trunk` is in de
egui-wereld de gebruikelijke weg en doet hetzelfde met minder regels; voor een proef
wilde ik geen extra gereedschap op het systeem.

## Wat er veranderd is

| Waar | Wat |
|---|---|
| `litevna` | niets: het crate `serialport` heeft voor onbekende platforms lege stubs, dus alles compileert; openen en poorten opsommen geven daar een fout |
| `main.rs` | de native ingang en de adapterkeuze achter `cfg(not(wasm32))`; een tweede `main` die `eframe::WebRunner` op een canvas start |
| `worker.rs` | in de browser geen thread: `spawn` geeft alleen de kanalen, niemand antwoordt |
| `app/mod.rs` | `Instant` uit `web-time` (die van std paniekt in een browser; op native is het std zelf) |
| `app/web.rs` (nieuw) | bestandsdialogen die later antwoorden: kiezen is een future die naam en bytes oplevert, opslaan is een download |
| `app/files.rs`, `app/calibration.rs` | het dialoogdeel gescheiden van wat er met het bestand gebeurt, zodat beide platforms dat tweede deel delen |
| `app/plots.rs` | geen map voor de referenties in de browser |
| `Cargo.toml` | `web-time` erbij, `env_logger` alleen native, `wasm-bindgen-futures` alleen voor wasm |

De compiler vond op het eerste gezicht negen fouten: zes keer `rfd::FileDialog` (bestaat
niet in de browser, alleen de async-variant), `eframe::storage_dir`, `NativeOptions` en
`run_native`. Daarnaast twee dingen die wel compileren maar bij het draaien paniek geven:
`std::time::Instant` en `thread::spawn`.

## Gemeten

| | |
|---|---|
| `vna-gui_bg.wasm` (release, zonder wasm-opt) | 11,7 MB |
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

**Niet getest:** Edge, Firefox en Safari; opslaan in een gewone browser (de download
zelf); de terugval naar WebGL (zit erin via `egui-wgpu`, maar is niet gezien); Windows en
Linux; een telefoon.

## Wat nog niet goed is

- **Geen apparaat.** De poortkeuze, Connect, Run en Single staan er nog en doen niets.
  Lege grafieken zeggen dat er in de browser alleen bestanden zijn.
- **Referenties** blijven niet bewaard: op native staan ze als bestanden naast de
  instellingen, in de browser is er geen map. Ze zouden in `localStorage` of IndexedDB
  moeten.
- **De calibratie komt na herladen niet terug**: de instellingen onthouden een pad, en
  een browser geeft alleen een naam.
- **Opslaan** gaat via het kaartje van `rfd` ("click here to download your file", Ok,
  Cancel): een klik extra en niet in de stijl van het programma. Een eigen downloadlink is
  een paar regels. Touchstone wordt altijd `.s2p`, want de naam ligt vast.
- **Calibratie per segment** en **screenshot opslaan** zijn uitgeschakeld (beide horen
  bij een apparaat).
- **File ▸ Quit** doet niets.
- In de testbrowser bleef het canvas na het laden soms een seconde of twee donker
  voordat het eerste beeld kwam. Niet uitgezocht; kan aan de testbrowser liggen.

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
