# Client en server: een VNA aan een Raspberry Pi, de GUI in de browser

Stand: 16 november 2026. Dit is een ontwerpnotitie; er is niets van gebouwd. De vraag was
hoeveel werk een client/server-model is en of het haalbaar is: een "server" op een
Raspberry Pi met de VNA aan de USB-poort, en de bestaande browserbouw van de GUI
([docs/09](09-wasm-proef.md)) als client die daar over het LAN mee praat, bijvoorbeeld
over WebSockets. De inschattingen hieronder komen uit het lezen van de code, niet uit
meten; wat er nog te beslissen valt staat onderaan.

Besloten op dezelfde dag: het gewone programma (macOS, Linux, Windows) mag vanaf de eerste
versie ook met een server verbinden, op voorwaarde dat de interface er nauwelijks door
verandert. Hoe dat past staat onder "Een server is een poort".

## Uitkomst in het kort

Goed haalbaar, en minder ingrijpend dan de Web Serial-route uit docs/09. De knip die
ervoor nodig is zit al in de code: de GUI praat niet rechtstreeks met het apparaat, maar
stuurt commando's over een kanaal naar een worker-thread en krijgt gebeurtenissen terug.
Dat kanaal over een WebSocket leggen is het hele idee. De driver blijft blokkerend en
draait native op de Pi, dus de async-ombouw van `litevna` die docs/09 als het echte werk
en het echte risico aanmerkte is hier niet nodig.

Grove omvang: ongeveer een week gericht werk, inclusief testen op de Pi met de LiteVNA 67
en de NanoVNA-F. Dat is evenveel als de Web Serial-schatting, maar met veel minder risico,
omdat het moeizaamst verworven deel (de oude FIFO-records, het herverbinden na USB-uitval,
de aanloopscan van de NanoVNA-F) niet wordt aangeraakt.

De grootste valkuil is geen Rust maar de browser. WebGPU is er niet overal, ook niet op
https, dus de terugval naar WebGL moet werken. Dat doet hij: geprobeerd op 16 november,
zonder een regel code te veranderen. En een pagina op
https mag alleen in Chrome met een Pi over http praten. Daarom twee ingangen naast
elkaar: de Pi serveert de pagina zelf, en dezelfde pagina staat op de eigen website. Zie
"De browser: WebGL, https en twee ingangen".

## Hoe het nu zit: de knip bestaat al

`crates/vna-gui/src/worker.rs` is de enige plek waar de GUI het apparaat aanraakt. De
`App` stuurt een `Cmd` en leest `Event`s; de worker draait op een eigen thread omdat
seriële I/O blokkeert.

| Richting | Berichten |
|---|---|
| `Cmd`, GUI naar worker (8) | `Connect(poort)`, `Disconnect`, `Sweep { cfgs, purpose, continuous, repeat }`, `Stop`, `SetAverage(u8)`, `Battery`, `Screenshot` |
| `Event`, worker naar GUI (10) | `Connected { port, info, battery_mv }`, `Disconnected`, `LinkLost { port, msg }`, `SweepDone { cfgs, result, purpose, repeat }`, `Progress { done, total }`, `Cancelled { purpose }`, `Screenshot(..)`, `Battery(mv)`, `Error { msg, stops_sweep }` |

Drie dingen maken dit gunstig voor een netwerkgrens:

- **De berichten zijn al een protocol.** Het zijn gewone enums met gewone data: sweep-
  configuraties, een `SweepResult`, een `VnaInfo`, een schermafbeelding. Geen handles,
  geen callbacks, geen levensduur over de grens heen.
- **Calibratie zit aan de goede kant.** Correctie, kit-fit en interpolatie gebeuren in de
  `App` (`corrected` in `app/mod.rs`), niet in de worker. De server hoeft dus alleen ruwe
  sweeps te leveren en van calibratie niets te weten. Hetzelfde geldt voor averaging in
  software, TDR, markers, referenties en Touchstone.
- **De browserbouw bestaat.** In de browser geeft `worker::spawn` nu een paar dode kanalen
  af en staat `HAS_DEVICE` op `false`. Vervang de dode kanalen door een WebSocket en de
  apparaatdelen van de interface (poortkeuze, Run, Single, Sweep, Calibration, Screenshot)
  komen terug zoals ze op native zijn.

Wat er níet door de worker gaat en dus apart aandacht vraagt:

- De **poortlijst**: `refresh_ports` in `app/mod.rs` roept `discovery::list_ports` zelf
  aan, buiten de worker om.
- Het **repaint-verzoek**: de worker houdt een `egui::Context` vast en roept na elke
  gebeurtenis `request_repaint` aan (twee plekken). Dat is de enige egui-afhankelijkheid
  van de worker.
- **Serde.** Van de typen die de berichten dragen heeft alleen de calibratie (`Standard`
  en de cal-bestanden) nu `Serialize`/`Deserialize`. `SweepConfig`, `SweepResult`,
  `FifoPoint`, `Screenshot`, `VnaInfo`, `DeviceInfo`, `ShellInfo` en `Kind` niet.
  `num-complex` staat al met de feature `serde` in de workspace, dus `Complex64` is geen
  probleem; `Duration` serialiseert serde zelf.

## Ontwerp

```
 Raspberry Pi                                     laptop, telefoon, iPad
┌──────────────────────────────────────┐          ┌───────────────────────┐
│ vna-server                           │          │ browser               │
│                                      │  http    │                       │
│  statische bestanden ───────────────────────────▶ index.html, .js, .wasm│
│  (index.html, vna-gui.js, .wasm)     │          │                       │
│                                      │  ws      │  vna-gui (wasm)       │
│  WebSocket ◀──── Cmd ───────────────────────────── App ── ewebsock      │
│     │      ───── Event ─────────────────────────▶                       │
│     ▼                                │          │  calibratie, TDR,     │
│  worker (bestaande code) ── Vna ── USB ── VNA   │  markers, plots       │
└──────────────────────────────────────┘          └───────────────────────┘
```

### De server

Een vierde crate, `vna-server`, native, zonder GUI. Hij doet drie dingen:

1. **Het apparaat.** De bestaande worker, verhuisd uit `vna-gui` (zie Werk, stap 1), met
   één `Vna`. Alles wat de worker nu kan blijft: segmenten, herhalen, voortgang,
   annuleren door een later commando, wachten op een weggevallen apparaat en
   herverbinden.
2. **De WebSocket.** Eén luisterpoort; per verbinding een thread die binnenkomende
   `Cmd`s deserialiseert en op het kanaal van de worker zet, en `Event`s van de worker
   serialiseert en wegstuurt. Synchrone `tungstenite` volstaat; de worker is al een
   thread en er is geen reden voor tokio.
3. **De pagina.** Dezelfde drie statische bestanden die `scripts/build-web.sh` schrijft,
   ingebakken in het binaire bestand (`include_bytes!`, zoals de voorbeeldmeting nu in de
   wasm zit) en geserveerd over http op dezelfde poort. Dan is één adres genoeg,
   `http://vna.local:8080`, en is er geen tweede webserver op de Pi.

Dat de server de pagina zelf serveert is geen gemak maar een noodzaak, zie "De browser:
WebGL, https en twee ingangen" hieronder.

### De client

In de browserbouw wordt `worker::spawn` een WebSocket-client. De voor de hand liggende
bouwsteen is [`ewebsock`](https://github.com/rerun-io/ewebsock) uit de egui-wereld:
non-blocking verzenden en `try_recv`, precies de vorm van de huidige `WorkerHandle`, en
hij werkt op native én op wasm. Eén client dus voor beide: de browserbouw heeft alleen de
WebSocket, het gewone programma heeft de lokale thread en de WebSocket naast elkaar.

`HAS_DEVICE` (nu een constante op `cfg`) wordt een eigenschap van de verbinding: er is
een backend (lokale thread of server) of niet. Op native blijft hij altijd waar. In de
browser is hij waar zodra er een server is: de server waar de pagina vandaan komt, of een
server die de bezoeker op de gehoste pagina heeft toegevoegd. Zonder server is het de
viewer van docs/09. De poortkeuze wordt op een server een `Cmd::ListPorts` met een `Event::Ports`,
of valt weg als de server altijd met zijn ene apparaat verbindt (zie Besluiten).

### Een server is een poort

De interface is al vol, dus het uitgangspunt is: geen nieuw paneel, geen nieuwe knop in
de bovenbalk, geen tweede manier van verbinden. Dat kan, omdat de GUI een poort nu al
als een stuk tekst behandelt: `selected_port` is een `String`, `Cmd::Connect` krijgt die
string, de instellingen bewaren hem en `LinkLost` noemt hem. Een server is dan gewoon
een poort met een ander soort naam, `ws://vna.local:8080` in plaats van
`/dev/cu.usbmodem…`.

Wat een gebruiker van het gewone programma ziet:

- **Device ▸ Add server…** opent een klein venster met één veld voor het adres. Dat is
  het enige nieuwe onderdeel.
- Een toegevoegde server staat daarna **in de bestaande poortlijst**, onder de seriële
  poorten, herkenbaar aan zijn adres. Kiezen en **Connect** werken zoals bij een poort.
- De groene regel in de bovenbalk toont wat hij nu toont (model, firmware, accu); de
  hover noemt het adres van de server erbij.
- **Connect at start**, **Disconnect**, Run, Single, Screenshot, calibratie: ongewijzigd.
- Valt de server weg, dan staat er dezelfde oranje regel als bij USB-uitval
  ("connection dropped, waiting for …"), met dezelfde Cancel.

Wie nooit een server toevoegt ziet dus precies één verschil: een regel in het menu
Device. De balk, het zijpaneel en de sneltoetsen blijven wat ze zijn.

In de code kiest de backend achter `WorkerHandle` op het adres: een pad gaat naar de
lokale thread, een adres met `ws://` naar de WebSocket-client. De `App` zelf hoeft het
verschil niet te kennen. De periodieke poortscan blijft lokaal; de servers komen uit de
instellingen en worden aan de lijst toegevoegd. Met mDNS (later) verschijnen servers op
het LAN vanzelf in de lijst en is zelfs het venster niet meer nodig.

### Het protocol

Dezelfde enums, met serde erop. Het formaat is een keuze tussen compact en leesbaar:

- **postcard** of **bincode**: klein en snel. Een `SweepResult` van 1001 punten is
  ruwweg 100 kB (frequenties, S11, S21 en de ruwe FIFO-records). Op een LAN is dat niets;
  een LiteVNA levert zo'n sweep in een fractie van een seconde en de worker stuurt
  `Progress` na elk blok.
- **JSON**: een paar honderd kB per sweep, maar leesbaar met elk gereedschap, en de CLI
  heeft met `--json` al een JSON-vorm van een sweep. Voor een eerste versie te overwegen,
  juist omdat het debuggen op de Pi dan met een browser-devtools kan.

Een schermafbeelding (RGB565, 480×320 bij de LiteVNA) is een paar honderd kB en komt op
verzoek. Een versienummer in de begroeting voorkomt dat een oude pagina met een nieuwe
server praat, of omgekeerd.

### De Pi

- **Bouwen.** De release bouwt al een aarch64-linux-archief (op GitHubs arm64-runner) en
  dat draait op Fedora Asahi; een 64-bits Raspberry Pi OS is hetzelfde doel. Een Pi Zero
  2 W, 3, 4 of 5 kan allemaal; voor een eerste proef is wat er ligt goed genoeg.
- **USB.** `serialport` werkt op Linux/ARM; de udev-regel `linux/70-litevna.rules` gaat
  mee zoals in de Linux-tarball.
- **Dienst.** Een systemd-unit die `vna-server` start en herstart; het apparaat mag er
  later in en weer uit, dat vangt de worker al.
- **Vindbaar.** Eerst gewoon `hostnaam:poort`. mDNS (`_vna-r._tcp.local`) is leuk voor
  later en geen voorwaarde.

## Werk, in stappen

| Stap | Wat | Omvang |
|---|---|---|
| 1 | De worker uit `vna-gui` naar een eigen crate (of in `litevna` achter een feature). Het repaint-verzoek wordt een callback of een `Fn()`; de GUI geeft `ctx.request_repaint` mee. Zuivere refactor, geen gedragsverandering; de bestaande tests van de worker gaan mee. | 1 dag |
| 2 | Serde op `Cmd`, `Event`, `Purpose` en de typen die ze dragen. `Cmd::ListPorts`/`Event::Ports` erbij, en `refresh_ports` daarop. | halve dag |
| 3 | `vna-server`: luisteren, één thread per verbinding, de drie bestanden ingebakken en geserveerd, begroeting met versie en de laatste toestand (zie Besluiten). | 1 à 2 dagen |
| 4 | De client: `ewebsock` achter `WorkerHandle`, op wasm als enige backend en op native naast de lokale thread, gekozen op het adres. `HAS_DEVICE` naar runtime. Native: Device ▸ Add server… en de servers in de poortlijst. Browser: de pagina leest het adres van de server waar hij vandaan komt, en heeft hetzelfde adresveld voor de gehoste pagina. | 2 dagen |
| 5 | De Pi: build, udev, systemd, QC met LiteVNA 67 en NanoVNA-F volgens docs/08, een hoofdstuk in beide handleidingen. | 1 à 2 dagen |

Stap 1 en 2 staan op zichzelf: ze veranderen niets aan het gedrag, maken `vna-gui` kleiner
en zijn ook nuttig als de Web Serial-route uit docs/09 ooit toch gebouwd wordt. Na stap 3
en 4 is er iets te zien; stap 5 maakt het af. Het beslismoment ligt na stap 4: bevalt het
niet, dan heeft het project een schonere worker en verder niets verloren.

## Valkuilen en besluiten

**De browser** heeft een eigen hoofdstuk hieronder: WebGL, https en waar de pagina
vandaan komt.

**Eén apparaat, geen poortkeuze.** Een Pi met één VNA heeft geen poortmenu nodig: de
server verbindt met het ene apparaat dat hij vindt en meldt `Connected` aan elke client
die binnenkomt. Dat scheelt een rondje berichten en past bij het gebruik. De poortlijst
als bericht kan altijd nog.

**Meerdere kijkers.** Wie mag sturen als twee tabs open staan? Eenvoudigste regel, en
genoeg voor thuis en de club: iedereen krijgt alle gebeurtenissen, iedereen mag sturen. De
worker verwerkt commando's toch al in volgorde en een later `Sweep` annuleert het vorige.
De server bewaart de laatste `Connected`, de laatste `SweepDone` en of er continu wordt
geveegd, en speelt dat af aan een client die later binnenkomt, zodat die niet naar een
leeg scherm kijkt.

**Calibratie in de browser.** Geen bestandssysteem. Voor de eerste versie: laden en
opslaan via de bestandsdialogen van de browser, die docs/09 al heeft, plus de calibratie
in `localStorage` naast de instellingen. Mooier is dat de server cal-bestanden op de Pi
bewaart en een `Cmd::ListCalibrations`/`LoadCalibration`/`SaveCalibration` aanbiedt; dan
hoort de calibratie bij het apparaat, waar hij thuishoort. Dat is een tweede ronde.

**Twee lagen uitval.** De worker kent USB-uitval en wacht dertig seconden op het apparaat.
Daar komt een WebSocket bij die kan wegvallen (wifi, de Pi herstart). De client toont dan
"wachten op de server" zoals hij nu `LinkLost` toont, en verbindt opnieuw; de server
praat hem bij met de laatste toestand.

**Beveiliging.** Iedereen op het LAN kan de VNA bedienen en de metingen zien. Voor een
eerste versie is dat acceptabel als de handleiding het zegt en de server standaard alleen
op het LAN luistert. Een token in het adres (`?key=…`) is later een paar regels; https en
echte accounts zijn voor dit project geen doel.

**Native verbindt ook** (besloten 16 november). Het gewone programma krijgt de
WebSocket-client vanaf de eerste versie, niet pas later. De voorwaarde is dat het de
interface niet zwaarder maakt: één menuregel en een regel in de poortlijst, verder niets.
Wat daar niet in past (een overzicht van servers, instellingen per server, een eigen
statusvenster) hoort niet in de eerste versie.

**Niet doen.** Een tweede driver of een tweede worker voor de server. De hele winst van
dit ontwerp is dat de kennis over de apparaten op één plek blijft en de server dezelfde
code draait als het programma.

## De browser: WebGL, https en twee ingangen

Nagezocht op 16 november 2026 in de documentatie van de browsers; niets hiervan is
getest. De bronnen staan onderaan dit hoofdstuk.

### WebGL is geen keuze maar een eis

De browserbouw tekent via wgpu. Dat kent in de browser twee wegen: WebGPU als de browser
het heeft, anders WebGL2. Het omschakelen gebeurt vanzelf bij het starten. eframe 0.37
bouwt standaard alleen wgpu mee (de eigen OpenGL-renderer `glow` zit er niet in), dus die
terugval is al deel van de huidige build.

**Geprobeerd op 16 november** (branch `spike/webgl-fallback`, verslag in docs/09 onder
Getest): dezelfde build over http op een LAN-adres, dus zonder veilige context. De
renderer is dan `Gl` op WebGL 2.0 in plaats van `BrowserWebGpu`; de voorbeeldmeting,
markers, TDR, zoomen en slepen werken en het beeld is gelijk aan dat met WebGPU. Dat was
in Chromium op een Mac. Firefox, Safari en een echte telefoon of tablet zijn niet gezien.

Voor vna-r maakt het in beeld en snelheid geen verschil: het programma tekent 2D (lijnen,
tekst, een Smith-chart) en WebGPU levert pas iets op bij zwaar 3D-werk of rekenen op de
videokaart.

De terugval is op twee manieren nodig, en de tweede staat los van http of https:

- **Geen veilige context.** WebGPU bestaat alleen over https of op localhost. Een pagina
  van `http://vna.local:8080` krijgt het dus nooit.
- **De browser heeft het niet.** Ook op https is WebGPU niet overal:

| Browser | WebGPU | https-pagina naar `ws://` op het LAN |
|---|---|---|
| Chrome en Edge, desktop | ja; op Linux nog deels | ja, na een toestemmingsvraag (vanaf 147) |
| Chrome op Android | vanaf Android 12 | niet nagegaan |
| Safari, en alles op iPhone en iPad | vanaf Safari 26 | nee |
| Firefox | Windows en Apple Silicon; niet Linux en Android | voor zover na te gaan nee |

Een oudere iPad of een Linux-laptop met Firefox komt dus hoe dan ook bij WebGL uit, en de
telefoon en de tablet in de tuin zijn juist waar dit ontwerp voor is.

De keuze die wel bestaat is welke WebGL-weg: de terugval van wgpu, of voor de webbouw de
`glow`-renderer van egui. Die tweede geeft naar verwachting een kleinere download (niet
gemeten) en is in browsers de oudste weg van egui, maar het is een tweede renderer naast
wgpu op native. De terugval werkt, dus er verandert niets: de webbouw blijft op wgpu.

### Een https-pagina en een Pi over http

Een browser laat een pagina die over https is geladen in principe geen onversleutelde
WebSocket (`ws://`) openen: mixed content. Chrome maakt daar een uitzondering op voor het
eigen netwerk (Local Network Access): een IP-adres uit een privébereik, een naam op
`.local` of `localhost`, nadat de gebruiker toestemming heeft gegeven. Voor WebSockets
geldt dat vanaf Chrome 147. Safari en Firefox hebben die uitzondering niet, voor zover na
te gaan; daar blijft het mixed content en dus geblokkeerd.

Https op de Pi zelf (`wss://`) lost het voor alle browsers op, maar vraagt een certificaat
voor een apparaat op het LAN. Zelfondertekend geeft elke bezoeker een waarschuwing, en bij
een WebSocket niet eens een knop om door te gaan. Een echt certificaat kan met een eigen
domeinnaam of via een dienst als Tailscale, maar dat is inrichting per gebruiker en geen
doel voor de eerste versie.

### Twee ingangen

Bas host de viewer toch al op https (pa1bh.nl). Dat vervangt de pagina vanaf de Pi niet,
het komt ernaast:

| | Pagina vanaf de Pi (`http://vna.local:8080`) | Pagina op de website (https) |
|---|---|---|
| Browsers die kunnen meten | alle | Chrome en Edge, na toestemming |
| Zonder internet (Pi bij de antenne) | ja | nee |
| Renderer | WebGL | WebGPU waar het kan, anders WebGL |
| Versie van de client | die van de server, past altijd | de nieuwste; de begroeting moet het verschil opvangen |
| Adres van de server | de pagina weet waar hij vandaan komt | invullen, zoals Add server… op native |

Wat hosten op https nog meer geeft:

- **Altijd de nieuwste client**, zonder dat de Pi wordt bijgewerkt. De versiecontrole in
  de begroeting wordt daardoor belangrijker: een nieuwe pagina moet met een oudere server
  kunnen praten, of duidelijk zeggen dat het niet gaat.
- **Web Serial blijft mogelijk.** Dat vraagt een veilige context; dezelfde pagina kan
  later ook rechtstreeks met een VNA aan USB praten (docs/09).
- **Installeren en offline.** Een pagina op https kan zich als app laten installeren en
  zichzelf bewaren voor gebruik zonder internet (service worker). Niet gebouwd.
- **De server op de eigen computer.** Een https-pagina mag met `localhost` praten. Wie
  Firefox gebruikt en dus geen Web Serial heeft, kan `vna-server` lokaal draaien en de
  gehoste pagina gebruiken. Voor Safari niet nagegaan.
- **Eén link om te delen**, met de viewer en de voorbeeldmeting voor wie geen apparaat
  heeft.

De server blijft de pagina dus zelf serveren (stap 3), en de browserbouw krijgt hetzelfde
adresveld als native voor het geval hij niet van een server komt. Dat is geen extra
ontwerp: het is "Een server is een poort" in de browser.

Bronnen:
[Local Network Access (Chrome for Developers)](https://developer.chrome.com/docs/modern-web-guidance/guides/security/local-network-access),
[de toestemmingsvraag (Chrome-blog)](https://developer.chrome.com/blog/local-network-access),
[Intent to Ship voor WebSockets](https://groups.google.com/a/chromium.org/g/blink-dev/c/O6GMKt44Ups),
[WebGPU in de grote browsers (web.dev)](https://web.dev/blog/webgpu-supported-major-browsers),
[stand per browser (gpuweb-wiki)](https://github.com/gpuweb/gpuweb/wiki/implementation-status).

## Waar het voor is

- De VNA aan het voedingspunt van de antenne, met een Pi Zero 2 W en een powerbank;
  afregelen vanaf een telefoon of iPad in de tuin. Werkt in Safari en Firefox, wat Web
  Serial nooit zal doen.
- De VNA in de shack of op zolder, meten vanaf de bank of de werkbank.
- Meerdere mensen die één meting zien: een clubavond, uitleg, een cursus.
- Een server die zonder GUI doorveegt en SWR over de tijd logt (regen, wind, opwarming).
  Dat is in wezen `vna-cli` die blijft draaien, en kan ook zonder client.

## Vergeleken met de Web Serial-route (docs/09)

| | Web Serial | Client/server |
|---|---|---|
| Extra apparaat | nee | ja, een Pi (of elke computer met USB) |
| Driver | async ombouwen, ~2650 regels geraakt | ongemoeid |
| Browsers | alleen Chromium | alle |
| Telefoon, tablet | Android met Chrome | alles met een browser |
| Vereist | https of localhost, een klik per sessie | http vanaf de Pi volstaat (met WebGL); vanaf een https-pagina alleen Chrome en Edge |
| Meerdere kijkers | nee | ja |
| Omvang | ~1 week, hoog risico in de overgang | ~1 week, laag risico |

De twee sluiten elkaar niet uit en delen stap 1 en 2. Als er maar één komt, is dit de
betere eerste.

## Wat het voor de native versies betekent

Weinig, en dat is de bedoeling. De worker verhuist en krijgt een callback in plaats van
een `egui::Context`; het programma op de Mac en op Linux doet wat het deed. Serde op de
datatypen is onzichtbaar voor wie het niet gebruikt. Het formaat van de instellingen en
van de calibratiebestanden verandert niet; een server is in de instellingen een poort met
een adres als naam, plus een lijst van toegevoegde servers.

Zichtbaar is één ding: Device ▸ Add server…, en daarna de server als regel in de
poortlijst (zie "Een server is een poort"). Wie met een kabel meet merkt verder niets.

Het risico voor bestaande gebruikers zit niet in stap 4 maar in stap 1: daar verhuist de
code met het herverbinden en de flapperende poort. Hij verhuist ongewijzigd en de tests
van de worker gaan mee, maar een QC-ronde met beide apparaten hoort erbij. Stap 1 en 2
kunnen als eigen release uit voordat de server bestaat.

## Open vragen

- Formaat van het protocol: postcard (compact) of JSON (leesbaar). Neiging: JSON voor de
  eerste versie, om de debugbaarheid; wisselen is later één regel per kant.
- ~~Werkt de WebGL-terugval van wgpu goed genoeg, of wordt de webbouw `glow`?~~ Ja,
  geprobeerd op 16 november in Chromium op een Mac; de webbouw blijft op wgpu. Nog te
  doen: dezelfde pagina op een echte telefoon en tablet (Safari op iOS, Chrome op Android)
  en in Firefox.
- De indeling op een telefoon. Rechtop (375 punten breed) vult het zijpaneel het hele
  scherm en zijn de grafieken niet te zien; dwars is het krap. Voor "afregelen vanaf een
  telefoon in de tuin" moet het zijpaneel op een smal scherm kunnen inklappen of onder de
  grafieken komen. Dat staat los van de server en van de renderer, en het raakt de
  interface van het programma: een eigen besluit.
- Doet de uitzondering van Chrome wat de documentatie zegt, met een naam op `.local` en
  met een IP-adres, ook op Android? En wat doen Safari en Firefox precies met `ws://`
  naar `localhost` en naar het LAN? Proberen zodra er een server is.
- Welke Pi ligt er, en welk OS? Een 64-bits systeem is nodig voor het bestaande
  aarch64-archief.
- Of de server ook de CLI-taken moet kunnen (een sweep naar een bestand op de Pi, loggen
  over tijd), of dat dat een apart programma blijft dat `litevna` gebruikt.
