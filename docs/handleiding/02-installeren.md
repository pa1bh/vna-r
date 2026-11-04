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

Werkt zonder extra stuurprogramma's, op Apple Silicon en Intel. De LiteVNA verschijnt als
`/dev/cu.usbmodem…`.

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

## Aansluiten

Zet de LiteVNA aan en verbind hem met een USB-C-kabel die data doet; sommige laadkabels
hebben geen datalijnen en dan verschijnt er geen poort. Controleer:

```bash
cargo run --release -p vna-cli -- list
```

De regel met `VNA` ervoor is de LiteVNA (USB-id `04B4:0008`). Op macOS heet de LiteVNA 67
`/dev/cu.usbmodemv1_01` en de LiteVNA 64 `/dev/cu.usbmodemDEMO1`; de naam komt uit de
firmware en zegt niets over het apparaat. Daarna:

```bash
cargo run --release -p vna-cli -- info
```

Dat toont de identiteitsregisters, de firmwareversie en de batterijspanning. Als dit
werkt, werkt alles.

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

## Eén programma tegelijk

De seriële poort kan maar door één programma open zijn. Staat de GUI verbonden, dan geeft
de CLI een foutmelding dat de poort bezet is; klik dan eerst op **Disconnect**. Hetzelfde
geldt voor NanoVNA-Saver of een ander programma dat met het apparaat praat.
