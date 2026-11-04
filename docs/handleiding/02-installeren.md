# 2. Installeren en aansluiten

## Bouwen

vna-r is geschreven in Rust. Je hebt een recente stabiele Rust-toolchain nodig
([rustup](https://rustup.rs)). Haal de broncode op en bouw de drie onderdelen:

```bash
git clone https://github.com/pa1bh/vna-r.git
```

```bash
cd vna-r && cargo build --release
```

De programma's staan daarna in `target/release/`: `vna-cli` en `vna-gui`. Starten kan ook
rechtstreeks via cargo:

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

Voor het bouwen zijn drie systeempakketten nodig: `pkg-config`, `libudev-dev` (voor de
seriële poort) en `libgtk-3-dev` (voor de bestandsdialogen). Op Debian en Ubuntu:

```bash
sudo apt-get install -y pkg-config libudev-dev libgtk-3-dev
```

De poort heet `/dev/ttyACM0` of hoger. Om hem zonder root te mogen openen, installeer je
de udev-regel uit de repository:

```bash
sudo cp linux/99-litevna.rules /etc/udev/rules.d/ && sudo udevadm control --reload-rules && sudo udevadm trigger
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

(`#vna-cli` erachter voor de commandoregel.) Om zelf te bouwen en te ontwikkelen geeft
`nix develop` een shell met Rust, pkg-config, udev, GTK en de grafische bibliotheken die
de GUI tijdens het draaien laadt; daarin werkt `cargo run --release -p vna-gui` gewoon.
Het pakket bevat de udev-regel, dus in de systeemconfiguratie volstaat

```nix
services.udev.packages = [ vna-r ];
```

met `vna-r` het pakket uit de flake, of simpeler `users.users.<naam>.extraGroups = [ "dialout" ];`.
De flake is in CI gebouwd op x86_64-linux; op een laptop met Wayland of X11 is de GUI
nog niet gedraaid, meldingen zijn welkom.

## Kant-en-klare binaries

Bij elke tag `v*` bouwt GitHub Actions tarballs voor x86_64-Linux en Apple Silicon en
hangt die aan een release onder *Releases* op GitHub, met `vna-cli`, `vna-gui`, de
handleiding en (op Linux) de udev-regel. Het zijn gewone dynamisch gelinkte binaries:
op Linux moeten `libudev` en GTK 3 van de distributie aanwezig zijn, en op NixOS werken
ze niet (gebruik daar de flake). Tussen tags in is er alleen de CI-build, die niets
publiceert; bouw dan zelf uit de bron.

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
