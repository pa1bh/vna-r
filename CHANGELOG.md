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

### Opgelost
- GUI: **Set sweep for TDR** en een nieuw segment gingen altijd tot 6,3 GHz, ook op een
  NanoVNA-F (1,5 GHz); ze nemen nu de bovengrens van het verbonden apparaat.
- Een onzinnige schermafdruk-header (van een apparaat dat geen LiteVNA is of een gestoorde
  verbinding) kon tot 17 GB laten reserveren voordat er iets gelezen was; de afmetingen
  zijn nu begrensd op 4096×4096 en alleen 16 bits per pixel wordt geaccepteerd, wat de
  omzetting toch al aannam.
- CLI: `vna-cli tdr --csv -` schrijft naar stdout zoals de handleiding zegt (er ontstond
  een bestand met de naam `-`); het commentaar in een .s2p van `sweep` noemt het echte
  model in plaats van altijd "LiteVNA" en zegt "host calibration per segment" bij
  `--segment …@cal.json` in plaats van "uncalibrated raw"; de regel "first raw record: …"
  verschijnt alleen nog met `--trace`.
- GUI: de pijltjestoetsen verplaatsten de actieve marker ook terwijl je in Start, Stop of
  een referentienaam typte.
- GUI: markers blijven op hun frequentie staan als het raster verandert (ander bereik,
  ander aantal punten, een geopend bestand); voorheen bleven ze op hetzelfde puntnummer
  staan en wezen daardoor een andere frequentie aan. Een marker buiten het nieuwe bereik
  verdwijnt.
- GUI: een mislukte schermafdruk of een mislukte **set** van de middeling stopte de
  lopende sweep; alleen een sweep- of verbindingsfout doet dat nog.
- GUI: de knoppen open/short/load maten op het raster dat in Start/Stop/Points getypt
  stond, ook als dat nog niet met **Apply sweep** was toegepast; de weergave liep dan op
  het oude raster en de calibratie werd stil geïnterpoleerd. Een standaard meten past nu
  eerst de getypte sweep toe, en het paneel waarschuwt zolang de velden afwijken van de
  lopende sweep.
- GUI: de statusregel van het calibratiepaneel ("measured open", "loaded", "kit updated …
  save to keep it") werd direct na elke actie overschreven door het opnieuw oplossen van
  de calibratie en was daardoor met **apply** aan nooit te zien. De reden waarom een
  calibratie niet toegepast kan worden staat nu apart ("not applied: …").
- Calibratie-interpolatie naar een ander raster gebeurt nu polair (grootte en fase apart)
  in plaats van lineair in reëel en imaginair deel. Dat laatste snijdt de hoek tussen twee
  fasoren af en maakte alle standaarden kleiner met cos(θ/2) per fasestap θ, zodat elke
  gecorrigeerde |Γ| te hoog uitkwam: verwaarloosbaar bij 1001 punten, maar +1,3 dB bij
  11 punten met een lange kabel. De grens op de fasestap gaat van 90° naar 135° en geldt
  nu ook voor short, through, line open en line short, niet alleen voor de open.
- De Δ-trace tegen een referentie op een ander raster gebruikt dezelfde polaire
  interpolatie.
- TDR: het gemelde eenduidige bereik ("alias-free range") was 1/Δf, het dubbele van wat de
  grafiek toont; de arrays en de gate liepen al tot 1/(2·Δf). GUI-label, CLI-regel en de
  as-limiet gebruiken nu de juiste waarde, en de gate-foutmelding noemt die direct.
- Een poort zonder herkenbare USB-id die geen NanoVNA-shell blijkt te zijn kostte bij het
  verbinden twee keer de commando-timeout (eerst de prompt afwachten, dan bij het opruimen
  nog een `resume` sturen en weer wachten); nu één keer.

### Documentatie
- De TDR-schermafdruk in hoofdstuk 5 toont nu een short in plaats van een open: met de
  duik naar 0 Ω blijft de 50 Ω-lijn leesbaar, bij de open zat die onderin een as tot
  500 Ω.
- Schermafdruk van een weerstand van 25 Ω achter 5 m coax bij het kabelrecept in
  hoofdstuk 6: de impedantiegrafiek stapt van 50 naar 26 Ω.

## [0.2.5] - 2026-11-05

TDR-impedantie bij lange kabels, en twee nieuwe schermafdrukken in de handleidingen.

### Opgelost
- TDR: de impedantie langs de lijn lag bij kabels van meer dan een paar meter te laag en
  liep scheef (25 Ω voor een 50 Ω-kabel van 4 m). Het DC-punt van de stapresponsie werd
  uit de laagste meetpunten geëxtrapoleerd, wat faalt zodra S11 daarbinnen een volle
  cirkel draait; het volgt nu uit de eis dat er vóór het referentievlak niets reflecteert.
  Bij het herbemonsteren wordt S11 bovendien in grootte en fase geïnterpoleerd, zodat de
  eindpiek van een lange kabel niet meer te laag uitvalt.

### Documentatie
- Schermafdruk van een S21-meting (FM-notchfilter FLT201A/N) bij het filterrecept in
  beide handleidingen, en een TDR van een 5 m kabel met open eind in hoofdstuk 5.

## [0.2.4] - 2026-11-05

Downloads voor meer platforms.

### Toegevoegd
- Releases voor Intel-Mac (`x86_64-macos`) en Windows (`x86_64-windows`, zip) naast Apple
  Silicon en Linux; de CI bouwt en test nu ook op Windows. De Windows-build is nog niet
  met een apparaat geprobeerd. Op Windows opent de GUI geen consolevenster meer.
- README en handleiding leggen uit hoe je op macOS langs Gatekeeper komt (uitpakken in de
  Terminal of `xattr -dr com.apple.quarantine`) en op Windows langs SmartScreen.
- Issue-sjabloon op GitHub dat om de uitvoer van **Copy diagnostics** vraagt.

## [0.2.3] - 2026-11-05

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

[Unreleased]: https://github.com/pa1bh/vna-r/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.5
[0.2.4]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.4
[0.2.3]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.3
[0.2.2]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.2
[0.2.1]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.1
[0.2.0]: https://github.com/pa1bh/vna-r/releases/tag/v0.2.0
[0.1.0]: https://github.com/pa1bh/vna-r/commit/e7c82a4
