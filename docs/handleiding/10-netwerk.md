# 10. Meten via het netwerk

## Waar het voor is

De analyzer hoeft niet naast de computer te staan waarmee je kijkt. Zet hem aan een kleine
computer waarop **vna-server** draait, een Raspberry Pi bijvoorbeeld, en meet vanaf een
laptop, tablet of telefoon op hetzelfde netwerk:

- aan het voedingspunt van de antenne, in de mast of in de tuin, met een Pi en een
  powerbank, terwijl je afregelt en de SWR op een tablet volgt;
- in de shack of op zolder, terwijl je zelf ergens anders zit;
- met meer mensen die naar één meting kijken, op een clubavond of een cursus.

De server stuurt de analyzer precies zo aan als het programma via USB doet; aan de meting
zelf verandert niets. Kijken doe je met het programma vna-r op een computer, of met een
browser, en dan hoeft er niets geïnstalleerd te worden.

## Wat je nodig hebt

- Een LiteVNA of een NanoVNA-F, zoals voor al het andere.
- Een computer voor de server, met een USB-poort: een Raspberry Pi 3, 4, 5 of Zero 2 W
  met een 64-bits Raspberry Pi OS (Bookworm of nieuwer), of een willekeurige computer met
  Linux, macOS of Windows.
- Een netwerk waar ze allebei op zitten: de wifi thuis, de hotspot van een telefoon.
- Iets om mee te kijken: het programma vna-r op een computer, of een browser.

## vna-server op een Raspberry Pi

Haal van de Releases-pagina het archief `…-aarch64-linux.tar.gz`. Daarin zitten
`vna-server`, de udev-regel en `vna-server.service`, die de server start telkens als de Pi
opstart. Op de Pi, in de map waar je het archief neerzette:

```bash
tar xzf vna-r-v*-aarch64-linux.tar.gz && cd vna-r-v*-aarch64-linux
```

```bash
sudo install -m 755 vna-server /usr/local/bin/
sudo cp 70-litevna.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger
sudo cp vna-server.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now vna-server
```

De server draait nu, en start opnieuw na elke herstart van de Pi en na een storing. Hij
zoekt elke twee seconden zelf naar de analyzer, dus het maakt niet uit of je die eerder of
later aansluit. Wat hij doet staat in het log:

```bash
journalctl -u vna-server -f
```

Daar zie je op welk adres hij luistert, wanneer de analyzer verbonden is of wegvalt, en wie
er verbindt. De dienst draait als gewone gebruiker in de groep `dialout`, die de seriële
poort mag openen; de udev-regel geeft de poort van de analyzer aan die groep. `sudo
systemctl stop vna-server` stopt hem, en de analyzer krijgt dan zijn eigen scherm terug.

Het archief is gebouwd voor 64-bits ARM. Het draait niet op een 32-bits Raspberry Pi OS,
niet op de eerste Pi Zero en niet op een Raspberry Pi OS van vóór Bookworm.

## Met de hand starten

De server zit ook in de archieven voor de andere platforms, en draait op elke computer
waar de analyzer aan zit (op Windows heet het programma `vna-server.exe`):

```bash
./vna-server
```

Hij luistert op poort 8080 op elke netwerkverbinding van de computer en verbindt met de
eerste analyzer die hij vindt. Met een paar opties verander je dat:

- `--port /dev/ttyACM0` (of `COM3`): deze seriële poort, als er meer zijn;
- `--listen 192.168.1.5:9000`: een ander adres of een andere poort; met `127.0.0.1:8080`
  blijft hij op deze computer;
- `--name shack`: de naam die de kijkers zien (standaard de naam van de computer);
- `--simulate`: een nagebootste analyzer, zie verderop;
- `--web <map>`: de pagina uit die map in plaats van het exemplaar in het programma.

Ctrl-C stopt hem; hij laat eerst de analyzer los. Een firewall (Windows, of macOS als de
firewall aanstaat) kan bij de eerste start vragen of het programma verbindingen mag
aannemen; sta dat toe.

## In de browser

Open `http://<naam van de Pi>:8080/` in een browser, bijvoorbeeld
`http://raspberrypi.local:8080/`. Werkt de naam niet, gebruik dan het IP-adres van de Pi
(`hostname -I` op de Pi laat het zien, en de lijst met apparaten in je router ook).

De pagina is de interface van vna-r. Hij verbindt vanzelf met zijn server, en in de
bovenbalk staat de server waar anders de poortlijst staat. Verder werkt alles zoals in
hoofdstuk 3: Run, Single, het sweeppaneel, de calibratie, de markers, TDR. De groene regel
toont de analyzer en, na `via`, de naam van de server; met de muis erop zie je de versie
van de server en de poort van de analyzer op de server.

## In het programma: Device ▸ Add server…

Kies in het programma vna-r op een computer **Device ▸ Add server…**, typ het adres van de
server (`raspberrypi.local`, `192.168.1.5`, of met een poort, `192.168.1.5:9000`; zonder
poort is 8080 bedoeld) en klik **Add**. De server staat dan in de poortlijst onder de
seriële poorten, als `server raspberrypi.local:8080`, en is gekozen: **Connect** verbindt.
In hetzelfde venster staan de servers die je kent, elk met **remove**.

De servers worden onthouden. Met **View ▸ Connect at start** verbindt het programma bij
het starten met een gekozen server, net als met een analyzer aan de USB.

De pagina op een website (in plaats van de pagina van de server) heeft dezelfde **Device ▸
Add server…**; onder "Wat deze versie nog niet heeft" lees je wanneer dat werkt.

## Met meer mensen op één analyzer

Er kunnen zoveel kijkers met één server verbinden als je wilt, programma's en browsers
door elkaar. Ze zien allemaal dezelfde analyzer:

- Iedereen kan sturen. Druk in de ene browser op **Run**, en bij de anderen verschijnt de
  knop **Stop** en komt dezelfde sweep binnen; iedereen kan op Stop drukken.
- Iedereen heeft zijn eigen calibratie. De server geeft alleen ruwe metingen door; de
  calibratie wordt in elk programma of elke browser zelf toegepast. Meet jij een
  standaard, dan krijg alleen jij het resultaat.
- Een schermafbeelding van het apparaat gaat alleen naar wie erom vroeg.
- Sluit de laatste kijker de pagina of verbreekt hij de verbinding, dan stopt de server
  met sweepen en krijgt de analyzer zijn eigen scherm terug.

## Als de regel oranje wordt

De bovenbalk zegt waar het programma op wacht:

- **"no analyser on the server yet, waiting for one…"**: de server draait, maar er zit geen
  analyzer aan. Sluit hem aan, dan verbindt het programma vanzelf. Dit is geen fout; je
  ziet het meteen na Connect als de analyzer er nog niet is.
- **"the analyser dropped off the server, waiting…"**: de analyzer is van de USB van de
  server verdwenen (kabel of stekker bewogen, of hij is herstart). De server wacht op hem,
  zoals het programma dat via USB doet (hoofdstuk 9), en een sweep die liep gaat daarna
  verder.
- **"the server lost its analyser, waiting for one…"**: de analyzer kwam niet binnen 30
  seconden terug. Sluit hem opnieuw aan.
- **"connection to the server dropped, waiting for …"**: de netwerkverbinding is weg: de
  wifi buiten bereik, of de server is herstart. Het programma probeert het twee minuten
  lang zelf opnieuw, met steeds langere pauzes, en gaat verder met Run zodra de server
  terug is. Een Raspberry Pi die herstart is meestal binnen die tijd terug.

Met **Cancel** stop je het wachten, waar het ook op wacht.

## Wat er anders is in de browser

- Bestanden gaan via de browser: **Load…** opent het bestandsvenster van de browser,
  **Save…** (calibratie, Touchstone, schermafbeelding) downloadt het bestand.
- Een segment heeft geen eigen calibratiebestand; elk segment gebruikt de calibratie van
  het calibratiepaneel, zo nodig geïnterpoleerd.
- Vastgehouden referenties blijven zolang de pagina open is, niet na herladen.
- De instellingen (sweep, weergaven, de servers die je toevoegde) bewaart de browser, per
  adres.

## Proberen zonder analyzer

`vna-server --simulate` start een server met een nagebootste analyzer: een
multiband-antenne op poort 1 en een banddoorlaatfilter op poort 2, met een beetje ruis,
op de snelheid van een LiteVNA. Calibratiestandaarden meten als ideale standaarden. Zo
probeer je de server, de browserpagina en meer kijkers tegelijk zonder analyzer. In het
programma zelf verbindt **View ▸ Developer tools** en dan **Developer ▸ Connect to
simulator** met dezelfde nagebootste analyzer binnen het programma.

## Wat deze versie nog niet heeft

- **Geen wachtwoord.** Iedereen op het netwerk kan de pagina openen, de meting zien en de
  analyzer bedienen. Gebruik het op een netwerk dat je vertrouwt: thuis, op de club, de
  hotspot van je telefoon. Met `--listen 127.0.0.1:8080` blijft het op één computer.
- **De pagina op een website.** Een pagina die over https komt, zoals een website, mag een
  onversleutelde verbinding met een apparaat op het eigen netwerk alleen openen in Chrome
  en Edge, en alleen nadat je dat toestaat; Safari en Firefox weigeren het. Open daarom
  normaal de pagina die de server zelf serveert (`http://…:8080/`); die werkt in elke
  browser.
- **Een telefoon rechtop.** De indeling is nog niet gemaakt voor een smal scherm: houd de
  telefoon dwars, of gebruik een tablet.
- **Het Nix-pakket.** In de server daaruit zit de browserpagina niet ingebouwd; de pagina
  zegt dat ook. Het programma op een computer verbindt er gewoon mee.
