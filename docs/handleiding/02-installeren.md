# 2. Installeren en aansluiten

## Bouwen

vna-r is geschreven in Rust. Je hebt een stabiele Rust-toolchain nodig, 1.95 of nieuwer
([rustup](https://rustup.rs)). Haal de broncode op en bouw alles:

```bash
git clone https://github.com/pa1bh/vna-r.git
```

```bash
cd vna-r && cargo build --release
```

Stopt cargo met `rustc 1.xx is not supported by the following packages`, dan gebruikt het
een oudere Rust dan je denkt te hebben: een toolchain die rustup eerder installeerde gaat
voor die van je distributie. `rustup update stable` werkt hem bij.

De programma's staan daarna in `target/release/`: `vna-cli`, `vna-gui` en `vna-server`
(hoofdstuk 10). Een zelfgebouwde vna-server heeft de browserpagina alleen ingebouwd als
`scripts/build-web.sh` vóór het bouwen gedraaid heeft (zie "In de browser" verderop);
anders serveert hij een pagina die zegt dat de browserbouw ontbreekt, en verbindt het
programma er gewoon mee. Starten kan ook rechtstreeks via cargo:

```bash
cargo run --release -p vna-gui
```

```bash
cargo run --release -p vna-cli -- info
```

### macOS

Werkt zonder extra stuurprogramma's, op Apple Silicon en Intel. LiteVNA en NanoVNA-F
verschijnen als `/dev/cu.usbmodem…`.

### Linux

Voor het bouwen zijn twee systeempakketten nodig: `pkg-config` en `libudev-dev` (voor de
seriële poort). Op Debian en Ubuntu:

```bash
sudo apt-get install -y pkg-config libudev-dev
```

De bestandsdialogen van de GUI lopen via de desktop-portal (`xdg-desktop-portal` met de
backend van je bureaubladomgeving). Op een kale windowmanager zonder portal verschijnen
ze niet; installeer dan een backend zoals `xdg-desktop-portal-gtk`. De GUI bereikt de
portal via `libdbus-1.so.3`, geladen bij de eerste dialoog; elke desktopdistributie heeft
die.

De poort heet `/dev/ttyACM0` of hoger. Om hem zonder root te mogen openen, installeer je
de udev-regel uit de repository:

```bash
sudo cp linux/70-litevna.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger
```

Haal daarna de USB-stekker even los en weer vast. Een alternatief is jezelf in de groep
`dialout` (Debian/Ubuntu) of `uucp` (Arch) zetten en opnieuw inloggen.

### NixOS

De repository heeft een `flake.nix`. Flakes staan op een standaard-NixOS uit; zet in
`configuration.nix` `nix.settings.experimental-features = [ "nix-command" "flakes" ];`
of geef het per aanroep mee. Zonder iets te installeren:

```bash
nix --extra-experimental-features 'nix-command flakes' run github:pa1bh/vna-r
```

(`#vna-cli` erachter voor de commandoregel, `#vna-server` voor de server van hoofdstuk 10;
in de server uit de flake zit de browserpagina niet ingebouwd, zijn pagina zegt dat ook,
maar het programma op een computer verbindt er gewoon mee.) Om zelf te bouwen en te ontwikkelen geeft
`nix develop` een shell met Rust, pkg-config, udev en de grafische bibliotheken die
de GUI tijdens het draaien laadt; daarin werkt `cargo run --release -p vna-gui` gewoon.
Het pakket bevat de udev-regel, dus in de systeemconfiguratie volstaat

```nix
services.udev.packages = [ vna-r ];
```

met `vna-r` het pakket uit de flake, of simpeler `users.users.<naam>.extraGroups = [ "dialout" ];`.
De flake is in CI gebouwd op x86_64-linux en op een NixOS-laptop gedraaid, CLI en GUI.

## Kant-en-klare binaries

Bij elke tag `v*` bouwt GitHub Actions een archief per platform en hangt die aan een
release onder *Releases* op GitHub, met `vna-cli`, `vna-gui`, `vna-server` (met de
browserpagina ingebouwd), de handleiding (Engels en Nederlands) en op Linux de udev-regel
en een systemd-unit voor de server: `aarch64-macos` (Apple Silicon), `x86_64-macos`
(Intel-Mac), `x86_64-linux`, `aarch64-linux` (arm64: Asahi Linux op Apple Silicon,
Raspberry Pi) en `x86_64-windows` (zip). Een apart archief `…-web.zip` bevat de drie
bestanden van de browserpagina, om op een website te zetten. Het zijn gewone dynamisch gelinkte binaries: op
Linux moeten `libudev` en een desktop-portal aanwezig zijn, en op NixOS werken ze niet
(gebruik daar de flake). Tussen tags in is er alleen de CI-build, die niets
publiceert; bouw dan zelf uit de bron.

Niets is gesigneerd. **macOS** weigert daarom een binary die met de browser is gedownload
en met de Finder is uitgepakt ("kan niet worden geopend omdat de ontwikkelaar niet kan
worden geverifieerd"). Pak uit in de Terminal, dan krijgen de bestanden die markering
niet:

```bash
tar xzf vna-r-v*-aarch64-macos.tar.gz
```

of haal de markering achteraf weg:

```bash
xattr -dr com.apple.quarantine vna-r-v*-aarch64-macos
```

**Windows** toont om dezelfde reden een SmartScreen-waarschuwing; *Meer informatie ▸ Toch
uitvoeren* start het programma. De Windows-build compileert en doorloopt de CI, maar is
nog niet met een apparaat geprobeerd; meldingen zijn welkom. Poorten heten daar `COM3`
en dergelijke; Windows 10 en later hebben het USB-seriële stuurprogramma voor de LiteVNA
en de NanoVNA-F aan boord. Wat er per versie veranderd is staat in
`CHANGELOG.md`; welke versie je draait zie je onder **Help ▸ About vna-r** of met
`vna-cli --version`.

## Aansluiten

Zet het apparaat aan en verbind het met een USB-kabel die data doet; sommige laadkabels
hebben geen datalijnen en dan verschijnt er geen poort. Controleer:

```bash
cargo run --release -p vna-cli -- list
```

De regel die met `LiteVNA` of `NanoVNA` begint is het apparaat: een LiteVNA heeft USB-id
`04B4:0008`, een NanoVNA-F of -H `0483:5740`. Op macOS heet de LiteVNA 67
`/dev/cu.usbmodemv1_01`, de LiteVNA 64 `/dev/cu.usbmodemDEMO1` en de NanoVNA-F
`/dev/cu.usbmodem` gevolgd door zijn serienummer; de naam komt uit de firmware en zegt
niets over het apparaat. vna-r kiest het protocol zelf aan de hand van het USB-id. Daarna:

```bash
cargo run --release -p vna-cli -- info
```

Dat toont het model, de firmwareversie en de batterijspanning (en bij een NanoVNA-F het
bereik en het aantal punten per scan). Als dit werkt, werkt alles.

## Wat het scherm van het apparaat doet

Zodra vna-r een meting opvraagt, zet het apparaat zijn scherm op **USB MODE** en stopt het
met zelf sweepen; de knoppen reageren niet. Dat is normaal. Na een meting met de CLI, en
na **Disconnect** of het sluiten van de GUI, krijgt het apparaat zijn scherm terug. Blijft
het hangen in USB MODE, bijvoorbeeld na een crash van de computer, dan helpt
`vna-cli release` of een keer uit en aan.

Twee instellingen van het apparaat worden door vna-r aangeraakt en blijven daarna staan,
omdat ze niet betrouwbaar terug te lezen zijn: de *average* (bandbreedte) en het gemeten
kanaal. Na een sessie kan het menu van de LiteVNA dus een andere bandbreedte tonen dan
daarvoor; zet die desgewenst terug.

Een **NanoVNA-F** kent geen USB MODE: zijn scherm blijft gewoon werken. Tijdens een
meting van vna-r staat zijn eigen sweep tijdelijk op het raster van het laatste stuk dat
vna-r gemeten heeft (maximaal 301 punten per stuk; langere sweeps gaan in stukken), en
na **Disconnect**, het einde van een CLI-commando of `vna-cli release` zet vna-r de
oorspronkelijke sweep terug. Zijn bandbreedte stel je op het apparaat zelf in; het vak
**Average** in de GUI staat voor dit apparaat uit.

## Eén programma tegelijk

De seriële poort kan maar door één programma open zijn. Staat de GUI verbonden, dan geeft
de CLI een foutmelding dat de poort bezet is; klik dan eerst op **Disconnect**. Hetzelfde
geldt voor NanoVNA-Saver of een ander programma dat met het apparaat praat.

## In de browser

Dezelfde GUI is ook als WebAssembly te bouwen. Komt de pagina van vna-server, dan meet
hij, in elke browser (hoofdstuk 10). Als losse pagina is het een viewer voor
Touchstone-bestanden (.s1p, .s2p) in een browsertab, met de Smith-chart, de grafieken,
markers, referenties, TDR en gate, maar zonder alles wat een apparaat nodig heeft: geen
poort, sweep of calibratie, tot je met **Device ▸ Add server…** een server toevoegt. De
pagina opent met een voorbeeldmeting (de OCF-antenne uit hoofdstuk 6) en **Open as
trace…** laadt je eigen bestand; dat blijft in de browser, er wordt niets geüpload. Elke
release heeft de pagina als `…-web.zip`; zelf bouwen vraagt eenmalig het Rust-doel en
daarna één script:

```bash
rustup target add wasm32-unknown-unknown
scripts/build-web.sh
python3 -m http.server -d target/web 8080
```

Open daarna <http://localhost:8080>. De map `target/web` bevat drie statische bestanden,
en dat is de hele site; kopieer ze naar een willekeurige webserver. De achtergrond staat
in `docs/09-wasm-proef.md`.
