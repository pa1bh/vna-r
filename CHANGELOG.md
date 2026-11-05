# Changelog

Alle noemenswaardige wijzigingen per versie. Het versienummer staat één keer in
`Cargo.toml` (`[workspace.package] version`) en geldt voor library, CLI, GUI en de
Nix-flake; de GUI toont het in de titelbalk en onder **Help ▸ About**, de CLI met
`vna-cli --version`. Een release is een git-tag `vX.Y.Z` op `main` (zie "Een release
maken" in de README).

Opzet naar [Keep a Changelog](https://keepachangelog.com/nl/1.1.0/); versies volgen
[SemVer](https://semver.org/lang/nl/): tot 1.0 kan een minor-versie (0.x) nog dingen
veranderen die niet terugwerkend compatibel zijn, zoals het formaat van de
calibratiebestanden.

## [Unreleased]

Zes punten uit een externe code-review, alle in de meetketen en het GUI-toestandsbeheer.

### Opgelost
- Een LiteVNA-record met een foute checksum wordt niet meer als meetpunt gebruikt; het punt
  blijft leeg en komt bij de volgende FIFO-doorgang opnieuw.
- Crash bij een NanoVNA-F met meerdere segmenten en calibratie aan (de shell levert geen
  ruwe records, het opdelen per segment ging daarvan uit).
- Na **Clear** in het calibratiepaneel bleef het label CAL staan terwijl de data ruw waren.
- De software-middeling (**Sweep avg**) wordt gewist bij verbinden en verbreken, zodat sweeps
  van een vorig apparaat niet meetellen.
- Calibratiebestanden worden bij het laden gecontroleerd (raster oplopend, minstens twee
  punten, per standaard evenveel waarden als rasterpunten, eindige getallen); een kapot
  bestand geeft een melding in plaats van een crash. Frequenties als `nan` of `inf`
  worden geweigerd.
- Touchstone-bestanden met een andere referentie-impedantie dan 50 Ω (`R 75`) worden
  voor S11 omgerekend; voorheen werd de waarde genegeerd.
- TDR weigert een sweep die te smal is voor zijn topfrequentie (FFT groter dan 4 Mi
  punten) met een melding, in plaats van gigabytes te alloceren op de GUI-thread.

## [0.2.2] - 2026-11-05

Eerste terugkoppeling van een tester op Fedora verwerkt.

### Toegevoegd
- **About**, **Copy diagnostics** en het log tonen de tekenlaag (Vulkan/Metal/OpenGL, adapter,
  driver, Wayland of X11); `RUST_LOG=info` laat de keuze van wgpu op de terminal zien.
  Handleiding: wat te doen bij een haperende GUI op Linux.

### Opgelost
- Haperende menu's op een Fedora-werkstation met een oudere NVIDIA-kaart (nouveau): wgpu
  koos daar de experimentele NVK-Vulkan-driver. De GUI kiest de adapter nu zelf en mijdt
  NVK en software-rendering zodra er een alternatief is; `WGPU_BACKEND` dwingt nog steeds.

## [0.2.1] - 2026-11-05

Voor testers buiten Nederland: Engelse README en handleiding.

### Toegevoegd
- Engelse handleiding in `docs/manual`, naast de Nederlandse in `docs/handleiding`. De
  release-tarball bevat nu beide, met de afbeeldingen.
- **Help** opent de Engelse of de Nederlandse handleiding op GitHub bij de tag van de
  eigen versie, zodat de tekst bij de knoppen van het draaiende programma past.

### Gewijzigd
- README in het Engels, met downloadinstructies, snelstart en apparatentabel.
- Handleiding: de drempel voor een losse open of short is −8 dB (was −3 dB in de tekst);
  de OCF-alinea koppelt het teken van X niet meer aan de lengte als je door de
  voedingslijn meet.

## [0.2.0] - 2026-11-05

Eerste versie die aan anderen gegeven wordt om te testen.

### Toegevoegd
- Menubalk in de GUI (File, Device, View, Help) met **About vna-r**: versie,
  build-revisie, auteur en de link naar GitHub.
- **View ▸ Developer tools** zet een Developer-menu aan met een logvenster
  (verbindingen, fouten, bestandsbewerkingen, sweeps) en **Copy diagnostics**, dat
  versie, apparaat, sweep, calibratie en het log naar het klembord zet voor een
  probleemmelding. Standaard uit.
- Eén versienummer voor het hele project; `vna-cli --version` en About tonen ook de
  git-revisie. De release-workflow weigert een tag die niet bij het versienummer in
  `Cargo.toml` past en gebruikt de bijbehorende sectie uit dit bestand als release-notes.
- NanoVNA-F (tekst-shell, USB 0483:5740) naast de LiteVNA 64/67: sweeps via `scan_bin`,
  schermafbeelding, info en batterij; CLI en GUI kiezen het protocol op de USB-id.
- Δ-trace (live min referentie) in elke weergave: dB, graden, SWR, ohm, nanoseconden.
- Automatisch poorten verversen zolang er geen verbinding is.
- Nix-flake (`nix run github:pa1bh/vna-r`) met udev-regel, release-workflow voor
  x86_64-Linux en Apple Silicon, CI die ook de flake bouwt.
- Nederlandse handleiding in `docs/handleiding` met schermafbeeldingen.

### Gewijzigd
- Knoppen gebruiken tekst in plaats van symbolen die Linux als blokjes tekende.
- De melding na het opslaan van een Touchstone-bestand staat onder **Files**.
- Plausibiliteitscontrole van open/short houdt rekening met de −3 dB ruwe
  reflectie van een NanoVNA-F.

## [0.1.0] - 2026-11-03

Interne versie: LiteVNA 64/67 via het binaire registerprotocol, SOLT-calibratie met
kit-fit en interpolatie, Smith-chart en zeven weergaven, markers, referenties,
segmenten, TDR met gating, Touchstone-export en -import, CLI met dezelfde functies.

[Unreleased]: https://github.com/pa1bh/vna-r/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.2
[0.2.1]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.1
[0.2.0]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.0
[0.1.0]: https://github.com/pa1bh/vna-r/commit/e7c82a4
